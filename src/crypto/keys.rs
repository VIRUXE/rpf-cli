use anyhow::{Context, Result, bail};
use sha1::{Digest, Sha1};
use std::{fs, path::Path};

/// GTA V cryptographic keys needed for RPF decryption.
pub struct GtaKeys {
    pub aes_key: [u8; 32],
    pub ng_keys: Vec<Vec<u8>>,
    pub ng_decrypt_tables: Box<[[[u32; 256]; 16]; 17]>,
}

impl GtaKeys {
    /// Load pre-extracted keys from a directory.
    /// Expected files: gtav_aes_key.dat, gtav_ng_key.dat, gtav_ng_decrypt_tables.dat
    pub fn load_from_path(path: &Path) -> Result<Self> {
        let aes_bytes = fs::read(path.join("gtav_aes_key.dat"))
            .context("Missing gtav_aes_key.dat")?;
        if aes_bytes.len() != 32 {
            bail!("gtav_aes_key.dat has wrong size (expected 32 bytes)");
        }
        let aes_key: [u8; 32] = aes_bytes.try_into().unwrap();

        let ng_key_bytes = fs::read(path.join("gtav_ng_key.dat"))
            .context("Missing gtav_ng_key.dat")?;
        let ng_keys = read_ng_keys(&ng_key_bytes)?;

        let ng_table_bytes = fs::read(path.join("gtav_ng_decrypt_tables.dat"))
            .context("Missing gtav_ng_decrypt_tables.dat")?;
        let ng_decrypt_tables = read_ng_tables(&ng_table_bytes)?;

        Ok(Self { aes_key, ng_keys, ng_decrypt_tables })
    }

    /// Extract keys from a GTA5.exe and optionally save them to a directory.
    pub fn extract_from_exe(exe_path: &Path, save_to: Option<&Path>) -> Result<Self> {
        eprintln!("[Keys] Reading GTA5.exe ({} MB)...", fs::metadata(exe_path)?.len() / 1024 / 1024);
        let exe_data = fs::read(exe_path).context("Failed to read GTA5.exe")?;

        eprintln!("[Keys] Searching for AES key...");
        let aes_bytes = search_hash(&exe_data, &PC_AES_KEY_HASH, 32)
            .context("AES key not found in GTA5.exe")?;
        let aes_key: [u8; 32] = aes_bytes.try_into().unwrap();

        eprintln!("[Keys] Searching for 101 NG keys...");
        let ng_key_bufs = search_hashes(&exe_data, &PC_NG_KEY_HASHES, 0x110)?;
        let mut ng_keys_flat = Vec::with_capacity(101 * 272);
        for buf in &ng_key_bufs {
            ng_keys_flat.extend_from_slice(buf);
        }
        let ng_keys = read_ng_keys(&ng_keys_flat)?;

        eprintln!("[Keys] Searching for 272 NG decrypt tables...");
        let table_bufs = search_hashes(&exe_data, &PC_NG_DECRYPT_TABLE_HASHES, 0x400)?;
        let mut tables_flat = Vec::with_capacity(272 * 0x400);
        for buf in &table_bufs {
            tables_flat.extend_from_slice(buf);
        }
        let ng_decrypt_tables = read_ng_tables(&tables_flat)?;

        let keys = Self { aes_key, ng_keys, ng_decrypt_tables };

        if let Some(out_path) = save_to {
            keys.save_to_path(out_path)?;
        }

        Ok(keys)
    }

    pub fn save_to_path(&self, path: &Path) -> Result<()> {
        fs::create_dir_all(path)?;
        fs::write(path.join("gtav_aes_key.dat"), &self.aes_key)?;
        fs::write(path.join("gtav_ng_key.dat"), write_ng_keys(&self.ng_keys))?;
        fs::write(path.join("gtav_ng_decrypt_tables.dat"), write_ng_tables(&self.ng_decrypt_tables))?;
        eprintln!("[Keys] Saved to {}", path.display());
        Ok(())
    }
}

fn read_ng_keys(data: &[u8]) -> Result<Vec<Vec<u8>>> {
    const KEY_SIZE: usize = 272;
    const KEY_COUNT: usize = 101;
    if data.len() < KEY_SIZE * KEY_COUNT {
        bail!("NG key data too small: {} bytes", data.len());
    }
    Ok((0..KEY_COUNT)
        .map(|i| data[i * KEY_SIZE..(i + 1) * KEY_SIZE].to_vec())
        .collect())
}

fn write_ng_keys(keys: &[Vec<u8>]) -> Vec<u8> {
    keys.iter().flat_map(|k| k.iter().cloned()).collect()
}

fn read_ng_tables(data: &[u8]) -> Result<Box<[[[u32; 256]; 16]; 17]>> {
    const EXPECTED: usize = 17 * 16 * 1024;
    if data.len() < EXPECTED {
        bail!("NG table data too small: {} bytes (expected {})", data.len(), EXPECTED);
    }
    let mut tables = vec![[[0u32; 256]; 16]; 17];
    let mut offset = 0;
    for i in 0..17 {
        for j in 0..16 {
            for k in 0..256 {
                tables[i][j][k] = u32::from_le_bytes([
                    data[offset],
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                ]);
                offset += 4;
            }
        }
    }
    let arr: [[[u32; 256]; 16]; 17] = tables
        .try_into()
        .map_err(|_| anyhow::anyhow!("Failed to convert ng tables to fixed array"))?;
    Ok(Box::new(arr))
}

fn write_ng_tables(tables: &[[[u32; 256]; 16]; 17]) -> Vec<u8> {
    let mut out = Vec::with_capacity(17 * 16 * 1024);
    for i in 0..17 {
        for j in 0..16 {
            for k in 0..256 {
                out.extend_from_slice(&tables[i][j][k].to_le_bytes());
            }
        }
    }
    out
}

fn search_hash(data: &[u8], expected_sha1: &[u8; 20], length: usize) -> Option<Vec<u8>> {
    if data.len() < length {
        return None;
    }
    for i in 0..=(data.len() - length) {
        let candidate = &data[i..i + length];
        let mut hasher = Sha1::new();
        hasher.update(candidate);
        let hash: [u8; 20] = hasher.finalize().into();
        if &hash == expected_sha1 {
            return Some(candidate.to_vec());
        }
    }
    None
}

fn search_hashes(data: &[u8], hashes: &[[u8; 20]], length: usize) -> Result<Vec<Vec<u8>>> {
    let mut results: Vec<Option<Vec<u8>>> = vec![None; hashes.len()];
    let mut found = 0usize;

    if data.len() >= length {
        for i in 0..=(data.len() - length) {
            if found == hashes.len() {
                break;
            }
            let candidate = &data[i..i + length];
            let mut hasher = Sha1::new();
            hasher.update(candidate);
            let hash: [u8; 20] = hasher.finalize().into();
            for (j, expected) in hashes.iter().enumerate() {
                if results[j].is_none() && &hash == expected {
                    results[j] = Some(candidate.to_vec());
                    found += 1;
                    break;
                }
            }
        }
    }

    results
        .into_iter()
        .enumerate()
        .map(|(i, r)| r.ok_or_else(|| anyhow::anyhow!("Hash {} not found in GTA5.exe", i)))
        .collect()
}

static PC_AES_KEY_HASH: [u8; 20] = [
    0xA0, 0x79, 0x61, 0x28, 0xA7, 0x75, 0x72, 0x0A, 0xC2, 0x04,
    0xD9, 0x81, 0x9F, 0x68, 0xC1, 0x72, 0xE3, 0x95, 0x2C, 0x6D,
];

static PC_NG_KEY_HASHES: [[u8; 20]; 101] =
    include!("ng_key_hashes.rs");

static PC_NG_DECRYPT_TABLE_HASHES: [[u8; 20]; 272] =
    include!("ng_table_hashes.rs");
