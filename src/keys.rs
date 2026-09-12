//! GTA V key extraction.
//!
//! Only the AES key is still present as plain bytes in GTA5.exe. Later game
//! builds no longer carry the 101 NG keys or the 272 NG decrypt tables, so
//! scanning the executable for them fails. They come from CodeWalker's
//! `magic.dat` blob instead, which is deflate-compressed, AES-encrypted with
//! that same AES key, and masked with four passes of .NET's seeded PRNG.

use aes::Aes256;
use aes::cipher::{BlockDecrypt, KeyInit, generic_array::GenericArray};
use anyhow::{Context, Result, bail};
use flate2::read::DeflateDecoder;
use sha1::{Digest, Sha1};
use std::io::Read;
use std::{fs, path::{Path, PathBuf}};

use crate::rpf::GtaKeys;

const EMBEDDED_MAGIC: &[u8] = include_bytes!("../resources/magic.dat");

const NG_KEY_BYTES: usize = 101 * 272;
const NG_TABLE_BYTES: usize = 17 * 16 * 1024;

static PC_AES_KEY_HASH: [u8; 20] = [
    0xA0, 0x79, 0x61, 0x28, 0xA7, 0x75, 0x72, 0x0A, 0xC2, 0x04,
    0xD9, 0x81, 0x9F, 0x68, 0xC1, 0x72, 0xE3, 0x95, 0x2C, 0x6D,
];

/// Derive the keys straight from a game executable, ready to use.
pub fn from_exe(exe_path: &Path) -> Result<GtaKeys> {
    let (aes_key, ng_keys, ng_tables) = recover(exe_path)?;
    Ok(GtaKeys {
        aes_key,
        ng_keys: ng_keys.chunks_exact(272).map(<[u8]>::to_vec).collect(),
        ng_decrypt_tables: parse_ng_tables(&ng_tables),
    })
}

/// Recover the keys for `exe_path` and write them into `out_dir` as the
/// `gtav_*.dat` files that `--keys` reads back.
pub fn extract(exe_path: &Path, out_dir: &Path) -> Result<()> {
    let (aes_key, ng_keys, ng_tables) = recover(exe_path)?;

    fs::create_dir_all(out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;
    fs::write(out_dir.join("gtav_aes_key.dat"), aes_key)?;
    fs::write(out_dir.join("gtav_ng_key.dat"), &ng_keys)?;
    fs::write(out_dir.join("gtav_ng_decrypt_tables.dat"), &ng_tables)?;

    eprintln!("[Keys] Saved to {}", out_dir.display());
    Ok(())
}

/// Accept either the executable itself or the folder holding it.
pub fn resolve_exe(path: &Path) -> Result<PathBuf> {
    if path.is_dir() {
        for name in ["GTA5.exe", "GTA5_Enhanced.exe"] {
            let candidate = path.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        bail!("no GTA5.exe found in {}", path.display());
    }
    Ok(path.to_path_buf())
}

fn recover(exe_path: &Path) -> Result<([u8; 32], Vec<u8>, Vec<u8>)> {
    let exe_data = fs::read(exe_path)
        .with_context(|| format!("failed to read {}", exe_path.display()))?;

    log::debug!("searching for AES key in {} MB", exe_data.len() / 1024 / 1024);
    let aes_bytes = search_hash(&exe_data, &PC_AES_KEY_HASH, 32)
        .context("AES key not found — is this a GTA V PC executable?")?;
    let aes_key: [u8; 32] = aes_bytes.try_into().unwrap();

    let (ng_keys, ng_tables) = unwrap_magic(EMBEDDED_MAGIC, &aes_key)?;
    Ok((aes_key, ng_keys, ng_tables))
}

fn parse_ng_tables(data: &[u8]) -> Box<[[[u32; 256]; 16]; 17]> {
    let mut tables = Box::new([[[0u32; 256]; 16]; 17]);
    let mut words = data.chunks_exact(4);
    for round in tables.iter_mut() {
        for table in round.iter_mut() {
            for entry in table.iter_mut() {
                let word = words.next().expect("table data length checked on unwrap");
                *entry = u32::from_le_bytes(word.try_into().unwrap());
            }
        }
    }
    tables
}

/// Strip the PRNG mask, AES-decrypt, inflate, then split out the NG blobs.
fn unwrap_magic(magic: &[u8], aes_key: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    let len = magic.len();

    // The mask is four consecutive fills of one seeded stream, so a single
    // draw of 4x the length gives the same bytes in the same order.
    let mut mask = vec![0u8; len * 4];
    DotNetRandom::new(jenkins_hash(aes_key) as i32).next_bytes(&mut mask);

    let mut data = magic.to_vec();
    for (i, byte) in data.iter_mut().enumerate() {
        for pass in 0..4 {
            *byte = byte.wrapping_sub(mask[pass * len + i]);
        }
    }

    let cipher = Aes256::new(GenericArray::from_slice(aes_key));
    for block in data.chunks_exact_mut(16) {
        cipher.decrypt_block(GenericArray::from_mut_slice(block));
    }

    let mut inflated = Vec::new();
    DeflateDecoder::new(&data[..])
        .read_to_end(&mut inflated)
        .context("failed to inflate magic data — AES key may not match this magic.dat")?;

    let expected = NG_KEY_BYTES + NG_TABLE_BYTES;
    if inflated.len() < expected {
        bail!("magic data too small: {} bytes (expected at least {})", inflated.len(), expected);
    }

    let ng_keys = inflated[..NG_KEY_BYTES].to_vec();
    let ng_tables = inflated[NG_KEY_BYTES..expected].to_vec();
    Ok((ng_keys, ng_tables))
}

fn search_hash(data: &[u8], expected_sha1: &[u8; 20], length: usize) -> Option<Vec<u8>> {
    if data.len() < length {
        return None;
    }
    for i in 0..=(data.len() - length) {
        let candidate = &data[i..i + length];
        let hash: [u8; 20] = Sha1::digest(candidate).into();
        if &hash == expected_sha1 {
            return Some(candidate.to_vec());
        }
    }
    None
}

/// Jenkins one-at-a-time, matching CodeWalker's `JenkHash.GenHash`.
fn jenkins_hash(data: &[u8]) -> u32 {
    let mut h: u32 = 0;
    for &b in data {
        h = h.wrapping_add(b as u32);
        h = h.wrapping_add(h << 10);
        h ^= h >> 6;
    }
    h = h.wrapping_add(h << 3);
    h ^= h >> 11;
    h.wrapping_add(h << 15)
}

/// .NET's seeded `System.Random` (Knuth subtractive). The mask bytes are only
/// reproducible by matching this generator exactly, including its overflow.
struct DotNetRandom {
    seed_array: [i32; 56],
    inext: usize,
    inextp: usize,
}

impl DotNetRandom {
    const MBIG: i32 = i32::MAX;
    const MSEED: i32 = 161_803_398;

    fn new(seed: i32) -> Self {
        let subtraction = if seed == i32::MIN { i32::MAX } else { seed.abs() };
        let mut seed_array = [0i32; 56];

        let mut mj = Self::MSEED.wrapping_sub(subtraction);
        seed_array[55] = mj;
        let mut mk: i32 = 1;

        for i in 1..55 {
            let ii = (21 * i) % 55;
            seed_array[ii] = mk;
            mk = mj.wrapping_sub(mk);
            if mk < 0 {
                mk = mk.wrapping_add(Self::MBIG);
            }
            mj = seed_array[ii];
        }

        for _ in 1..5 {
            for i in 1..56 {
                seed_array[i] = seed_array[i].wrapping_sub(seed_array[1 + (i + 30) % 55]);
                if seed_array[i] < 0 {
                    seed_array[i] = seed_array[i].wrapping_add(Self::MBIG);
                }
            }
        }

        Self { seed_array, inext: 0, inextp: 21 }
    }

    fn internal_sample(&mut self) -> i32 {
        let mut inext = self.inext + 1;
        if inext >= 56 {
            inext = 1;
        }
        let mut inextp = self.inextp + 1;
        if inextp >= 56 {
            inextp = 1;
        }

        let mut ret = self.seed_array[inext].wrapping_sub(self.seed_array[inextp]);
        if ret == Self::MBIG {
            ret -= 1;
        }
        if ret < 0 {
            ret = ret.wrapping_add(Self::MBIG);
        }

        self.seed_array[inext] = ret;
        self.inext = inext;
        self.inextp = inextp;
        ret
    }

    fn next_bytes(&mut self, buffer: &mut [u8]) {
        for byte in buffer.iter_mut() {
            *byte = (self.internal_sample() % 256) as u8;
        }
    }
}
