use anyhow::{Context, Result};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use crate::rpf::RpfArchive;
use crate::utils::matches_pattern;

pub fn run(archive_path: &Path, output_dir: Option<&Path>, pattern: Option<&str>) -> Result<()> {
    let archive = RpfArchive::open(archive_path)?;
    
    // Determine output directory: use provided one or create from archive name
    let output_path = match output_dir {
        Some(dir) => dir.to_path_buf(),
        None => {
            // Use archive name without extension as directory name
            PathBuf::from(archive_path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("extracted"))
        }
    };
    
    // Create output directory if it doesn't exist
    fs::create_dir_all(&output_path)?;
    
    let files = archive.list_files();
    
    // Count directories in the archive
    let mut dir_count = 0;
    for entry in &archive.entries {
        if matches!(entry, crate::rpf::RpfEntry::Directory(_)) {
            dir_count += 1;
        }
    }
    
    let total_items = files.len() + dir_count;
    
    println!("Archive contains {} items ({} files, {} directories)", total_items, files.len(), dir_count);
    
    // Filter files if pattern is provided
    let files_to_extract: Vec<_> = if let Some(pattern) = pattern {
        // Check if pattern is a specific file path
        if !pattern.contains('*') && !pattern.contains('?') {
            // Try to find exact file
            if let Some(file) = archive.find_file(pattern) {
                vec![file]
            } else {
                println!("File not found: {}", pattern);
                return Ok(());
            }
        } else {
            // Pattern matching
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
        
        // Create parent directories
        if let Some(parent) = file_output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        match extract_single_file(&archive, file, &file_output_path) {
            Ok(_) => {
                extracted_count += 1;
                // Progress bar: [=====>     ] 45/137 filename.ext
                let progress = (i + 1) as f32 / files_to_extract.len() as f32;
                let bar_width = 30;
                let filled = (progress * bar_width as f32) as usize;
                let bar = if filled >= bar_width {
                    "=".repeat(bar_width)
                } else {
                    "=".repeat(filled) + ">" + &" ".repeat(bar_width - filled - 1)
                };
                // Truncate filename if too long to keep progress on one line
                let max_filename_len = 40;
                let display_name = if file.name.len() > max_filename_len {
                    format!("...{}", &file.name[file.name.len() - (max_filename_len - 3)..])
                } else {
                    file.name.clone()
                };
                // Pad the line to clear any leftover characters from previous longer filenames
                print!("\r[{}] {}/{} {:<40}", bar, i + 1, files_to_extract.len(), display_name);
                io::stdout().flush().unwrap();
            }
            Err(e) => {
                eprintln!("Failed to extract {}: {:?}", file.path, e);
                failed_count += 1;
            }
        }
    }
    
    println!(); // New line after progress bar
    println!("\nExtraction complete!");
    println!("Extracted: {} files", extracted_count);
    if failed_count > 0 { println!("Failed: {} files", failed_count); }
    
    Ok(())
}

fn extract_single_file(archive: &RpfArchive, file: &crate::rpf::RpfFileEntry, output_path: &Path) -> Result<()> {
    let data = archive.extract_file(file).with_context(|| format!("Failed to extract file: {}", file.path))?;
    
    fs::write(output_path, data).with_context(|| format!("Failed to write file: {}", output_path.display()))?;
    
    Ok(())
}