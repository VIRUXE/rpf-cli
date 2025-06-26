use anyhow::{anyhow, Context, Result};
use byteorder::{LittleEndian, ReadBytesExt};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, BufReader};
use std::path::{Path, PathBuf};
use flate2::read::{ZlibDecoder, DeflateDecoder};

const RPF7_MAGIC: u32 = 0x52504637; // 'RPF7'

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
}

impl RpfArchive {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = BufReader::new(File::open(&path)?);
        
        let header = Self::read_header(&mut file)?;
        let entries_start_pos = if header.entry_count < 100 {
            file.seek(SeekFrom::Start(2048))?;
            let mut test_bytes = [0u8; 16];
            file.read_exact(&mut test_bytes)?;
            
            if test_bytes.iter().all(|&b| b == 0) { 16u64 } else { 2048u64 }
        } else {
            2048u64
        };
        
        file.seek(SeekFrom::Start(entries_start_pos))?;
        let entries_data_size = (header.entry_count * 16) as usize;
        let mut entries_data = vec![0u8; entries_data_size];
        file.read_exact(&mut entries_data)?;

        let mut names_data = vec![0u8; header.names_length as usize];
        file.read_exact(&mut names_data)?;
        match header.encryption {
            0x4E45504F => {},
            0          => {},
            _          => {
                eprintln!("Warning: Archive appears to be encrypted (encryption: 0x{:08X})", header.encryption);
            }
        }
        
        let entries = Self::parse_entries(&entries_data, &names_data, header.entry_count)?;
        let root    = Self::build_directory_structure(&entries)?;
        
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
        let mut cursor  = std::io::Cursor::new(entries_data);
        
        for i in 0..count {
            cursor.seek(SeekFrom::Start((i * 16) as u64))?;
            
            let y    = cursor.read_u32::<LittleEndian>()?;
            let x    = cursor.read_u32::<LittleEndian>()?;
            let val2 = cursor.read_u32::<LittleEndian>()?;
            let val3 = cursor.read_u32::<LittleEndian>()?;

            let name_offset = y & 0xFFFFFF;
            
            let name = if name_offset as usize >= names_data.len() || name_offset > 1000000 {
                format!("hash_{:08x}", y)
            } else {
                match Self::read_name(names_data, name_offset) {
                    Ok(name) => name,
                    Err(_)   => { format!("entry_{}", i) }
                }
            };
            
            if x == 0x7FFFFF00 {
                entries.push(RpfEntry::Directory(RpfDirectoryEntry { name, path: String::new(), entries_index: val2, entries_count: val3, files: Vec::new(), directories: Vec::new() }));
            } else if (x & 0x80000000) == 0 {
                // Binary file entry
                cursor.seek(SeekFrom::Start((i * 16) as u64))?;
                let packed      = cursor.read_u64::<LittleEndian>()?;
                let name_offset = (packed & 0xFFFF) as u32;
                let file_size   = ((packed >> 16) & 0xFFFFFF) as u32;
                let file_offset = ((packed >> 40) & 0xFFFFFF) as u32;
                
                let uncompressed_size = cursor.read_u32::<LittleEndian>()?;
                let _encryption       = cursor.read_u32::<LittleEndian>()?;
                
                let name = match Self::read_name(names_data, name_offset) {
                    Ok(name) => name,
                    Err(_)   => { format!("binary_entry_{}", i) }
                };
                
                entries.push(RpfEntry::File(RpfFileEntry { name, path: String::new(), offset: file_offset, size: file_size, uncompressed_size, is_resource: false }));
            } else {
                // Resource file entry
                cursor.seek(SeekFrom::Start((i * 16) as u64))?;
                let name_offset = cursor.read_u16::<LittleEndian>()? as u32;
                
                let size_bytes = [cursor.read_u8()?, cursor.read_u8()?, cursor.read_u8()?];
                let file_size = (size_bytes[0] as u32) | ((size_bytes[1] as u32) << 8) | ((size_bytes[2] as u32) << 16);
                
                let offset_bytes = [cursor.read_u8()?, cursor.read_u8()?, cursor.read_u8()?];
                let file_offset = ((offset_bytes[0] as u32) | ((offset_bytes[1] as u32) << 8) | ((offset_bytes[2] as u32) << 16)) & 0x7FFFFF;
                
                let _system_flags = cursor.read_u32::<LittleEndian>()?;
                let _graphics_flags = cursor.read_u32::<LittleEndian>()?;
                

                
                let name = match Self::read_name(names_data, name_offset) {
                    Ok(name) => name,
                    Err(_) => { format!("resource_entry_{}", i) }
                };
                
                entries.push(RpfEntry::File(RpfFileEntry { name, path: String::new(), offset: file_offset, size: file_size, uncompressed_size: 0, is_resource: true }));
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
        
        // Try UTF-8 first (most common)
        if let Ok(name) = String::from_utf8(name_bytes.to_vec()) { return Ok(name); }
        
        // Try UTF-8 with lossy conversion (replaces invalid bytes with �)
        let lossy_name = String::from_utf8_lossy(name_bytes);
        if !lossy_name.contains('�') { return Ok(lossy_name.to_string()); }
        
        // Try Latin-1 (ISO-8859-1) encoding - common in older files
        let latin1_name: String = name_bytes.iter().map(|&b| b as char).collect();
        if latin1_name.chars().all(|c| c.is_ascii() || c as u32 <= 255) { return Ok(latin1_name); }
        
        // If all else fails, use lossy UTF-8 anyway
        Ok(lossy_name.to_string())
    }
    
    fn build_directory_structure(entries: &[RpfEntry]) -> Result<RpfDirectoryEntry> {
        if entries.is_empty() { return Err(anyhow!("No entries in archive")); }
        
        let root = match &entries[0] {
            RpfEntry::Directory(dir) => dir.clone(),
            _ => {
                RpfDirectoryEntry {
                    name         : String::new(),
                    path         : String::new(),
                    entries_index: 0,
                    entries_count: entries.len() as u32,
                    files        : Vec::new(),
                    directories  : Vec::new(),
                }
            }
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
        let mut file = File::open(&self.path)?;
        
        let file_offset = (file_entry.offset as u64) * 512;
        file.seek(SeekFrom::Start(file_offset))?;
        let size_to_read = if file_entry.size > 0 { 
            file_entry.size as usize 
        } else { 
            file_entry.uncompressed_size as usize 
        };
        
        if size_to_read == 0 { return Err(anyhow!("File has zero size")); }
        
        let mut data = vec![0u8; size_to_read];
        file.read_exact(&mut data)
            .with_context(|| format!("Failed to read {} bytes from offset {}", size_to_read, file_offset))?;
        
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