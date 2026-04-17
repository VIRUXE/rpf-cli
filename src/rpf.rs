#![allow(dead_code)]

use anyhow::{anyhow, Context, Result};
use byteorder::{LittleEndian, ReadBytesExt};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, BufReader};
use std::path::{Path, PathBuf};
use flate2::read::{ZlibDecoder, DeflateDecoder};

use crate::crypto::{decrypt_aes, decrypt_ng, GtaKeys};

const RPF7_MAGIC: u32 = 0x52504637; // 'RPF7'
const RESOURCE_IDENT: u32 = 0x37435352; // 'RSC7'

pub const ENC_NONE: u32 = 0x00000000;
pub const ENC_OPEN: u32 = 0x4E45504F;
pub const ENC_AES:  u32 = 0x0FFFFFF9;
pub const ENC_NG:   u32 = 0x0FEFFFFF;

#[derive(Debug, Clone)]
pub struct RpfArchive {
    pub path             : PathBuf,
    pub header           : RpfHeader,
    pub entries          : Vec<RpfEntry>,
    pub root             : RpfDirectoryEntry,
}

#[derive(Debug, Clone)]
pub struct RpfHeader {
    pub version     : u32,
    pub entry_count : u32,
    pub names_length: u32,
    pub encryption  : u32,
}

#[derive(Debug, Clone)]
pub enum RpfEntry {
    Directory(RpfDirectoryEntry),
    File(RpfFileEntry),
}

#[derive(Debug, Clone)]
pub struct RpfDirectoryEntry {
    pub name         : String,
    pub path         : String,
    pub entries_index: u32,
    pub entries_count: u32,
    pub files        : Vec<RpfFileEntry>,
    pub directories  : Vec<RpfDirectoryEntry>,
}

#[derive(Debug, Clone)]
pub struct RpfFileEntry {
    pub name             : String,
    pub path             : String,
    pub offset           : u32,
    pub size             : u32,
    pub uncompressed_size: u32,
    pub is_resource      : bool,
    pub is_encrypted     : bool,
    pub system_flags     : u32,
    pub graphics_flags   : u32,
}

impl RpfArchive {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::open_with_keys(path, None)
    }

    pub fn open_with_keys<P: AsRef<Path>>(path: P, keys: Option<&GtaKeys>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = BufReader::new(File::open(&path)?);

        let header = Self::read_header(&mut file)?;

        if matches!(header.encryption, ENC_AES | ENC_NG) && keys.is_none() {
            return Err(anyhow!(
                "Archive is encrypted (type 0x{:08X}) but no keys were provided. \
                 Pass --keys <dir> with extracted GTA V keys.",
                header.encryption
            ));
        }

        file.seek(SeekFrom::Start(16))?;
        let entries_data_size = (header.entry_count as usize) * 16;
        let mut entries_data = vec![0u8; entries_data_size];
        file.read_exact(&mut entries_data)?;

        let mut names_data = vec![0u8; header.names_length as usize];
        file.read_exact(&mut names_data)?;

        let file_size = std::fs::metadata(&path)?.len();
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        match header.encryption {
            ENC_AES => {
                let key = keys.unwrap();
                entries_data = decrypt_aes(&entries_data, &key.aes_key);
                names_data   = decrypt_aes(&names_data, &key.aes_key);
            }
            ENC_NG => {
                let key = keys.unwrap();
                entries_data = decrypt_ng(&entries_data, key, file_name, file_size as u32);
                names_data   = decrypt_ng(&names_data, key, file_name, file_size as u32);
            }
            _ => {}
        }

        let mut entries = Self::parse_entries(&entries_data, &names_data, header.entry_count)?;

        // Resolve resource entries where size == 0xFFFFFF (size encoded in body header)
        let has_giant = entries.iter().any(|e| {
            matches!(e, RpfEntry::File(f) if f.is_resource && f.size == 0xFFFFFF)
        });
        if has_giant {
            let mut f = File::open(&path)?;
            for entry in entries.iter_mut() {
                if let RpfEntry::File(file_entry) = entry {
                    if file_entry.is_resource && file_entry.size == 0xFFFFFF {
                        let body_off = (file_entry.offset as u64) * 512;
                        f.seek(SeekFrom::Start(body_off))?;
                        let mut b = [0u8; 16];
                        if f.read_exact(&mut b).is_ok() {
                            file_entry.size = ((b[7] as u32) << 0)
                                | ((b[14] as u32) << 8)
                                | ((b[5] as u32) << 16)
                                | ((b[2] as u32) << 24);
                        }
                    }
                }
            }
        }

        let root = Self::build_directory_structure(&entries)?;

        Ok(RpfArchive { path, header, entries, root })
    }

    fn read_header(file: &mut BufReader<File>) -> Result<RpfHeader> {
        let version = file.read_u32::<LittleEndian>()?;
        if version != RPF7_MAGIC { return Err(anyhow!("Not a valid RPF7 archive (magic: 0x{:08X})", version)); }

        let entry_count  = file.read_u32::<LittleEndian>()?;
        let names_length = file.read_u32::<LittleEndian>()?;
        let encryption   = file.read_u32::<LittleEndian>()?;

        Ok(RpfHeader { version, entry_count, names_length, encryption })
    }

    fn parse_entries(entries_data: &[u8], names_data: &[u8], count: u32) -> Result<Vec<RpfEntry>> {
        let mut entries = Vec::new();

        for i in 0..count as usize {
            let off = i * 16;
            let chunk = &entries_data[off..off + 16];
            let h1 = u32::from_le_bytes(chunk[0..4].try_into().unwrap());
            let h2 = u32::from_le_bytes(chunk[4..8].try_into().unwrap());

            if h2 == 0x7FFFFF00 {
                // Directory entry
                let name_offset = h1 as u32;
                let entries_index = u32::from_le_bytes(chunk[8..12].try_into().unwrap());
                let entries_count = u32::from_le_bytes(chunk[12..16].try_into().unwrap());
                let name = Self::read_name(names_data, name_offset)
                    .unwrap_or_else(|_| format!("dir_{}", i));
                entries.push(RpfEntry::Directory(RpfDirectoryEntry {
                    name,
                    path: String::new(),
                    entries_index,
                    entries_count,
                    files: Vec::new(),
                    directories: Vec::new(),
                }));
            } else if (h2 & 0x80000000) == 0 {
                // Binary file entry
                let name_offset = u16::from_le_bytes(chunk[0..2].try_into().unwrap()) as u32;
                let file_size = (chunk[2] as u32)
                    | ((chunk[3] as u32) << 8)
                    | ((chunk[4] as u32) << 16);
                let file_offset = (chunk[5] as u32)
                    | ((chunk[6] as u32) << 8)
                    | ((chunk[7] as u32) << 16);
                let uncompressed_size = u32::from_le_bytes(chunk[8..12].try_into().unwrap());
                let encryption_type = u32::from_le_bytes(chunk[12..16].try_into().unwrap());
                let is_encrypted = encryption_type == 1;

                let name = Self::read_name(names_data, name_offset)
                    .unwrap_or_else(|_| format!("binary_entry_{}", i));

                entries.push(RpfEntry::File(RpfFileEntry {
                    name,
                    path: String::new(),
                    offset: file_offset,
                    size: file_size,
                    uncompressed_size,
                    is_resource: false,
                    is_encrypted,
                    system_flags: 0,
                    graphics_flags: 0,
                }));
            } else {
                // Resource file entry
                let name_offset = u16::from_le_bytes(chunk[0..2].try_into().unwrap()) as u32;
                let file_size = (chunk[2] as u32)
                    | ((chunk[3] as u32) << 8)
                    | ((chunk[4] as u32) << 16);
                let file_offset_raw = (chunk[5] as u32)
                    | ((chunk[6] as u32) << 8)
                    | ((chunk[7] as u32) << 16);
                let file_offset = file_offset_raw & 0x7FFFFF;
                let system_flags = u32::from_le_bytes(chunk[8..12].try_into().unwrap());
                let graphics_flags = u32::from_le_bytes(chunk[12..16].try_into().unwrap());

                let name = Self::read_name(names_data, name_offset)
                    .unwrap_or_else(|_| format!("resource_entry_{}", i));
                let is_encrypted = name.to_lowercase().ends_with(".ysc");

                entries.push(RpfEntry::File(RpfFileEntry {
                    name,
                    path: String::new(),
                    offset: file_offset,
                    size: file_size,
                    uncompressed_size: 0,
                    is_resource: true,
                    is_encrypted,
                    system_flags,
                    graphics_flags,
                }));
            }
        }

        Ok(entries)
    }

    fn read_name(names_data: &[u8], offset: u32) -> Result<String> {
        let offset = offset as usize;
        if offset >= names_data.len() { return Err(anyhow!("Name offset {} out of bounds (names data length: {})", offset, names_data.len())); }

        let end = names_data[offset..]
            .iter()
            .position(|&b| b == 0)
            .map(|p| offset + p)
            .unwrap_or(names_data.len());

        let name_bytes = &names_data[offset..end];

        if let Ok(name) = String::from_utf8(name_bytes.to_vec()) { return Ok(name); }

        let lossy_name = String::from_utf8_lossy(name_bytes);
        if !lossy_name.contains('\u{FFFD}') { return Ok(lossy_name.to_string()); }

        let latin1_name: String = name_bytes.iter().map(|&b| b as char).collect();
        if latin1_name.chars().all(|c| c.is_ascii() || c as u32 <= 255) { return Ok(latin1_name); }

        Ok(lossy_name.to_string())
    }

    fn build_directory_structure(entries: &[RpfEntry]) -> Result<RpfDirectoryEntry> {
        if entries.is_empty() { return Err(anyhow!("No entries in archive")); }

        let root = match &entries[0] {
            RpfEntry::Directory(dir) => dir.clone(),
            _ => RpfDirectoryEntry {
                name         : String::new(),
                path         : String::new(),
                entries_index: 0,
                entries_count: entries.len() as u32,
                files        : Vec::new(),
                directories  : Vec::new(),
            },
        };

        let mut root = root;
        root.path = String::new();

        Self::populate_directory(&mut root, entries, "")?;

        Ok(root)
    }

    fn populate_directory(dir: &mut RpfDirectoryEntry, all_entries: &[RpfEntry], parent_path: &str) -> Result<()> {
        let start_idx = dir.entries_index as usize;
        let end_idx   = start_idx + dir.entries_count as usize;

        if end_idx > all_entries.len() { return Err(anyhow!("Directory entry indices out of bounds")); }

        let is_synthetic_root = dir.entries_index == 0 && dir.name.is_empty() && matches!(all_entries.get(0), Some(RpfEntry::File(_)));

        if is_synthetic_root {
            for entry in all_entries {
                if let RpfEntry::File(file) = entry {
                    let mut file = file.clone();
                    file.path = file.name.clone();
                    dir.files.push(file);
                }
            }
        } else {
            for i in start_idx..end_idx {
                match &all_entries[i] {
                    RpfEntry::Directory(subdir) => {
                        let mut subdir = subdir.clone();
                        subdir.path = if parent_path.is_empty() {
                            subdir.name.clone()
                        } else {
                            format!("{}/{}", parent_path, subdir.name)
                        };

                        let subdir_path = subdir.path.clone();
                        Self::populate_directory(&mut subdir, all_entries, &subdir_path)?;
                        dir.directories.push(subdir);
                    }
                    RpfEntry::File(file) => {
                        let mut file = file.clone();
                        file.path = if parent_path.is_empty() {
                            file.name.clone()
                        } else {
                            format!("{}/{}", parent_path, file.name)
                        };
                        dir.files.push(file);
                    }
                }
            }
        }

        Ok(())
    }

    pub fn extract_file(&self, file_entry: &RpfFileEntry) -> Result<Vec<u8>> {
        self.extract_file_with_keys(file_entry, None)
    }

    pub fn extract_file_with_keys(&self, file_entry: &RpfFileEntry, keys: Option<&GtaKeys>) -> Result<Vec<u8>> {
        let mut file = File::open(&self.path)?;

        let file_offset = (file_entry.offset as u64) * 512;
        file.seek(SeekFrom::Start(file_offset))?;

        if file_entry.is_resource {
            return self.extract_resource(&mut file, file_entry, keys);
        }

        let size_to_read = if file_entry.size > 0 {
            file_entry.size as usize
        } else {
            file_entry.uncompressed_size as usize
        };

        if size_to_read == 0 { return Err(anyhow!("File has zero size")); }

        let mut data = vec![0u8; size_to_read];
        file.read_exact(&mut data)
            .with_context(|| format!("Failed to read {} bytes from offset {}", size_to_read, file_offset))?;

        if file_entry.is_encrypted {
            data = self.decrypt_bytes(&data, &file_entry.name, file_entry.uncompressed_size, keys)?;
        }

        if file_entry.size > 0 && file_entry.size < file_entry.uncompressed_size {
            let mut decompressed = Vec::new();
            if ZlibDecoder::new(&data[..]).read_to_end(&mut decompressed).is_ok() { return Ok(decompressed); }

            decompressed.clear();
            if DeflateDecoder::new(&data[..]).read_to_end(&mut decompressed).is_ok() { return Ok(decompressed); }

            eprintln!("Warning: File appears compressed but decompression failed, returning raw data");
            Ok(data)
        } else {
            Ok(data)
        }
    }

    fn extract_resource(
        &self,
        file: &mut File,
        file_entry: &RpfFileEntry,
        keys: Option<&GtaKeys>,
    ) -> Result<Vec<u8>> {
        let total = file_entry.size as usize;
        if total < 16 {
            return Err(anyhow!("Resource file too small ({} bytes)", total));
        }

        // Skip the 16-byte in-RPF RSC7 header; the body follows.
        file.seek(SeekFrom::Current(16))?;
        let body_len = total - 16;
        let mut body = vec![0u8; body_len];
        file.read_exact(&mut body)
            .with_context(|| format!("Failed to read resource body ({} bytes)", body_len))?;

        if file_entry.is_encrypted {
            body = self.decrypt_bytes(&body, &file_entry.name, file_entry.size, keys)?;
        }

        // Rebuild a standalone RSC7 file: fresh 16-byte header + original deflate body.
        let version = version_from_flags(file_entry.system_flags, file_entry.graphics_flags);
        let mut out = Vec::with_capacity(body.len() + 16);
        out.extend_from_slice(&RESOURCE_IDENT.to_le_bytes());
        out.extend_from_slice(&version.to_le_bytes());
        out.extend_from_slice(&file_entry.system_flags.to_le_bytes());
        out.extend_from_slice(&file_entry.graphics_flags.to_le_bytes());
        out.extend_from_slice(&body);
        Ok(out)
    }

    fn decrypt_bytes(
        &self,
        data: &[u8],
        name: &str,
        length: u32,
        keys: Option<&GtaKeys>,
    ) -> Result<Vec<u8>> {
        match self.header.encryption {
            ENC_AES => {
                let key = keys.ok_or_else(|| anyhow!("AES-encrypted entry requires --keys"))?;
                Ok(decrypt_aes(data, &key.aes_key))
            }
            ENC_NG => {
                let key = keys.ok_or_else(|| anyhow!("NG-encrypted entry requires --keys"))?;
                Ok(decrypt_ng(data, key, name, length))
            }
            _ => Ok(data.to_vec()),
        }
    }

    pub fn find_file(&self, path: &str) -> Option<&RpfFileEntry> {
        self.find_file_in_dir(&self.root, path)
    }

    fn find_file_in_dir<'a>(&'a self, dir: &'a RpfDirectoryEntry, path: &str) -> Option<&'a RpfFileEntry> {
        let path = path.replace('\\', "/");

        for file in &dir.files {
            if file.path == path || file.name == path { return Some(file); }
        }
        for subdir in &dir.directories {
            if let Some(file) = self.find_file_in_dir(subdir, &path) { return Some(file); }
        }

        None
    }

    pub fn list_files(&self) -> Vec<&RpfFileEntry> {
        let mut files = Vec::new();
        self.collect_files(&self.root, &mut files);
        files
    }

    fn collect_files<'a>(&'a self, dir: &'a RpfDirectoryEntry, files: &mut Vec<&'a RpfFileEntry>) {
        files.extend(&dir.files);
        for subdir in &dir.directories { self.collect_files(subdir, files); }
    }
}

/// Resource version field from system+graphics page flags.
fn version_from_flags(sys_flags: u32, gfx_flags: u32) -> u32 {
    let sv = (sys_flags >> 28) & 0xF;
    let gv = (gfx_flags >> 28) & 0xF;
    (sv << 4) | gv
}
