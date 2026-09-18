//! The local index of a CASC storage: which archive a key is in, and where.
//!
//! `Data\data\*.idx` are small tables, one per bucket, mapping the first nine
//! bytes of an encoding key to a place in one of the `data.NNN` archives. The
//! header says how wide each field is, so nothing here is hard coded beyond
//! the shape.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Result, anyhow};

/// Where one encoded file sits in the archives.
#[derive(Clone, Copy, Debug)]
pub struct Place {
    pub archive: u32,
    pub offset: u64,
    pub size: u32,
}

/// Every key the local indices know about.
#[derive(Default)]
pub struct Index {
    pub places: HashMap<[u8; 9], Place>,
}

impl Index {
    pub fn find(&self, ekey: &[u8]) -> Option<Place> {
        if ekey.len() < 9 {
            return None;
        }
        let mut short = [0u8; 9];
        short.copy_from_slice(&ekey[..9]);
        self.places.get(&short).copied()
    }
}

/// Read every `.idx` in a folder. A newer index wins: Battle.net leaves the
/// older generation of a bucket behind, and the highest number is the live one.
pub fn read_folder(folder: &Path) -> Result<Index> {
    let mut newest: HashMap<u8, (u32, std::path::PathBuf)> = HashMap::new();
    for entry in std::fs::read_dir(folder)?.flatten() {
        let path = entry.path();
        let Some(name) = path.file_stem().map(|n| n.to_string_lossy().to_string()) else {
            continue;
        };
        if path.extension().map(|e| e.to_ascii_lowercase()) != Some("idx".into()) {
            continue;
        }
        // `0a00000042.idx`: two hex digits of bucket, eight of generation.
        if name.len() != 10 {
            continue;
        }
        let (Ok(bucket), Ok(generation)) = (
            u8::from_str_radix(&name[..2], 16),
            u32::from_str_radix(&name[2..], 16),
        ) else {
            continue;
        };
        let keep = match newest.get(&bucket) {
            Some((seen, _)) => generation > *seen,
            None => true,
        };
        if keep {
            newest.insert(bucket, (generation, path));
        }
    }
    if newest.is_empty() {
        return Err(anyhow!("no .idx files in {}", folder.display()));
    }

    let mut index = Index::default();
    for (_, path) in newest.values() {
        if let Ok(found) = read_one(path) {
            index.places.extend(found);
        }
    }
    Ok(index)
}

fn read_one(path: &Path) -> Result<HashMap<[u8; 9], Place>> {
    let raw = std::fs::read(path)?;
    if raw.len() < 0x28 {
        return Err(anyhow!("{} is too short to be an index", path.display()));
    }
    let head_size = u32::from_le_bytes(raw[0..4].try_into()?) as usize;
    // The header proper starts after its own size and hash.
    let head = &raw[8..8 + head_size];
    let key_len = head[6] as usize;
    let offset_len = head[5] as usize;
    let size_len = head[4] as usize;
    let offset_bits = head[7] as u32;
    let entry_len = key_len + offset_len + size_len;
    if entry_len == 0 || key_len < 9 {
        return Err(anyhow!("{}: unreadable index fields", path.display()));
    }

    // The entry table starts on the next sixteen byte boundary after the
    // header, not straight after it: an .idx has eight bytes of padding there.
    let table_at = (8 + head_size).div_ceil(16) * 16;
    if table_at + 8 > raw.len() {
        return Err(anyhow!("{}: no entry table", path.display()));
    }
    // The entry table is preceded by its own length and hash, both 4 bytes.
    let entries_size = u32::from_le_bytes(raw[table_at..table_at + 4].try_into()?) as usize;
    let start = table_at + 8;
    let end = (start + entries_size).min(raw.len());

    let mut found = HashMap::new();
    let mut at = start;
    while at + entry_len <= end {
        let entry = &raw[at..at + entry_len];
        at += entry_len;

        let mut key = [0u8; 9];
        key.copy_from_slice(&entry[..9]);
        // The place is big endian, and splits into archive number and offset
        // at whatever bit the header named.
        let mut packed = 0u64;
        for byte in &entry[key_len..key_len + offset_len] {
            packed = (packed << 8) | *byte as u64;
        }
        let offset = packed & ((1u64 << offset_bits) - 1);
        let archive = (packed >> offset_bits) as u32;

        let mut size = 0u32;
        for (i, byte) in entry[key_len + offset_len..].iter().enumerate() {
            size |= (*byte as u32) << (8 * i);
        }
        // The free-space list at the front of a bucket has no key worth
        // keeping: its entries point at gaps, not files.
        if key == [0u8; 9] {
            continue;
        }
        found.insert(
            key,
            Place {
                archive,
                offset,
                size,
            },
        );
    }
    Ok(found)
}
