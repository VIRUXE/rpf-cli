use anyhow::Result;
use std::path::Path;
use crate::rpf::RpfArchive;
use crate::utils::matches_pattern;

pub fn run(archive_path: &Path, pattern: Option<&str>, detailed: bool) -> Result<()> {
    let archive = RpfArchive::open(archive_path)?;
    let files   = archive.list_files();
    
    // Filter files if pattern is provided
    let filtered_files: Vec<_> = if let Some(pattern) = pattern {
        files.into_iter()
            .filter(|f| matches_pattern(&f.path, pattern))
            .collect()
    } else {
        files
    };
    
    if filtered_files.is_empty() {
        println!("No files found matching the pattern");
        return Ok(());
    }
    
    // Sort files by path
    let mut sorted_files = filtered_files;
    sorted_files.sort_by(|a, b| a.path.cmp(&b.path));
    
    if detailed {
        println!("{:<60} {:>12} {:>12} {}", "Path", "Size", "Compressed", "Type");
        println!("{}", "-".repeat(100));
        
        for file in sorted_files {
            let file_type = if file.is_resource { "Resource" } else { "Binary" };
            println!("{:<60} {:>12} {:>12} {}", file.path, file.uncompressed_size,file.size,file_type);
        }
    } else {
        for file in sorted_files {
            println!("{}", file.path);
        }
    }
    
    Ok(())
} 