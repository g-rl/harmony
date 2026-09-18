//! BLTE: the block container everything in a CASC storage is wrapped in.
//!
//! A header names the blocks, and each block carries a mode byte saying how it
//! was stored: `N` as it is, `Z` deflated, `F` another BLTE inside, `E`
//! encrypted. Harmony ships no keys, so an encrypted block is reported rather
//! than guessed at.

use std::io::Read;

use anyhow::{Result, anyhow};

pub const MAGIC: &[u8; 4] = b"BLTE";

/// Decode a whole BLTE stream.
pub fn decode(raw: &[u8]) -> Result<Vec<u8>> {
    if raw.len() < 8 || &raw[..4] != MAGIC {
        return Err(anyhow!("not a blte stream"));
    }
    let header_size = u32::from_be_bytes(raw[4..8].try_into()?) as usize;

    // No header means one block, the rest of the stream.
    if header_size == 0 {
        return block(&raw[8..]);
    }
    if raw.len() < 12 {
        return Err(anyhow!("blte header is cut short"));
    }
    let count = u32::from_be_bytes([0, raw[9], raw[10], raw[11]]) as usize;
    let table = &raw[12..];
    // 0x0F entries are size, size, md5; 0x10 adds the decoded md5.
    let entry_len = match raw[8] {
        0x10 => 40,
        _ => 24,
    };
    if table.len() < count * entry_len {
        return Err(anyhow!("blte block table is cut short"));
    }

    let mut out = Vec::new();
    let mut at = header_size;
    for i in 0..count {
        let entry = &table[i * entry_len..];
        let packed = u32::from_be_bytes(entry[0..4].try_into()?) as usize;
        let end = at
            .checked_add(packed)
            .filter(|end| *end <= raw.len())
            .ok_or_else(|| anyhow!("blte block runs past the end of the stream"))?;
        out.extend_from_slice(&block(&raw[at..end])?);
        at = end;
    }
    Ok(out)
}

/// One block, mode byte and all.
fn block(raw: &[u8]) -> Result<Vec<u8>> {
    let Some((mode, body)) = raw.split_first() else {
        return Err(anyhow!("an empty blte block"));
    };
    match mode {
        b'N' => Ok(body.to_vec()),
        b'Z' => {
            let mut out = Vec::new();
            flate2::read::ZlibDecoder::new(body).read_to_end(&mut out)?;
            Ok(out)
        }
        // A block that is itself a blte stream. Deprecated, and harmless to
        // support: it is two lines.
        b'F' => decode(body),
        b'E' => Err(anyhow!(
            "this block is encrypted, and harmony ships no keys for it"
        )),
        other => Err(anyhow!(
            "blte block mode {:?} is one harmony does not read",
            *other as char
        )),
    }
}
