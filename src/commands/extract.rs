use anyhow::{Context, Result};
use std::fs;
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
    
    for file in files_to_extract {
        let file_output_path = output_path.join(&file.path);
        
        // Create parent directories
        if let Some(parent) = file_output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        match extract_single_file(&archive, file, &file_output_path) {
            Ok(_) => {
                extracted_count += 1;
                if extracted_count % 100 == 0 { println!("Extracted {} files...", extracted_count); }
            }
            Err(e) => {
                eprintln!("Failed to extract {}: {:?}", file.path, e);
                failed_count += 1;
            }
        }
    }
    
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