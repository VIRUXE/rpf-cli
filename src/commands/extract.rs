use anyhow::{Context, Result};
use std::{fs, io::{self, Write}, path::{Path, PathBuf}};
use crate::rpf::{Archive, FileRef, GtaKeys};
use crate::utils::matches_pattern;

pub fn run(archive_path: &Path, output_dir: Option<&Path>, pattern: Option<&str>, keys: Option<&GtaKeys>) -> Result<()> {
    let archive = Archive::open(archive_path, keys)?;

    let output_path = output_dir.map(Path::to_path_buf).unwrap_or_else(|| {
        PathBuf::from(archive_path.file_stem().and_then(|s| s.to_str()).unwrap_or("extracted"))
    });
    fs::create_dir_all(&output_path)?;

    println!("Archive contains {} entries ({} dirs, {} files)",
        archive.entry_count, archive.dir_count, archive.entry_count - archive.dir_count);

    let all_files = archive.list_files();
    let to_extract: Vec<&FileRef> = if let Some(pat) = pattern {
        if !pat.contains('*') && !pat.contains('?') {
            match archive.find_file(pat) {
                Some(f) => vec![f],
                None    => { println!("File not found: {}", pat); return Ok(()); }
            }
        } else {
            all_files.into_iter().filter(|f| matches_pattern(&f.path, pat)).collect()
        }
    } else {
        all_files
    };

    if to_extract.is_empty() {
        println!("No files to extract");
        return Ok(());
    }

    println!("Extracting {} files...", to_extract.len());

    let total = to_extract.len();
    let mut ok = 0usize;
    let mut fail = 0usize;

    for (i, file) in to_extract.iter().enumerate() {
        let dest = output_path.join(&file.path);
        if let Some(parent) = dest.parent() { fs::create_dir_all(parent)?; }

        match archive.extract(file, keys) {
            Ok(data) => {
                fs::write(&dest, &data)
                    .with_context(|| format!("Write failed: {}", dest.display()))?;
                ok += 1;
                print_progress(i + 1, total, &file.name);
            }
            Err(e) => {
                eprintln!("\nFailed to extract {}: {}", file.path, e);
                fail += 1;
            }
        }
    }

    println!("\n\nExtracted: {}  Failed: {}", ok, fail);
    Ok(())
}

fn print_progress(n: usize, total: usize, name: &str) {
    let pct = n as f32 / total as f32;
    let filled = (pct * 30.0) as usize;
    let bar = if filled >= 30 {
        "=".repeat(30)
    } else {
        format!("{}>{}",  "=".repeat(filled), " ".repeat(29 - filled))
    };
    let label = if name.len() > 40 { format!("...{}", &name[name.len()-37..]) } else { name.to_string() };
    print!("\r[{}] {}/{} {:<40}", bar, n, total, label);
    io::stdout().flush().ok();
}
