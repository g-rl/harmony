//! BLTE: the block container everything in a CASC storage is wrapped in.
//!
//! A header names the blocks, and each block carries a mode byte saying how it
//! was stored: `N` as it is, `Z` deflated, `F` another BLTE inside, `E`
//! encrypted. Harmony ships no keys, so an encrypted block is reported rather
//! than guessed at.

use std::io::Read;

use anyhow::{Result, anyhow};

pub const MAGIC: &[u8; 4] = b"BLTE";

/// The block table at the front of a stream: where each block's packed bytes
/// sit, and how many bytes they unpack to.
pub struct Table {
    /// How far into the stream the first block starts.
    pub header_size: usize,
    /// `(packed, plain)` per block, in order.
    pub blocks: Vec<(usize, usize)>,
}

/// How much of a stream is needed to read its whole table.
pub fn table_len(head: &[u8]) -> Result<usize> {
    if head.len() < 8 || &head[..4] != MAGIC {
        return Err(anyhow!("not a blte stream"));
    }
    let header_size = u32::from_be_bytes(head[4..8].try_into()?) as usize;
    Ok(header_size.max(8))
}

/// The block table, out of the first `table_len` bytes of a stream.
///
/// A stream with no header is one block and its plain size is unknown until
/// it is decoded, which is reported as a plain size of zero.
pub fn table(head: &[u8]) -> Result<Table> {
    let header_size = table_len(head)?;
    if header_size == 8 {
        return Ok(Table {
            header_size: 8,
            blocks: vec![(0, 0)],
        });
    }
    if head.len() < header_size || head.len() < 12 {
        return Err(anyhow!("blte header is cut short"));
    }
    let count = u32::from_be_bytes([0, head[9], head[10], head[11]]) as usize;
    let table = &head[12..];
    // 0x0F entries are size, size, md5; 0x10 adds the decoded md5.
    let entry_len = match head[8] {
        0x10 => 40,
        _ => 24,
    };
    if table.len() < count * entry_len {
        return Err(anyhow!("blte block table is cut short"));
    }
    let blocks = (0..count)
        .map(|i| {
            let entry = &table[i * entry_len..];
            let packed = u32::from_be_bytes(entry[0..4].try_into().unwrap()) as usize;
            let plain = u32::from_be_bytes(entry[4..8].try_into().unwrap()) as usize;
            (packed, plain)
        })
        .collect();
    Ok(Table { header_size, blocks })
}

/// Decode a whole BLTE stream.
pub fn decode(raw: &[u8]) -> Result<Vec<u8>> {
    let table = table(raw)?;
    // No header means one block, the rest of the stream.
    if table.header_size == 8 {
        return block(&raw[8..]);
    }
    let mut out = Vec::new();
    let mut at = table.header_size;
    for (packed, _) in table.blocks {
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
pub fn decode_block(raw: &[u8]) -> Result<Vec<u8>> {
    block(raw)
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
