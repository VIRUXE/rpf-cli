use anyhow::Result;
use std::path::Path;
use crate::rpf::RpfArchive;
use crate::crypto::GtaKeys;

pub fn run(archive_path: &Path, keys: Option<&GtaKeys>) -> Result<()> {
    println!("RPF Archive Information");
    println!("======================");
    println!("Path: {}", archive_path.display());

    let archive = RpfArchive::open_with_keys(archive_path, keys)?;

    println!("Version: RPF7 (0x{:08X})", archive.header.version);
    println!("Entries: {}", archive.header.entry_count);
    println!("Encryption: {}", match archive.header.encryption {
        0x4E45504F => "OPEN (No encryption)",
        0          => "NONE",
        0x0FFFFFF9 => "AES",
        0x0FEFFFFF => "NG",
        _          => "Unknown",
    });

    let (file_count, dir_count) = count_entries(&archive);

    println!("Files: {}", file_count);
    println!("Directories: {}", dir_count);

    let total_size: u64 = archive.list_files()
        .iter()
        .map(|f| f.uncompressed_size as u64)
        .sum();

    println!("Total uncompressed size: {} bytes ({:.2} MB)", total_size,  total_size as f64 / (1024.0 * 1024.0));

    Ok(())
}

fn count_entries(archive: &RpfArchive) -> (usize, usize) {
    let mut file_count = 0;
    let mut dir_count  = 0;

    for entry in &archive.entries {
        match entry {
            crate::rpf::RpfEntry::File(_) => file_count += 1,
            crate::rpf::RpfEntry::Directory(_) => dir_count += 1,
        }
    }

    (file_count, dir_count)
}
