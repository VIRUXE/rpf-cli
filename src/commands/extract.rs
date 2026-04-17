use anyhow::{Context, Result};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use crate::rpf::RpfArchive;
use crate::crypto::GtaKeys;
use crate::utils::matches_pattern;

pub fn run(archive_path: &Path, output_dir: Option<&Path>, pattern: Option<&str>, keys: Option<&GtaKeys>) -> Result<()> {
    let archive = RpfArchive::open_with_keys(archive_path, keys)?;

    let output_path = match output_dir {
        Some(dir) => dir.to_path_buf(),
        None => PathBuf::from(archive_path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("extracted")),
    };

    fs::create_dir_all(&output_path)?;

    let files = archive.list_files();

    let mut dir_count = 0;
    for entry in &archive.entries {
        if matches!(entry, crate::rpf::RpfEntry::Directory(_)) {
            dir_count += 1;
        }
    }

    let total_items = files.len() + dir_count;

    println!("Archive contains {} items ({} files, {} directories)", total_items, files.len(), dir_count);

    let files_to_extract: Vec<_> = if let Some(pattern) = pattern {
        if !pattern.contains('*') && !pattern.contains('?') {
            if let Some(file) = archive.find_file(pattern) {
                vec![file]
            } else {
                println!("File not found: {}", pattern);
                return Ok(());
            }
        } else {
            files.into_iter().filter(|f| matches_pattern(&f.path, pattern)).collect()
        }
    } else {
        files
    };

    if files_to_extract.is_empty() {
        println!("No files found to extract");
        return Ok(());
    }

    println!("Extracting {} files...", files_to_extract.len());

    let mut extracted_count = 0;
    let mut failed_count = 0;

    for (i, file) in files_to_extract.iter().enumerate() {
        let file_output_path = output_path.join(&file.path);

        if let Some(parent) = file_output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        match extract_single_file(&archive, file, &file_output_path, keys) {
            Ok(_) => {
                extracted_count += 1;
                let progress = (i + 1) as f32 / files_to_extract.len() as f32;
                let bar_width = 30;
                let filled = (progress * bar_width as f32) as usize;
                let bar = if filled >= bar_width {
                    "=".repeat(bar_width)
                } else {
                    "=".repeat(filled) + ">" + &" ".repeat(bar_width - filled - 1)
                };
                let max_filename_len = 40;
                let display_name = if file.name.len() > max_filename_len {
                    format!("...{}", &file.name[file.name.len() - (max_filename_len - 3)..])
                } else {
                    file.name.clone()
                };
                print!("\r[{}] {}/{} {:<40}", bar, i + 1, files_to_extract.len(), display_name);
                io::stdout().flush().unwrap();
            }
            Err(e) => {
                eprintln!("Failed to extract {}: {:?}", file.path, e);
                failed_count += 1;
            }
        }
    }

    println!();
    println!("\nExtraction complete!");
    println!("Extracted: {} files", extracted_count);
    if failed_count > 0 { println!("Failed: {} files", failed_count); }

    Ok(())
}

fn extract_single_file(archive: &RpfArchive, file: &crate::rpf::RpfFileEntry, output_path: &Path, keys: Option<&GtaKeys>) -> Result<()> {
    let data = archive.extract_file_with_keys(file, keys)
        .with_context(|| format!("Failed to extract file: {}", file.path))?;

    fs::write(output_path, data).with_context(|| format!("Failed to write file: {}", output_path.display()))?;

    Ok(())
}
