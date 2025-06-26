use anyhow::Result;
use std::path::Path;
use crate::rpf::{RpfArchive, RpfDirectoryEntry};

pub fn run(archive_path: &Path, max_depth: Option<usize>) -> Result<()> {
    let archive = RpfArchive::open(archive_path)?;
    
    println!("{}", archive_path.file_name().unwrap_or_default().to_string_lossy());
    print_directory_tree(&archive.root, "", true, 0, max_depth);
    
    let file_count = archive.list_files().len();
    let dir_count = count_directories(&archive.root) - 1;
    println!("\n{} directories, {} files", dir_count, file_count);
    
    Ok(())
}

fn print_directory_tree(
    dir          : &RpfDirectoryEntry,
    prefix       : &str,
    _is_last     : bool,
    current_depth: usize,
    max_depth    : Option<usize>
) {
    if let Some(max) = max_depth {
        if current_depth >= max {
            return;
        }
    }
    
    let mut dirs = dir.directories.clone();
    dirs.sort_by(|a, b| a.name.cmp(&b.name));
    
    let mut files = dir.files.clone();
    files.sort_by(|a, b| a.name.cmp(&b.name));
    
    let total_entries = dirs.len() + files.len();
    let mut entry_count = 0;
	
    for subdir in &dirs {
        entry_count += 1;
        let is_last_entry = entry_count == total_entries;
        
        let connector = if is_last_entry { "└── " } else { "├── " };
        println!("{}{}{}/", prefix, connector, subdir.name);
        
        let new_prefix = if is_last_entry {
            format!("{}    ", prefix)
        } else {
            format!("{}│   ", prefix)
        };
        
        print_directory_tree(&subdir, &new_prefix, is_last_entry, current_depth + 1, max_depth);
    }
    
    for file in &files {
        entry_count += 1;
        let is_last_entry = entry_count == total_entries;
        
        let connector = if is_last_entry { "└── " } else { "├── " };
        println!("{}{}{} ({})", prefix, connector, file.name, format_size(file.uncompressed_size as u64));
    }
}

fn count_directories(dir: &RpfDirectoryEntry) -> usize {
    let mut count = 1;
    for subdir in &dir.directories {
        count += count_directories(subdir);
    }
    count
}

fn format_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = size as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", size as u64, UNITS[unit_index])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
} 