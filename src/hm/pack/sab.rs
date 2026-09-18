use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Result, anyhow};

pub const MAGIC: u32 = 0x2358_5532;

#[derive(Clone, Debug)]
pub struct BankHeader {
    pub version: u32,
    pub entry_size: u32,
    pub hash_size: u32,
    pub name_size: u32,
    pub entry_count: u32,
    pub file_size: u64,
    pub entry_offset: u64,
    pub hash_offset: u64,
}

#[derive(Clone, Debug)]
pub struct BankEntry {
    pub key: u64,
    pub offset: u64,
    pub size: u32,
    pub seek_table: u32,
    pub primed: u32,
    pub frames: u32,
    pub rate: u32,
    pub channels: u8,
    pub looping: bool,
    pub format: u8,
    pub name: Option<String>,
}

/// A sound bank, open but not read in.
///
/// Black Ops II ships nine and a half gigabytes of these and harmony mounts
/// all of them at once. Holding each one's bytes meant the whole install sat
/// in memory for as long as the tab was open, which is where eight gigabytes
/// of it went. The tables are small and are kept; the audio is read from the
/// file when a sound is actually played or extracted.
pub struct Bank {
    pub header: BankHeader,
    pub entries: Vec<BankEntry>,
    file: std::fs::File,
    len: u64,
}

impl Bank {
    /// `size` bytes from `at`, read now.
    pub fn bytes(&self, at: u64, size: usize) -> Result<Vec<u8>> {
        if at.saturating_add(size as u64) > self.len {
            return Err(anyhow!("entry runs past the end of the bank"));
        }
        let mut out = vec![0u8; size];
        let mut file = &self.file;
        file.seek(SeekFrom::Start(at))?;
        file.read_exact(&mut out)?;
        Ok(out)
    }

    pub fn len(&self) -> u64 {
        self.len
    }
}

fn u32at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn u64at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

const RATES: [u32; 8] = [8000, 12000, 16000, 24000, 32000, 44100, 48000, 96000];

pub fn is_bank(raw: &[u8]) -> bool {
    raw.len() > 0x40 && u32at(raw, 0) == MAGIC
}

pub fn read_header(raw: &[u8]) -> Option<BankHeader> {
    if !is_bank(raw) {
        return None;
    }
    Some(BankHeader {
        version: u32at(raw, 4),
        entry_size: u32at(raw, 8),
        hash_size: u32at(raw, 12),
        name_size: u32at(raw, 16),
        entry_count: u32at(raw, 20),
        file_size: u64at(raw, 32),
        entry_offset: u64at(raw, 40),
        hash_offset: u64at(raw, 48),
    })
}

/// Everything a bank says about itself lives in its first kilobyte, so that is
/// what is read to find the tables: the tables themselves sit at the far end of
/// the file and are fetched by seeking to them.
const HEAD: usize = 0x400;

/// As much of a name table as is worth reading. A quarter of a million names
/// at a hundred and twenty-eight bytes each is far past any bank that exists.
const NAMES_MOST: u64 = 32 << 20;

pub fn open(path: &Path) -> Result<Bank> {
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let mut head = vec![0u8; HEAD.min(len as usize)];
    file.read_exact(&mut head)?;

    let header = read_header(&head).ok_or_else(|| anyhow!("not a sab bank"))?;
    let count = header.entry_count as usize;

    // Where the names are, if this bank carries any. The older banks leave the
    // pointer at zero and are hashes all the way down.
    let names_at = match head.len() >= 0x258 {
        true => {
            let at = match header.version == 0x15 && header.entry_size == 0x38 {
                true => 0x24C,
                false => 0x250,
            };
            u64at(&head, at)
        }
        false => 0,
    };
    let names = match names_at > 0 && names_at < len {
        true => {
            // Everything from the names to whatever table comes after them.
            // The header's `name_size` is not the stride — infinite warfare
            // says sixty-four and writes them a hundred and twenty-eight
            // apart — so the end is taken from the file instead, and the
            // names are read out of it by shape.
            let after = [header.entry_offset, header.hash_offset, len]
                .into_iter()
                .filter(|at| *at > names_at)
                .min()
                .unwrap_or(len);
            let want = (after - names_at).min(NAMES_MOST) as usize;
            read_names(&span(&mut file, names_at, want)?, count)
        }
        false => Vec::new(),
    };

    let table_at = match header.version == 0x15 {
        true => u64at(&head, 36),
        false => header.entry_offset,
    };
    let stride = header.entry_size.max(1) as u64;
    if table_at >= len {
        return Err(anyhow!("truncated bank"));
    }
    let table = span(
        &mut file,
        table_at,
        ((count as u64 * stride).min(len - table_at)) as usize,
    )?;
    let entries = read_entries(&table, &header, &names);

    Ok(Bank {
        header,
        entries,
        file,
        len,
    })
}

fn span(file: &mut std::fs::File, at: u64, size: usize) -> Result<Vec<u8>> {
    let mut out = vec![0u8; size];
    file.seek(SeekFrom::Start(at))?;
    file.read_exact(&mut out)?;
    Ok(out)
}

/// `aliens\brute_swipe_01` as everything else here spells it.
fn slashed(name: &str) -> String {
    name.replace('\\', "/")
}

/// The names in a name table, in the order they are written.
///
/// Two shapes, and nothing in the header says which: names packed one after
/// another, and names in fixed-width slots padded out with zeroes. Reading
/// them as runs of text with the padding skipped covers both, which is what
/// infinite warfare needs — it writes a forty-four character name every
/// hundred and twenty-eight bytes and calls the stride sixty-four.
fn read_names(raw: &[u8], count: usize) -> Vec<String> {
    let mut names = Vec::with_capacity(count);
    let mut at = 0usize;
    while at < raw.len() && names.len() < count {
        if raw[at] == 0 {
            at += 1;
            continue;
        }
        let start = at;
        while at < raw.len() && raw[at] != 0 {
            at += 1;
        }
        names.push(String::from_utf8_lossy(&raw[start..at]).to_string());
    }
    names
}

/// The entry table, already fetched: one fixed-width row per sound.
fn read_entries(table: &[u8], header: &BankHeader, names: &[String]) -> Vec<BankEntry> {
    let stride = header.entry_size.max(1) as usize;
    let mut entries = Vec::with_capacity(header.entry_count as usize);

    for i in 0..header.entry_count as usize {
        let o = i * stride;
        if o + stride > table.len() {
            break;
        }
        let e = &table[o..o + stride];
        // The version alone does not say which table this is: black ops iii
        // writes version fifteen with thirty-six byte rows, and black ops ii
        // writes fourteen and fifteen with twenty. The row's own width settles
        // it.
        let entry = match (header.version, stride) {
            // Black ops iii: the same fields, spread out, with a sixty-four
            // bit offset and the shape bytes at the end of the first half.
            (0xF, 36) => BankEntry {
                key: u32at(e, 0) as u64,
                size: u32at(e, 4),
                frames: u32at(e, 8),
                offset: u64at(e, 16),
                rate: RATES.get(e[24] as usize).copied().unwrap_or(48000),
                channels: e[25],
                looping: e[26] != 0,
                format: e[27],
                seek_table: 0,
                primed: 0,
                name: names.get(i).map(|name| slashed(name)),
            },
            (0xE | 0xF, _) => BankEntry {
                key: u32at(e, 0) as u64,
                size: u32at(e, 4),
                offset: u32at(e, 8) as u64,
                frames: u32at(e, 12),
                rate: RATES
                    .get(e[16] as usize)
                    .copied()
                    .unwrap_or(48000),
                channels: e[17],
                looping: e[18] != 0,
                format: e[19],
                seek_table: 0,
                primed: 0,
                name: names.get(i).map(|name| slashed(name)),
            },
            (0x11 | 0x15, _) => BankEntry {
                key: u64at(e, 0),
                offset: u64at(e, 16),
                size: u32at(e, 24),
                seek_table: u32at(e, 28),
                frames: u32at(e, 32),
                primed: u32at(e, 36),
                rate: u32at(e, 40),
                channels: if stride > 45 { e[45] } else { 1 },
                looping: stride > 46 && e[46] != 0,
                format: if stride > 47 { e[47] } else { 0 },
                name: names.get(i).map(|name| slashed(name)),
            },
            _ => BankEntry {
                key: u32at(e, 0) as u64,
                size: u32at(e, 4),
                seek_table: u32at(e, 8),
                frames: u32at(e, 12),
                offset: u64at(e, 20),
                rate: u32at(e, 28),
                channels: e[32],
                looping: e[33] != 0,
                format: e[34],
                primed: 0,
                name: names.get(i).map(|name| slashed(name)),
            },
        };
        entries.push(entry);
    }
    entries
}
