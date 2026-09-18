use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

use crate::hm::pack::oodle::Oodle;

pub const MAGIC: u32 = 0x4950_414b;

const HEADER_SIZE: usize = 0x7E8;
const ENTRY_SIZE: usize = 20;
const BLOCK_SIZE: usize = 21;
const MAX_BLOB: usize = 0x240_0000;
/// How much of the data file to pull in at a time when serving small reads.
const WINDOW: usize = 0x40_0000;

#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub key: u64,
    pub offset: u64,
    pub size: usize,
}

#[derive(Clone, Debug)]
pub struct Header {
    pub version: u16,
    pub kind: u64,
    pub size: u64,
    pub file_count: u64,
    pub data_offset: u64,
    pub data_size: u64,
    pub hash_count: u64,
    pub hash_offset: u64,
    pub hash_size: u64,
}

pub struct Package {
    pub path: PathBuf,
    pub header: Header,
    pub entries: Vec<Entry>,
    file: File,
    /// A sliding window over the data file. Entries read in offset order, so
    /// one big sequential read serves many small ones and the disk head stays
    /// put: this is the difference between minutes and hours on a deep scan.
    cache: Vec<u8>,
    cache_at: u64,
}

fn u16at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap())
}

fn u32at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn u64at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

pub fn peek(path: &Path) -> Option<Header> {
    let mut file = File::open(path).ok()?;
    let mut raw = [0u8; HEADER_SIZE];
    file.read_exact(&mut raw).ok()?;
    read_header(&raw)
}

fn read_header(raw: &[u8]) -> Option<Header> {
    if raw.len() < HEADER_SIZE || u32at(raw, 0) != MAGIC {
        return None;
    }
    Some(Header {
        version: u16at(raw, 6),
        kind: u64at(raw, 0x10),
        size: u64at(raw, 0x18),
        file_count: u64at(raw, 0x788),
        data_offset: u64at(raw, 0x790),
        data_size: u64at(raw, 0x798),
        hash_count: u64at(raw, 0x7A0),
        hash_offset: u64at(raw, 0x7A8),
        hash_size: u64at(raw, 0x7B0),
    })
}

impl Package {
    pub fn open(path: &Path) -> Result<Package> {
        let mut file = File::open(path)?;
        let mut raw = [0u8; HEADER_SIZE];
        file.read_exact(&mut raw)?;
        let header = read_header(&raw).ok_or_else(|| anyhow!("not a kapi package"))?;

        let on_disk = file.metadata()?.len();
        if header.kind != 3 && header.kind != 1 {
            return Ok(Package {
                path: path.to_path_buf(),
                header,
                entries: Vec::new(),
                file,
                cache: Vec::new(),
                cache_at: u64::MAX,
            });
        }
        if header.hash_offset >= on_disk || header.hash_count == 0 {
            return Ok(Package {
                path: path.to_path_buf(),
                header,
                entries: Vec::new(),
                file,
                cache: Vec::new(),
                cache_at: u64::MAX,
            });
        }

        let count = header.hash_count as usize;
        // A row is a key and a packed word, and the newer titles pad each one
        // out to twenty bytes: black ops cold war writes them sixteen apart.
        // The header's own sizes say which.
        let stride = match (header.hash_size as usize).checked_div(count) {
            Some(stride) if (16..=32).contains(&stride) => stride,
            _ => ENTRY_SIZE,
        };
        file.seek(SeekFrom::Start(header.hash_offset))?;
        let mut table = vec![0u8; count * stride];
        file.read_exact(&mut table)?;

        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let at = i * stride;
            let packed = u64at(&table, at + 8);
            entries.push(Entry {
                key: u64at(&table, at),
                offset: (packed >> 32) << 7,
                size: ((packed >> 1) & 0x3FFF_FFFF) as usize,
            });
        }

        let data = if header.kind == 1 {
            let mut alt = path.as_os_str().to_os_string();
            alt.push("data");
            let alt = PathBuf::from(alt);
            if alt.is_file() { File::open(alt)? } else { file }
        } else {
            file
        };

        Ok(Package {
            path: path.to_path_buf(),
            header,
            entries,
            file: data,
            cache: Vec::new(),
            cache_at: u64::MAX,
        })
    }

    pub fn name(&self) -> String {
        self.path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    pub fn read(&mut self, entry: Entry, oodle: Option<&Oodle>) -> Result<Vec<u8>> {
        self.read_upto(entry, oodle, usize::MAX)
    }

    pub fn read_head(&mut self, entry: Entry, oodle: Option<&Oodle>, want: usize) -> Result<Vec<u8>> {
        self.read_upto(entry, oodle, want)
    }

    /// Serve `len` bytes at `pos` from the window, refilling it when the ask
    /// falls outside. Short reads at the end of the file are kept, so callers
    /// get whatever is really there.
    fn fetch(&mut self, pos: u64, len: usize) -> Result<&[u8]> {
        let held = self.cache_at != u64::MAX
            && pos >= self.cache_at
            && pos + len as u64 <= self.cache_at + self.cache.len() as u64;
        if !held {
            // Only read ahead when the caller is marching forward through the
            // file; a random read gets exactly what it asked for.
            let sequential = self.cache_at != u64::MAX
                && pos >= self.cache_at
                && pos <= self.cache_at + self.cache.len() as u64 + 0x10000;
            let span = if sequential { len.max(WINDOW) } else { len };
            self.cache.resize(span, 0);
            self.file.seek(SeekFrom::Start(pos))?;
            let mut filled = 0usize;
            while filled < span {
                match self.file.read(&mut self.cache[filled..]) {
                    Ok(0) => break,
                    Ok(n) => filled += n,
                    Err(error) => return Err(error.into()),
                }
            }
            self.cache.truncate(filled);
            self.cache_at = pos;
        }
        let from = (pos - self.cache_at) as usize;
        self.cache
            .get(from..from + len)
            .ok_or_else(|| anyhow!("read past the end of the package"))
    }

    fn read_upto(&mut self, entry: Entry, oodle: Option<&Oodle>, want: usize) -> Result<Vec<u8>> {
        if entry.size == 0 {
            return Err(anyhow!("empty entry"));
        }
        if self.header.version == T9_VERSION {
            return self.read_t9(entry, oodle, want);
        }

        // A block chain starts with the entry's own key; anything else is
        // stored as it lies and can be handed straight back.
        let keyed = self
            .fetch(entry.offset + 2, 8)
            .map(|probe| u64at(probe, 0) == entry.key)
            .unwrap_or(false);
        if !keyed {
            let take = entry.size.min(want.max(16));
            return Ok(self.fetch(entry.offset, take)?.to_vec());
        }

        // Grown on demand: a head read of a big blob should not cost a full
        // MAX_BLOB allocation, and zeroing 36 MB per entry dwarfs the decompress.
        let first = entry.size.saturating_mul(4).clamp(0x10000, MAX_BLOB);
        let mut out = vec![0u8; first.min(want.saturating_add(0x10000).max(0x10000))];
        let mut written = 0usize;
        let mut block_at = entry.offset;
        let end = entry.offset + entry.size as u64;
        let mut src: Vec<u8> = Vec::new();

        loop {
            if block_at >= end {
                break;
            }
            let Ok(count) = self.fetch(block_at + 22, 1).map(|raw| raw[0] as usize) else {
                break;
            };
            if count == 0 || count > 256 {
                break;
            }
            let blocks = self.fetch(block_at + 23, count * BLOCK_SIZE)?.to_vec();

            for i in 0..count {
                let b = &blocks[i * BLOCK_SIZE..i * BLOCK_SIZE + BLOCK_SIZE];
                let kind = b[0];
                let packed = u32at(b, 1) as usize;
                let plain = u32at(b, 5) as usize;
                let at = u32at(b, 9) as u64;
                let into = u32at(b, 13) as usize;
                if packed == 0 || into + plain > MAX_BLOB {
                    continue;
                }
                if into + plain > out.len() {
                    out.resize((into + plain).min(MAX_BLOB), 0);
                }
                src.clear();
                src.extend_from_slice(self.fetch(block_at + at, packed)?);
                match kind {
                    3 => {
                        lz4_flex::block::decompress_into(&src, &mut out[into..into + plain])
                            .map_err(|e| anyhow!("lz4: {e}"))?;
                        written = written.max(into + plain);
                    }
                    6 => {
                        let oodle = oodle.ok_or_else(|| anyhow!("oodle needed, none loaded"))?;
                        oodle.run(&src, &mut out[into..into + plain])?;
                        written = written.max(into + plain);
                    }
                    0 => {
                        let n = packed.min(plain);
                        out[into..into + n].copy_from_slice(&src[..n]);
                        written = written.max(into + n);
                    }
                    _ => {}
                }
                if written >= want {
                    break;
                }
            }

            if written >= want {
                break;
            }
            // The next chain header sits on the 128 byte boundary after the
            // last block this one described.
            let last = (0..count)
                .map(|i| {
                    let b = &blocks[i * BLOCK_SIZE..i * BLOCK_SIZE + BLOCK_SIZE];
                    u32at(b, 9) as u64 + u32at(b, 1) as u64
                })
                .max()
                .unwrap_or(0);
            let next = (block_at + last + 0x7F) & !0x7Fu64;
            if next <= block_at {
                break;
            }
            block_at = next;
        }

        if written == 0 {
            return Err(anyhow!("nothing came out of the block chain"));
        }
        out.truncate(written);
        Ok(out)
    }

    /// Black ops cold war's entry: a count of blocks, a word per block with
    /// its packed size in the low twenty-four bits and how it was packed in
    /// the top eight, the table rounded up to the next hundred and twenty-
    /// eight bytes, and then the blocks one after another. Each block starts
    /// with its own plain size, and is oodle after that.
    fn read_t9(&mut self, entry: Entry, oodle: Option<&Oodle>, want: usize) -> Result<Vec<u8>> {
        let count = u64at(self.fetch(entry.offset, 8)?, 0) as usize;
        if count == 0 || count > T9_BLOCKS_MOST {
            return Err(anyhow!("not a block table"));
        }
        let words = self.fetch(entry.offset + 8, count * 4)?.to_vec();
        let table = ((8 + count * 4 + 0x7F) & !0x7F) as u64;
        let end = entry.offset + entry.size as u64;
        let mut block_at = entry.offset + table;
        let mut out = Vec::new();
        for i in 0..count {
            let word = u32at(&words, i * 4);
            let packed = (word & 0x00FF_FFFF) as usize;
            let kind = word >> 24;
            if packed < 4 || block_at + packed as u64 > end {
                break;
            }
            let plain = u32at(self.fetch(block_at, 4)?, 0) as usize;
            if plain == 0 || plain > MAX_BLOB {
                break;
            }
            let src = self.fetch(block_at + 4, packed - 4)?.to_vec();
            match kind {
                T9_OODLE => {
                    let oodle = oodle.ok_or_else(|| anyhow!("oodle needed, none loaded"))?;
                    let at = out.len();
                    out.resize(at + plain, 0);
                    oodle.run(&src, &mut out[at..at + plain])?;
                }
                _ if packed - 4 == plain => out.extend_from_slice(&src),
                other => return Err(anyhow!("block kind {other} is not one harmony reads")),
            }
            block_at += packed as u64;
            if out.len() >= want {
                break;
            }
        }
        if out.is_empty() {
            return Err(anyhow!("nothing came out of the block table"));
        }
        Ok(out)
    }
}

/// The header version black ops cold war writes.
pub const T9_VERSION: u16 = 16;
/// The kind byte of an oodle block in a cold war entry.
const T9_OODLE: u32 = 9;
/// More blocks than any entry has: past this, the count is not a count.
const T9_BLOCKS_MOST: usize = 0x10000;
