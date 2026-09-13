// Shared helpers for commands that pull renderable resources (textures,
// drawables) out of an RPF archive or a loose file on disk.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use rpf_archive::{parse_drawables, parse_ytd, DrawableEntry, DrawableKind, RpfEntryKind, YtdTexture};

use crate::rpf::{Archive, GtaKeys};

/// Finds `name` inside `archive` (case-insensitive name or path), extracts it
/// and returns its raw bytes. The entry must be a resource file (the kind
/// used by .ytd/.ydr/.ydd/.yft).
pub fn load_resource(archive: &Archive, name: &str, keys: Option<&GtaKeys>) -> Result<Vec<u8>> {
    let name_lower = name.to_lowercase();
    let file_ref = archive.find_file(&name_lower).with_context(|| {
        format!(
            "'{}' not found in archive (retail x64*.rpf keep drawables in nested .rpf files — run `rpf search <archive> <name>` to find which one, then extract it)",
            name
        )
    })?;

    if !matches!(archive.entry_kind(file_ref), RpfEntryKind::ResourceFile { .. }) {
        anyhow::bail!("'{}' is not a resource file", name);
    }

    archive
        .extract(file_ref, keys)
        .with_context(|| format!("failed to extract '{}'", name))
}

/// Loads and parses `name` (a .ydr, .ydd or .yft) from `archive` into its
/// list of drawable entries.
pub fn load_drawables(archive: &Archive, name: &str, keys: Option<&GtaKeys>) -> Result<Vec<DrawableEntry>> {
    let ext = Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or("");
    let kind = DrawableKind::from_extension(ext)
        .with_context(|| format!("'{}' is not a drawable (.ydr/.ydd/.yft)", name))?;

    let data = load_resource(archive, name, keys)?;
    parse_drawables(&data, kind).with_context(|| format!("failed to parse drawable '{}'", name))
}

/// Normalises a `--file`-style spec into the lookup name used against an
/// archive: appends ".ytd" when the spec carries no extension of its own.
pub fn ytd_lookup_name(spec: &str) -> String {
    if Path::new(spec).extension().is_some() {
        spec.to_string()
    } else {
        format!("{spec}.ytd")
    }
}

/// Loads a texture dictionary either from a loose RSC7 file on disk (when
/// `spec` names an existing file) or from `archive` (appending ".ytd" when
/// `spec` has no extension).
pub fn load_texture_dictionary(archive: &Archive, spec: &str, keys: Option<&GtaKeys>) -> Result<Vec<YtdTexture>> {
    if Path::new(spec).is_file() {
        let data = fs::read(spec).with_context(|| format!("failed to read '{}'", spec))?;
        return parse_ytd(&data).with_context(|| format!("failed to parse YTD '{}'", spec));
    }

    let lookup_name = ytd_lookup_name(spec);
    let data = load_resource(archive, &lookup_name, keys)?;
    parse_ytd(&data).with_context(|| format!("failed to parse YTD '{}'", lookup_name))
}

/// Collects every texture referenced by the drawables' shader groups,
/// deduplicated by lowercase name (first occurrence wins), in encounter order.
pub fn embedded_textures(entries: &[DrawableEntry]) -> Vec<&YtdTexture> {
    let mut seen = std::collections::HashSet::new();
    let mut textures = Vec::new();

    for entry in entries {
        let Some(shader_group) = &entry.drawable.shader_group else { continue };
        for tex in &shader_group.textures {
            if seen.insert(tex.name.to_lowercase()) {
                textures.push(tex);
            }
        }
    }

    textures
}

/// Reduces a name to characters that are safe in a file name: a crafted name
/// containing `..`/`/` must not be able to escape the destination directory.
pub fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();

    if cleaned.is_empty() { "entry".to_string() } else { cleaned }
}

/// The file stem of a (possibly archive-relative) path: `"a/b/prop_x.ydr"` ->
/// `"prop_x"`.
pub fn file_stem(name: &str) -> String {
    Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_stem_strips_dirs_and_extension() {
        assert_eq!(file_stem("a/b/prop_x.ydr"), "prop_x");
        assert_eq!(file_stem("prop_x.ydr"), "prop_x");
        assert_eq!(file_stem("prop_x"), "prop_x");
        assert_eq!(file_stem("a\\b\\vehicles.ytd"), "vehicles");
    }

    #[test]
    fn sanitize_strips_path_traversal_characters() {
        assert_eq!(sanitize("../../etc/passwd"), "______etc_passwd");
        assert_eq!(sanitize("prop_x"), "prop_x");
        assert_eq!(sanitize(""), "entry");
    }

    #[test]
    fn ytd_lookup_name_appends_extension_when_missing() {
        assert_eq!(ytd_lookup_name("vehicles"), "vehicles.ytd");
        assert_eq!(ytd_lookup_name("vehicles.ytd"), "vehicles.ytd");
        assert_eq!(ytd_lookup_name("path/to/vehicles"), "path/to/vehicles.ytd");
        assert_eq!(ytd_lookup_name("weird.name.ytd"), "weird.name.ytd");
    }
}
