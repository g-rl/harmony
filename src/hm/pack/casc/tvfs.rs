//! TVFS: the file tree of a modern CASC storage.
//!
//! The root file of a Call of Duty on Battle.net is a TVFS manifest: a path
//! table of folder and file names, a span table saying which part of which
//! stored file each one is, and a container table holding the encoding keys.
//! Walking it gives what harmony actually wants — real paths, and a key to
//! read each one with.

use anyhow::{Result, anyhow};

pub const MAGIC: &[u8; 4] = b"TVFS";

/// One file in the storage.
#[derive(Clone, Debug)]
pub struct File {
    pub path: String,
    /// The key of the first span, which is the whole file for all but the
    /// biggest.
    pub ekey: Vec<u8>,
    /// How many bytes the file is, all its spans together.
    pub size: u32,
    /// The pieces the file is stored as. A sound bank past a gigabyte is cut
    /// into several, each its own blob; everything smaller is one span.
    pub spans: Vec<Span>,
}

/// One stored piece of a file: where it starts in the file, how long it is,
/// and the blob that holds it.
#[derive(Clone, Debug)]
pub struct Span {
    pub offset: u32,
    pub size: u32,
    pub ekey: Vec<u8>,
}

struct Header {
    ekey_size: usize,
    path_at: usize,
    path_size: usize,
    vfs_at: usize,
    vfs_size: usize,
    cft_at: usize,
    cft_size: usize,
    flags: u32,
}

/// How wide an offset into a table is, which depends on how big the table is.
fn offset_width(size: usize) -> usize {
    match size {
        0..=0xFF => 1,
        0x100..=0xFFFF => 2,
        0x10000..=0xFFFFFF => 3,
        _ => 4,
    }
}

fn be(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0u64, |value, b| (value << 8) | *b as u64)
}

pub fn parse(raw: &[u8]) -> Result<Vec<File>> {
    if raw.len() < 0x26 || &raw[..4] != MAGIC {
        return Err(anyhow!("not a tvfs manifest"));
    }
    let head = Header {
        ekey_size: raw[6] as usize,
        flags: u32::from_be_bytes(raw[0x08..0x0C].try_into()?),
        path_at: u32::from_be_bytes(raw[0x0C..0x10].try_into()?) as usize,
        path_size: u32::from_be_bytes(raw[0x10..0x14].try_into()?) as usize,
        vfs_at: u32::from_be_bytes(raw[0x14..0x18].try_into()?) as usize,
        vfs_size: u32::from_be_bytes(raw[0x18..0x1C].try_into()?) as usize,
        cft_at: u32::from_be_bytes(raw[0x1C..0x20].try_into()?) as usize,
        cft_size: u32::from_be_bytes(raw[0x20..0x24].try_into()?) as usize,
    };
    if head.path_at + head.path_size > raw.len() || head.vfs_at + head.vfs_size > raw.len() {
        return Err(anyhow!("tvfs tables run past the end of the manifest"));
    }

    let mut files = Vec::new();
    let path_table = &raw[head.path_at..head.path_at + head.path_size];
    walk(raw, &head, path_table, String::new(), &mut files)?;
    Ok(files)
}

/// One level of the path table.
///
/// Names are length-prefixed fragments. A zero before a name means the path
/// separator goes in front of it, a zero after means it goes behind, and
/// `0xFF` introduces a node value: either a folder, whose contents follow
/// inside the length it names, or a file, which indexes the span table.
fn walk(
    raw: &[u8],
    head: &Header,
    table: &[u8],
    prefix: String,
    out: &mut Vec<File>,
) -> Result<()> {
    let mut at = 0usize;
    let mut here = prefix.clone();

    while at < table.len() {
        if table[at] == 0 {
            // A separator in front of the next fragment.
            if !here.is_empty() && !here.ends_with('/') {
                here.push('/');
            }
            at += 1;
            if at >= table.len() {
                break;
            }
        }
        if table[at] == 0xFF {
            if at + 5 > table.len() {
                break;
            }
            let value = u32::from_be_bytes(table[at + 1..at + 5].try_into()?);
            at += 5;
            if value & 0x8000_0000 != 0 {
                // A folder: its own path table follows, as long as the value
                // says, and the four bytes of that length are part of it.
                let length = (value & 0x7FFF_FFFF) as usize;
                let inner_end = (at + length.saturating_sub(4)).min(table.len());
                walk(raw, head, &table[at..inner_end], here.clone(), out)?;
                at = inner_end;
            } else if let Some(file) = span(raw, head, value as usize, &here) {
                out.push(file);
            }
            // Either way the name that led here is spent.
            here = prefix.clone();
            continue;
        }

        let length = table[at] as usize;
        at += 1;
        let end = (at + length).min(table.len());
        here.push_str(&String::from_utf8_lossy(&table[at..end]));
        at = end;
    }
    Ok(())
}

/// The span table entry a file node points at, and the container entries
/// behind its spans.
fn span(raw: &[u8], head: &Header, at: usize, path: &str) -> Option<File> {
    let vfs = raw.get(head.vfs_at..head.vfs_at + head.vfs_size)?;
    let entry = vfs.get(at..)?;
    if entry.len() < 1 {
        return None;
    }
    let count = entry[0] as usize;
    if count == 0 {
        return None;
    }
    let cft_width = offset_width(head.cft_size);
    let cft = raw.get(head.cft_at..head.cft_at + head.cft_size)?;
    // Harmony never writes to a storage, so the patch and content key fields
    // that may follow the key are of no use here.
    let _ = head.flags;

    // Each span's own offset field is where it starts in its blob, which is
    // zero for every one of them: the spans follow each other, and where one
    // starts in the file is the sum of those before it.
    let row = 8 + cft_width;
    let mut spans = Vec::with_capacity(count);
    let mut offset = 0u64;
    for i in 0..count {
        let body = entry.get(1 + i * row..1 + (i + 1) * row)?;
        let size = u32::from_be_bytes(body[4..8].try_into().ok()?);
        let cft_at = be(&body[8..8 + cft_width]) as usize;
        let ekey = cft.get(cft_at..cft_at + head.ekey_size)?.to_vec();
        spans.push(Span {
            offset: offset.min(u32::MAX as u64) as u32,
            size,
            ekey,
        });
        offset += size as u64;
    }
    let size = offset.min(u32::MAX as u64) as u32;

    Some(File {
        path: path.trim_start_matches('/').to_ascii_lowercase(),
        ekey: spans[0].ekey.clone(),
        size,
        spans,
    })
}

#[cfg(test)]
pub fn offset_width_for_test(size: usize) -> usize {
    offset_width(size)
}
