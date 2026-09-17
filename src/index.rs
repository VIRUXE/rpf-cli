// A cached, game-wide index that lets `screenshot` resolve a drawable's
// external texture dictionary the way the game itself does, instead of the
// single same-stem guess in `resources::load_texture_dictionary`.
//
// This follows CodeWalker's `Renderer.TryGetRenderable` resolution order: an
// archetype's `.ytyp` names a texture dictionary by hash, that hash (and its
// parent-dictionary chain, from `gtxd.meta`/`gtxd.ymt`/`mph4_gtxd.ymt`/
// `vehicles.meta`) is resolved to `.ytd`s by a game-wide name index, and a
// texture still missing after all of that falls back to the two "resident"
// dictionaries the game always keeps loaded (`mapdetail.ytd`, `vehshare.ytd`).

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use rpf_archive::{parse_archetype_txds, parse_txd_relationships, parse_ytd, rage_joaat};

use crate::commands::search::collect_archives;
use crate::keys;
use crate::rpf::{Archive, GtaKeys};

/// Where a file lives: a top-level `.rpf` on disk, then zero or more nested
/// `.rpf` entries to descend through, then the entry itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryLoc {
    pub top_archive: PathBuf,
    pub nested_rpfs: Vec<String>,
    pub inner_path: String,
}

/// The built index: enough to resolve `<archetype name hash>` (a drawable's
/// file stem) to the `.ytd` bytes that hold its textures.
#[derive(Debug, Default)]
pub struct GameIndex {
    /// `.ytd` stem hash -> where that dictionary lives. Later archives win
    /// (last write), matching the game's own DLC-overrides-base ordering.
    pub ytd_by_name: HashMap<u32, EntryLoc>,
    /// Archetype/model name hash -> its `textureDictionary` hash, from every
    /// `.ytyp`. Later `.ytyp`s win.
    pub archetype_txd: HashMap<u32, u32>,
    /// Texture name hash -> the name hash of the resident dictionary
    /// (`mapdetail`/`vehshare`) that holds it.
    pub resident_textures: HashMap<u32, u32>,
    /// Child texture-dictionary hash -> parent texture-dictionary hash, from
    /// every `gtxd.meta`/`gtxd.ymt`/`mph4_gtxd.ymt`/`vehicles.meta`. First
    /// relationship for a given child wins, matching CodeWalker's own
    /// first-wins merge (see `merge_txd_relationships`).
    pub parent_txds: HashMap<u32, u32>,
}

const RESIDENT_DICTS: [&str; 2] = ["mapdetail", "vehshare"];

/// CodeWalker's `GameFileCache.InitGtxds` matches these names exactly
/// (`entry.NameLower == "..."`), not as a suffix — a filename like
/// `dlc_gtxd.ymt` is deliberately not picked up, matching the reference.
const TXD_RELATIONSHIP_FILES: [&str; 4] = ["gtxd.ymt", "gtxd.meta", "mph4_gtxd.ymt", "vehicles.meta"];

impl GameIndex {
    /// Builds the index by walking every `.rpf` under `game_root`,
    /// descending into nested archives. This decompresses every `.ytyp` and
    /// the two resident dictionaries, but only reads the directory listing
    /// of everything else — no other `.ytd` is decoded.
    pub fn build(game_root: &Path, keys: Option<&GtaKeys>) -> Result<Self> {
        let archives = collect_archives(game_root)?;
        if archives.is_empty() {
            bail!("no .rpf archives found under {}", game_root.display());
        }

        let mut index = GameIndex::default();
        for archive_path in &archives {
            let archive = match Archive::open(archive_path, keys) {
                Ok(a) => a,
                Err(e) => { eprintln!("index: skipping {}: {}", archive_path.display(), e); continue; }
            };
            if archive.require_keys(keys).is_err() {
                eprintln!("index: skipping {} (needs keys)", archive_path.display());
                continue;
            }
            index_archive(&archive, archive_path, &[], keys, &mut index);
        }

        Ok(index)
    }

    /// Reads the raw bytes of an already-located entry, descending through
    /// any nested archives on the way.
    pub fn load_bytes(&self, loc: &EntryLoc, keys: Option<&GtaKeys>) -> Result<Vec<u8>> {
        let mut archive = Archive::open(&loc.top_archive, keys)?;
        archive.require_keys(keys)?;

        for nested_path in &loc.nested_rpfs {
            let file = archive
                .find_file(nested_path)
                .with_context(|| format!("'{}' not found in '{}'", nested_path, loc.top_archive.display()))?;
            // `Archive::from_bytes`'s `name` isn't cosmetic: RPF7's NG
            // decryption selects its per-archive key from this name (and
            // the TOC length), so it must be the archive's own bare file
            // name — the same value `file.name` already is elsewhere in
            // this crate (`search_recursive`, `extract_recursive`) — not
            // its full path within the parent, or the TOC decrypts to
            // garbage and every entry name comes back as a placeholder.
            let bare_name = file.name.clone();
            let data = archive.extract(file, keys)
                .with_context(|| format!("failed to extract nested archive '{}'", nested_path))?;
            archive = Archive::from_bytes(data, &bare_name, keys)?;
        }

        let file = archive
            .find_file(&loc.inner_path)
            .with_context(|| format!("'{}' not found", loc.inner_path))?;
        archive.extract(file, keys).with_context(|| format!("failed to extract '{}'", loc.inner_path))
    }

    /// Every `.ytd` layer `screenshot` should try, in CodeWalker's order,
    /// for a drawable named `file_stem` and (if known) its archetype hash:
    /// the archetype's own texture dictionary and its full parent chain,
    /// then the drawable's own stem hash and *its* parent chain as the
    /// same-name guess this has always made. Names, not yet loaded — the
    /// caller loads and parses only the ones it still needs.
    ///
    /// Each candidate's parent chain is walked in full before the next
    /// candidate starts (CodeWalker's `Renderer.cs TryGetRenderable` builds
    /// exactly this array — `[own txd, parent, grandparent, ...]` — for the
    /// archetype's resolved dictionary). A single shared `seen` set is what
    /// keeps this a cycle guard rather than a repeat of the same chain:
    /// CodeWalker has no such guard anywhere in this walk.
    pub fn resolution_order(&self, file_stem_hash: u32) -> Vec<u32> {
        const MAX_HOPS: usize = 64;

        // The archetype hash is usually just the drawable's own file stem
        // (CodeWalker's `ModelForm.cs` fallback for a model with no known
        // archetype): try that hash's texture dictionary directly, and also
        // check whether an archetype named it explicitly.
        let mut starts = Vec::with_capacity(2);
        if let Some(&txd_hash) = self.archetype_txd.get(&file_stem_hash)
            && txd_hash != 0
        {
            starts.push(txd_hash);
        }
        starts.push(file_stem_hash);

        let mut seen = std::collections::HashSet::new();
        let mut order = Vec::new();

        for start in starts {
            let mut current = start;
            for _ in 0..=MAX_HOPS {
                if !seen.insert(current) {
                    break; // cycle, or already covered by an earlier chain
                }
                order.push(current);
                let Some(&parent) = self.parent_txds.get(&current) else { break };
                current = parent;
            }
        }

        order
    }

    /// Looks up which resident dictionary (`mapdetail`/`vehshare`) carries a
    /// texture named `texture_name_hash`, if either does. Used by
    /// `screenshot`'s resident-dictionary fallback once rendering has
    /// reported a texture name still missing after the whole chain.
    pub fn resident_dict_for_texture(&self, texture_name_hash: u32) -> Option<u32> {
        self.resident_textures.get(&texture_name_hash).copied()
    }

    // ─── On-disk cache ──────────────────────────────────────────────────

    /// `~/.rpf-cli/index/<game build>/index.bin`, keyed the same way as the
    /// key cache (`keys::cache_entry_name`) so a game update never serves a
    /// stale index.
    pub fn cache_path(exe_path: &Path) -> Option<PathBuf> {
        let root = crate::paths::config_root()?.join("index");
        let meta = std::fs::metadata(exe_path).ok()?;
        let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
        Some(root.join(keys::cache_entry_name(meta.len(), modified)).join("index.bin"))
    }

    pub fn load_cached(path: &Path) -> Result<Self> {
        let mut data = Vec::new();
        std::fs::File::open(path)?.read_to_end(&mut data)?;
        decode(&data)
    }

    pub fn save_cached(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::File::create(path)?.write_all(&encode(self))?;
        Ok(())
    }

    pub fn stats(&self) -> IndexStats {
        IndexStats {
            ytds: self.ytd_by_name.len(),
            archetypes: self.archetype_txd.len(),
            resident_textures: self.resident_textures.len(),
            parent_txds: self.parent_txds.len(),
        }
    }

    /// The one-line human summary shared by `rpf index build`/`info` and
    /// the on-demand build in `screenshot`.
    pub fn summary(&self) -> String {
        let s = self.stats();
        format!(
            "{} dictionaries, {} archetypes, {} resident textures, {} txd parent links",
            s.ytds, s.archetypes, s.resident_textures, s.parent_txds
        )
    }
}

/// Sizes of each map in a [`GameIndex`], for `rpf index build`/`info` and
/// the on-demand build notice in `screenshot`.
#[derive(Debug, Clone, Copy, Default)]
pub struct IndexStats {
    pub ytds: usize,
    pub archetypes: usize,
    pub resident_textures: usize,
    pub parent_txds: usize,
}

fn index_archive(archive: &Archive, archive_path: &Path, nested_rpfs: &[String], keys: Option<&GtaKeys>, out: &mut GameIndex) {
    for file in archive.list_files() {
        let name_lower = file.name.to_lowercase();

        if name_lower.ends_with(".rpf") {
            let Ok(data) = archive.extract(file, keys) else { continue };
            let Ok(nested) = Archive::from_bytes(data, &file.name, keys) else { continue };
            let mut chain = nested_rpfs.to_vec();
            chain.push(file.path.clone());
            index_archive(&nested, archive_path, &chain, keys, out);
            continue;
        }

        if TXD_RELATIONSHIP_FILES.contains(&name_lower.as_str()) {
            match archive.extract(file, keys) {
                Ok(data) => match parse_txd_relationships(&data) {
                    Ok(rels) => merge_txd_relationships(&mut out.parent_txds, &rels),
                    Err(err) => log::debug!("index: failed to parse '{}': {err}", file.path),
                },
                Err(err) => log::debug!("index: failed to extract '{}': {err}", file.path),
            }
            continue;
        }

        let stem = crate::resources::file_stem(&name_lower);

        if name_lower.ends_with(".ytd") {
            out.ytd_by_name.insert(rage_joaat(&stem), EntryLoc {
                top_archive: archive_path.to_path_buf(),
                nested_rpfs: nested_rpfs.to_vec(),
                inner_path: file.path.clone(),
            });

            if RESIDENT_DICTS.contains(&stem.as_str())
                && let Ok(data) = archive.extract(file, keys)
                && let Ok(textures) = parse_ytd(&data)
            {
                let owner_hash = rage_joaat(&stem);
                for tex in textures {
                    // The stored hash block falls back to 0 when absent, so
                    // this is indexed under both that hash and the hashed
                    // lowercase name to make the lookup unmissable — the
                    // caller (a still-missing texture name, post-render)
                    // only ever has the name.
                    if tex.name_hash != 0 {
                        out.resident_textures.insert(tex.name_hash, owner_hash);
                    }
                    out.resident_textures.insert(rage_joaat(&tex.name.to_lowercase()), owner_hash);
                }
            }
        } else if name_lower.ends_with(".ytyp")
            && let Ok(data) = archive.extract(file, keys)
            && let Ok(archetypes) = parse_archetype_txds(&data)
        {
            for a in archetypes {
                if a.texture_dict_hash != 0 {
                    out.archetype_txd.insert(a.name_hash, a.texture_dict_hash);
                }
            }
        }
    }
}

/// Merges `rels` into `out`, first child-wins — matching CodeWalker's
/// `addTxdRelationships` (`GameFileCache.cs`: `if (!parentTxds.ContainsKey(chash))`)
/// and each individual file's own parse. Combined with base-before-DLC scan
/// order this would mean the base game's relationship beats a DLC's; today's
/// archive scan order is alphabetical rather than base/update/DLC-ranked
/// (tracked separately), so this only guarantees a stable pick, not
/// necessarily the game's own.
fn merge_txd_relationships(out: &mut HashMap<u32, u32>, rels: &[rpf_archive::TxdRelationship]) {
    for rel in rels {
        let child = rage_joaat(&rel.child.to_lowercase());
        let parent = rage_joaat(&rel.parent.to_lowercase());
        if child == 0 || parent == 0 || child == parent {
            continue;
        }
        out.entry(child).or_insert(parent);
    }
}

// ─── Minimal binary (de)serialization — no serde dependency for one struct ──

const MAGIC: u32 = 0x5850_4652; // "RPFX" little-endian
// Bumped from 1: the on-disk layout is unchanged (the parent_txds section
// already existed and was already last), but the cache path is keyed only
// on the GTA5 executable's size+mtime, not on any tool version — so without
// a bump, every existing cache (written with an always-empty parent_txds)
// would be served straight to the new resolver and the parent chain would
// be silently inert until someone ran `rpf index clear`.
const FORMAT_VERSION: u32 = 2;

fn write_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_str(buf: &mut Vec<u8>, s: &str) {
    write_u32(buf, s.len() as u32);
    buf.extend_from_slice(s.as_bytes());
}

fn encode(index: &GameIndex) -> Vec<u8> {
    let mut buf = Vec::new();
    write_u32(&mut buf, MAGIC);
    write_u32(&mut buf, FORMAT_VERSION);

    write_u32(&mut buf, index.ytd_by_name.len() as u32);
    for (hash, loc) in &index.ytd_by_name {
        write_u32(&mut buf, *hash);
        write_str(&mut buf, &loc.top_archive.to_string_lossy());
        write_u32(&mut buf, loc.nested_rpfs.len() as u32);
        for n in &loc.nested_rpfs {
            write_str(&mut buf, n);
        }
        write_str(&mut buf, &loc.inner_path);
    }

    write_u32(&mut buf, index.archetype_txd.len() as u32);
    for (k, v) in &index.archetype_txd {
        write_u32(&mut buf, *k);
        write_u32(&mut buf, *v);
    }

    write_u32(&mut buf, index.resident_textures.len() as u32);
    for (k, v) in &index.resident_textures {
        write_u32(&mut buf, *k);
        write_u32(&mut buf, *v);
    }

    write_u32(&mut buf, index.parent_txds.len() as u32);
    for (k, v) in &index.parent_txds {
        write_u32(&mut buf, *k);
        write_u32(&mut buf, *v);
    }

    buf
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn u32(&mut self) -> Result<u32> {
        let bytes = self.data.get(self.pos..self.pos + 4).context("index: truncated (u32)")?;
        self.pos += 4;
        Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn string(&mut self) -> Result<String> {
        let len = self.u32()? as usize;
        let bytes = self.data.get(self.pos..self.pos + len).context("index: truncated (string)")?;
        self.pos += len;
        String::from_utf8(bytes.to_vec()).context("index: invalid UTF-8")
    }
}

fn decode(data: &[u8]) -> Result<GameIndex> {
    let mut c = Cursor { data, pos: 0 };
    if c.u32()? != MAGIC {
        bail!("index: bad magic");
    }
    let version = c.u32()?;
    if version != FORMAT_VERSION {
        bail!("index: unsupported format version {version}");
    }

    let mut index = GameIndex::default();

    let ytd_count = c.u32()? as usize;
    for _ in 0..ytd_count {
        let hash = c.u32()?;
        let top_archive = PathBuf::from(c.string()?);
        let nested_count = c.u32()? as usize;
        let mut nested_rpfs = Vec::with_capacity(nested_count);
        for _ in 0..nested_count {
            nested_rpfs.push(c.string()?);
        }
        let inner_path = c.string()?;
        index.ytd_by_name.insert(hash, EntryLoc { top_archive, nested_rpfs, inner_path });
    }

    let archetype_count = c.u32()? as usize;
    for _ in 0..archetype_count {
        let k = c.u32()?;
        let v = c.u32()?;
        index.archetype_txd.insert(k, v);
    }

    let resident_count = c.u32()? as usize;
    for _ in 0..resident_count {
        let k = c.u32()?;
        let v = c.u32()?;
        index.resident_textures.insert(k, v);
    }

    let parent_count = c.u32()? as usize;
    for _ in 0..parent_count {
        let k = c.u32()?;
        let v = c.u32()?;
        index.parent_txds.insert(k, v);
    }

    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_encode_decode() {
        let mut index = GameIndex::default();
        index.ytd_by_name.insert(1, EntryLoc {
            top_archive: PathBuf::from("C:/game/x64f.rpf"),
            nested_rpfs: vec!["levels/gta5/x.rpf".to_string()],
            inner_path: "prop_table_chair_02.ytd".to_string(),
        });
        index.archetype_txd.insert(2, 3);
        index.resident_textures.insert(4, 5);
        index.parent_txds.insert(6, 7);

        let bytes = encode(&index);
        let decoded = decode(&bytes).expect("should decode");

        assert_eq!(decoded.ytd_by_name.get(&1), index.ytd_by_name.get(&1));
        assert_eq!(decoded.archetype_txd, index.archetype_txd);
        assert_eq!(decoded.resident_textures, index.resident_textures);
        assert_eq!(decoded.parent_txds, index.parent_txds);
    }

    #[test]
    fn rejects_bad_magic() {
        let bytes = vec![0u8; 8];
        assert!(decode(&bytes).is_err());
    }

    #[test]
    fn rejects_old_format_version() {
        let mut bytes = Vec::new();
        write_u32(&mut bytes, MAGIC);
        write_u32(&mut bytes, 1); // the pre-parent-chain format
        assert!(decode(&bytes).is_err());
    }

    #[test]
    fn merge_txd_relationships_is_first_wins() {
        let mut out = HashMap::new();
        merge_txd_relationships(&mut out, &[rel("a", "b")]);
        merge_txd_relationships(&mut out, &[rel("a", "c")]);
        assert_eq!(out.get(&rage_joaat("a")), Some(&rage_joaat("b")));
    }

    #[test]
    fn merge_txd_relationships_lowercases_before_hashing() {
        let mut out = HashMap::new();
        merge_txd_relationships(&mut out, &[rel("PROP_Foo", "VehShare")]);
        assert_eq!(out.get(&rage_joaat("prop_foo")), Some(&rage_joaat("vehshare")));
    }

    #[test]
    fn merge_txd_relationships_rejects_self_parent() {
        let mut out = HashMap::new();
        merge_txd_relationships(&mut out, &[rel("same", "same")]);
        assert!(out.is_empty());
    }

    fn rel(child: &str, parent: &str) -> rpf_archive::TxdRelationship {
        rpf_archive::TxdRelationship { child: child.to_string(), parent: parent.to_string() }
    }

    #[test]
    fn resolution_order_tries_archetype_txd_then_own_stem_hash() {
        let mut index = GameIndex::default();
        index.archetype_txd.insert(100, 200);

        let order = index.resolution_order(100);
        assert_eq!(order, vec![200, 100]);

        // No archetype known: falls back to the stem hash alone.
        let order = index.resolution_order(999);
        assert_eq!(order, vec![999]);
    }

    #[test]
    fn resolution_order_walks_parent_chain_with_cycle_guard() {
        let mut index = GameIndex::default();
        index.parent_txds.insert(1, 2);
        index.parent_txds.insert(2, 1); // cycle

        let order = index.resolution_order(1);
        // 1 (self), 2 (parent), then the cycle back to 1 is rejected.
        assert_eq!(order, vec![1, 2]);
    }

    #[test]
    fn resolution_order_walks_the_archetype_txds_chain_not_the_stems() {
        // Regression test: the chain must be walked from the archetype's
        // resolved dictionary (200), not from the stem hash (100) that
        // happens to be pushed last — a prior version seeded the walk from
        // `order.last()` and so never found this parent at all.
        let mut index = GameIndex::default();
        index.archetype_txd.insert(100, 200);
        index.parent_txds.insert(200, 300);

        let order = index.resolution_order(100);
        assert_eq!(order, vec![200, 300, 100]);
    }

    #[test]
    fn resolution_order_merges_both_chains_without_duplicates() {
        let mut index = GameIndex::default();
        index.archetype_txd.insert(100, 200);
        index.parent_txds.insert(200, 400);
        index.parent_txds.insert(100, 400); // same parent as the archetype chain

        let order = index.resolution_order(100);
        assert_eq!(order, vec![200, 400, 100]);
    }

    #[test]
    fn resolution_order_stops_at_the_hop_limit() {
        let mut index = GameIndex::default();
        for i in 0..200u32 {
            index.parent_txds.insert(i, i + 1);
        }

        let order = index.resolution_order(0);
        assert!(order.len() <= 65, "expected the walk to stop at the hop limit, got {} entries", order.len());
    }
}
