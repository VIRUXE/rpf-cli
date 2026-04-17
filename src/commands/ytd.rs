use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use rage_rpf::{parse_ytd, RpfEntryKind};

use crate::rpf::{Archive, GtaKeys};

/// Extract all textures from a .ytd file inside an RPF archive as DDS files.
pub fn run(
    archive_path: &Path,
    ytd_name: &str,
    output_dir: Option<&Path>,
    keys: Option<&GtaKeys>,
) -> Result<()> {
    let archive = Archive::open(archive_path, keys)?;

    let name_lower = ytd_name.to_lowercase();
    let file_ref = archive
        .find_file(&name_lower)
        .with_context(|| format!("'{}' not found in archive", ytd_name))?;

    if !matches!(archive.entry_kind(file_ref), RpfEntryKind::ResourceFile { .. }) {
        anyhow::bail!("'{}' is not a resource file", ytd_name);
    }

    let rsc7_data = archive
        .extract(file_ref, keys)
        .with_context(|| format!("failed to extract '{}'", ytd_name))?;

    let textures = parse_ytd(&rsc7_data)
        .with_context(|| format!("failed to parse YTD '{}'", ytd_name))?;

    if textures.is_empty() {
        eprintln!("No textures found in '{}'", ytd_name);
        return Ok(());
    }

    let stem = ytd_name.trim_end_matches(".ytd");
    let out_dir = output_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(stem));

    std::fs::create_dir_all(&out_dir)?;

    for tex in &textures {
        let tex_name = if tex.name.is_empty() {
            format!("0x{:08X}", tex.name_hash)
        } else {
            tex.name.clone()
        };

        let dds_path = out_dir.join(format!("{}.dds", tex_name));
        let dds_data = tex.to_dds();
        std::fs::write(&dds_path, &dds_data)
            .with_context(|| format!("failed to write {}", dds_path.display()))?;

        println!(
            "  {} — {}x{}x{} {} {} mip(s) ({} bytes)",
            tex_name,
            tex.width, tex.height, tex.depth,
            tex.format,
            tex.levels,
            tex.pixel_data.len(),
        );
    }

    println!("Extracted {} texture(s) to {}", textures.len(), out_dir.display());
    Ok(())
}
