use anyhow::Result;
use std::path::Path;
use crate::rpf::RpfArchive;

pub fn run(archive_path: &Path) -> Result<()> {
    println!("Verifying archive: {}", archive_path.display());
    
    let archive = match RpfArchive::open(archive_path) {
        Ok(a) => a,
        Err(e) => {
            println!("✗ Failed to open archive: {}", e);
            return Ok(());
        }
    };
    
    println!("✓ Archive header valid");
    println!("✓ Entry table loaded successfully");
    
    let files = archive.list_files();
    println!("  Found {} files", files.len());
    
    let mut errors = 0;
    let mut checked = 0;
    
    for file in &files {
        checked += 1;
        
        if file.offset == 0 && file.size > 0 {
            eprintln!("✗ File {} has invalid offset", file.path);
            errors += 1;
        }
        if file.size > 0 && file.uncompressed_size > 0 && file.size > file.uncompressed_size {
            eprintln!("✗ File {} has compressed size larger than uncompressed size", file.path);
            errors += 1;
        }
        
        if checked % 1000 == 0 {
            print!("\rChecking file entries... {}/{}", checked, files.len());
        }
    }
    
    if checked >= 1000 { println!(); }
    
    if errors == 0 {
        println!("✓ All {} file entries appear valid", files.len());
        println!("\nArchive verification completed successfully!");
    } else {
        println!("✗ Found {} errors in file entries", errors);
        println!("\nArchive verification completed with errors!");
    }
    
    Ok(())
} 