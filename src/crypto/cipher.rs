use aes::Aes256;
use cipher::{BlockDecrypt, KeyInit, generic_array::GenericArray};

use super::keys::GtaKeys;

pub fn decrypt_aes(data: &[u8], key: &[u8; 32]) -> Vec<u8> {
    let cipher = Aes256::new(GenericArray::from_slice(key));
    let mut buf = data.to_vec();
    let block_count = buf.len() / 16;
    for i in 0..block_count {
        let block = GenericArray::from_mut_slice(&mut buf[i * 16..(i + 1) * 16]);
        cipher.decrypt_block(block);
    }
    buf
}

pub fn get_ng_key_idx(name: &str, length: u32) -> usize {
    let hash = jenkins_hash(name);
    ((hash.wrapping_add(length).wrapping_add(101 - 40)) % 101) as usize
}

pub fn decrypt_ng(data: &[u8], keys: &GtaKeys, name: &str, length: u32) -> Vec<u8> {
    let key_idx = get_ng_key_idx(name, length);
    let key = &keys.ng_keys[key_idx];
    let key_uints: Vec<u32> = key
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();

    let mut result = data.to_vec();
    let block_count = data.len() / 16;

    for block_idx in 0..block_count {
        let block: &[u8; 16] = &data[block_idx * 16..(block_idx + 1) * 16]
            .try_into()
            .unwrap();
        let decrypted = decrypt_ng_block(block, &key_uints, &keys.ng_decrypt_tables);
        result[block_idx * 16..(block_idx + 1) * 16].copy_from_slice(&decrypted);
    }

    let left = data.len() % 16;
    if left != 0 {
        let offset = data.len() - left;
        result[offset..].copy_from_slice(&data[offset..]);
    }

    result
}

fn decrypt_ng_block(
    data: &[u8; 16],
    key: &[u32],
    tables: &[[[u32; 256]; 16]; 17],
) -> [u8; 16] {
    let sub_keys: Vec<[u32; 4]> = (0..17)
        .map(|i| [key[4 * i], key[4 * i + 1], key[4 * i + 2], key[4 * i + 3]])
        .collect();

    let mut buf = *data;
    buf = decrypt_ng_round_a(&buf, &sub_keys[0], &tables[0]);
    buf = decrypt_ng_round_a(&buf, &sub_keys[1], &tables[1]);
    for k in 2..=15 {
        buf = decrypt_ng_round_b(&buf, &sub_keys[k], &tables[k]);
    }
    buf = decrypt_ng_round_a(&buf, &sub_keys[16], &tables[16]);
    buf
}

fn decrypt_ng_round_a(data: &[u8; 16], key: &[u32; 4], table: &[[u32; 256]; 16]) -> [u8; 16] {
    let x1 = table[0][data[0] as usize]
        ^ table[1][data[1] as usize]
        ^ table[2][data[2] as usize]
        ^ table[3][data[3] as usize]
        ^ key[0];
    let x2 = table[4][data[4] as usize]
        ^ table[5][data[5] as usize]
        ^ table[6][data[6] as usize]
        ^ table[7][data[7] as usize]
        ^ key[1];
    let x3 = table[8][data[8] as usize]
        ^ table[9][data[9] as usize]
        ^ table[10][data[10] as usize]
        ^ table[11][data[11] as usize]
        ^ key[2];
    let x4 = table[12][data[12] as usize]
        ^ table[13][data[13] as usize]
        ^ table[14][data[14] as usize]
        ^ table[15][data[15] as usize]
        ^ key[3];

    let mut result = [0u8; 16];
    result[0..4].copy_from_slice(&x1.to_le_bytes());
    result[4..8].copy_from_slice(&x2.to_le_bytes());
    result[8..12].copy_from_slice(&x3.to_le_bytes());
    result[12..16].copy_from_slice(&x4.to_le_bytes());
    result
}

fn decrypt_ng_round_b(data: &[u8; 16], key: &[u32; 4], table: &[[u32; 256]; 16]) -> [u8; 16] {
    let x1 = table[0][data[0] as usize]
        ^ table[7][data[7] as usize]
        ^ table[10][data[10] as usize]
        ^ table[13][data[13] as usize]
        ^ key[0];
    let x2 = table[1][data[1] as usize]
        ^ table[4][data[4] as usize]
        ^ table[11][data[11] as usize]
        ^ table[14][data[14] as usize]
        ^ key[1];
    let x3 = table[2][data[2] as usize]
        ^ table[5][data[5] as usize]
        ^ table[8][data[8] as usize]
        ^ table[15][data[15] as usize]
        ^ key[2];
    let x4 = table[3][data[3] as usize]
        ^ table[6][data[6] as usize]
        ^ table[9][data[9] as usize]
        ^ table[12][data[12] as usize]
        ^ key[3];

    [
        (x1 >> 0) as u8, (x1 >> 8) as u8, (x1 >> 16) as u8, (x1 >> 24) as u8,
        (x2 >> 0) as u8, (x2 >> 8) as u8, (x2 >> 16) as u8, (x2 >> 24) as u8,
        (x3 >> 0) as u8, (x3 >> 8) as u8, (x3 >> 16) as u8, (x3 >> 24) as u8,
        (x4 >> 0) as u8, (x4 >> 8) as u8, (x4 >> 16) as u8, (x4 >> 24) as u8,
    ]
}

pub fn jenkins_hash(name: &str) -> u32 {
    let lower = name.to_lowercase();
    let bytes = lower.as_bytes();
    let mut hash: u32 = 0;
    for &b in bytes {
        hash = hash.wrapping_add(b as u32);
        hash = hash.wrapping_add(hash << 10);
        hash ^= hash >> 6;
    }
    hash = hash.wrapping_add(hash << 3);
    hash ^= hash >> 11;
    hash = hash.wrapping_add(hash << 15);
    hash
}

