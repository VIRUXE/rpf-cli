// Thin adapter over rage_rpf for rpf-cli commands.
// Re-exports rage_rpf types that commands use directly.
pub use rage_rpf::{
    DirNode, FileRef, GtaKeys, RpfArchive, RpfEncryption,
    build_directory_tree, list_all_files,
};

use anyhow::Result;
use std::path::Path;

/// Full archive with parsed metadata, directory tree, and raw data in memory.
pub struct Archive {
    pub path        : std::path::PathBuf,
    pub encryption  : RpfEncryption,
    pub entry_count : usize,
    pub dir_count   : usize,
    pub root        : DirNode,
    archive         : RpfArchive,
    data            : Vec<u8>,
}

impl Archive {
    pub fn open(path: &Path, keys: Option<&GtaKeys>) -> Result<Self> {
        let data = std::fs::read(path)?;
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let archive = RpfArchive::parse(&data, name, keys)?;

        let encryption = archive.encryption;
        let entry_count = archive.entries.len();
        let dir_count = archive.entries.iter().filter(|e| e.is_directory()).count();
        let root = build_directory_tree(&archive.entries);

        Ok(Self { path: path.to_path_buf(), encryption, entry_count, dir_count, root, archive, data })
    }

    pub fn list_files(&self) -> Vec<&FileRef> {
        list_all_files(&self.root)
    }

    pub fn find_file(&self, path: &str) -> Option<&FileRef> {
        let path = path.replace('\\', "/").to_lowercase();
        find_in_dir(&self.root, &path)
    }

    pub fn extract(&self, file: &FileRef, keys: Option<&GtaKeys>) -> Result<Vec<u8>> {
        let entry = &self.archive.entries[file.entry_index];
        self.archive.extract_entry(&self.data, entry, keys)
    }
}

fn find_in_dir<'a>(dir: &'a DirNode, path: &str) -> Option<&'a FileRef> {
    for f in &dir.files {
        if f.path == path || f.name.to_lowercase() == path { return Some(f); }
    }
    for sub in &dir.subdirs {
        if let Some(f) = find_in_dir(sub, path) { return Some(f); }
    }
    None
}
