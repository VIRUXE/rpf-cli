use anyhow::Result;
use std::path::Path;
use crate::rpf::{Archive, GtaKeys};
use crate::utils::matches_pattern;

pub fn run(archive_path: &Path, pattern: Option<&str>, detailed: bool, keys: Option<&GtaKeys>) -> Result<()> {
    let archive = Archive::open(archive_path, keys)?;

    let mut files: Vec<_> = archive.list_files()
        .into_iter()
        .filter(|f| pattern.map_or(true, |p| matches_pattern(&f.path, p)))
        .collect();

    if files.is_empty() {
        println!("No files found");
        return Ok(());
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));

    if detailed {
        println!("{:<60} {:>12} {:>12} {}", "Path", "Size", "Compressed", "Type");
        println!("{}", "-".repeat(100));
        for f in files {
            let kind = if f.is_resource { "Resource" } else { "Binary" };
            println!("{:<60} {:>12} {:>12} {}", f.path, f.mem_size, f.size, kind);
        }
    } else {
        for f in files {
            println!("{}", f.path);
        }
    }

    Ok(())
}
