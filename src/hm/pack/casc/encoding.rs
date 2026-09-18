//! The encoding table: content keys to encoding keys.
//!
//! A build config names its files by content key — what the file is — while
//! the archives are addressed by encoding key — how it was stored. The
//! encoding table is the map between them, and it is itself a file in the
//! archives, named by both.

use std::collections::HashMap;

use anyhow::{Result, anyhow};

#[derive(Default)]
pub struct Encoding {
    /// Content key to the first encoding key listed for it.
    pub by_ckey: HashMap<[u8; 16], [u8; 16]>,
}

impl Encoding {
    pub fn ekey_of(&self, ckey: &[u8]) -> Option<[u8; 16]> {
        if ckey.len() < 16 {
            return None;
        }
        let mut key = [0u8; 16];
        key.copy_from_slice(&ckey[..16]);
        self.by_ckey.get(&key).copied()
    }
}

pub fn parse(raw: &[u8]) -> Result<Encoding> {
    if raw.len() < 22 || &raw[..2] != b"EN" {
        return Err(anyhow!("not an encoding table"));
    }
    let ckey_size = raw[3] as usize;
    let ekey_size = raw[4] as usize;
    let page_size = u16::from_be_bytes(raw[5..7].try_into()?) as usize * 1024;
    let pages = u32::from_be_bytes(raw[9..13].try_into()?) as usize;
    let espec_size = u32::from_be_bytes(raw[18..22].try_into()?) as usize;

    // Header, then the espec strings, then the page index, then the pages.
    let index_at = 22 + espec_size;
    let index_len = pages * (ckey_size + 16);
    let mut at = index_at
        .checked_add(index_len)
        .ok_or_else(|| anyhow!("encoding table is cut short"))?;

    let mut out = Encoding::default();
    for _ in 0..pages {
        let end = (at + page_size).min(raw.len());
        if at >= end {
            break;
        }
        let page = &raw[at..end];
        let mut cursor = 0usize;
        // Entry: how many encoding keys, the file size, the content key, then
        // the keys themselves. A zero count is the padding at the page's end.
        while cursor + 6 + ckey_size <= page.len() {
            let keys = page[cursor] as usize;
            if keys == 0 {
                break;
            }
            let ckey_at = cursor + 6;
            let ekey_at = ckey_at + ckey_size;
            let entry_len = 6 + ckey_size + keys * ekey_size;
            if cursor + entry_len > page.len() {
                break;
            }
            if ckey_size == 16 && ekey_size == 16 {
                let mut ckey = [0u8; 16];
                ckey.copy_from_slice(&page[ckey_at..ckey_at + 16]);
                let mut ekey = [0u8; 16];
                ekey.copy_from_slice(&page[ekey_at..ekey_at + 16]);
                out.by_ckey.insert(ckey, ekey);
            }
            cursor += entry_len;
        }
        at = end;
    }
    Ok(out)
}
