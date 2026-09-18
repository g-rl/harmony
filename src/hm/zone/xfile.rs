use std::path::Path;

use anyhow::{Result, anyhow};

use crate::hm::pack::oodle::Oodle;

pub const MAGIC: &[u8; 8] = b"IWffa100";
pub const PATCH_MAGIC: &[u8; 8] = b"IWffd100";

const IWC: u32 = 0x4357_4902;
const IWFF: u32 = 0x6666_5749;
const RSA_CHUNK: usize = 0x4000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flavour {
    Mw19,
    Mwii,
    Mwiii,
    Bo6,
}

impl Flavour {
    pub fn from_header_version(version: u32) -> Option<Flavour> {
        match version {
            0x0B => Some(Flavour::Mw19),
            0x17 => Some(Flavour::Mwii),
            0x18 => Some(Flavour::Mwiii),
            0x19 => Some(Flavour::Bo6),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Flavour::Mw19 => "mw19",
            Flavour::Mwii => "mwii",
            Flavour::Mwiii => "mwiii",
            Flavour::Bo6 => "bo6",
        }
    }
}

#[derive(Clone, Debug)]
pub struct XFileHeader {
    pub flavour: Flavour,
    pub header_version: u32,
    pub xfile_version: u32,
    pub flags: u32,
    pub file_size: u32,
    pub size: u64,
    pub preload_walk: u64,
    pub blocks: Vec<u64>,
    pub encrypted: bool,
}

fn u32at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn u64at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

pub fn read_header(raw: &[u8]) -> Option<XFileHeader> {
    if raw.len() < 0xE0 || &raw[0..8] != MAGIC {
        return None;
    }
    let header_version = u32at(raw, 8);
    let flavour = Flavour::from_header_version(header_version)?;
    let (block_at, count, encrypted_at) = match flavour {
        Flavour::Mw19 => (0x30, 8, 0x70),
        Flavour::Mwii => (0x48, 16, 0xC8),
        Flavour::Mwiii | Flavour::Bo6 => (0x48, 16, 0xC8),
    };
    Some(XFileHeader {
        flavour,
        header_version,
        xfile_version: u32at(raw, 12),
        flags: u32at(raw, 0x10),
        file_size: u32at(raw, 0x14),
        size: u64at(raw, block_at - 0x10),
        preload_walk: u64at(raw, block_at - 8),
        blocks: (0..count).map(|i| u64at(raw, block_at + i * 8)).collect(),
        encrypted: raw.len() > encrypted_at + 4 && u32at(raw, encrypted_at) != 0,
    })
}

pub fn peek(path: &Path) -> Option<XFileHeader> {
    let mut raw = vec![0u8; 0xE0];
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    file.read_exact(&mut raw).ok()?;
    read_header(&raw)
}

pub struct Zone {
    pub header: XFileHeader,
    pub data: Vec<u8>,
}

pub fn open(path: &Path, oodle: Option<&Oodle>) -> Result<Zone> {
    let raw = std::fs::read(path)?;
    decompress(&raw, oodle)
}

pub fn decompress(raw: &[u8], oodle: Option<&Oodle>) -> Result<Zone> {
    let header = read_header(raw).ok_or_else(|| anyhow!("not a fastfile this reader knows"))?;
    if header.flavour == Flavour::Mw19 {
        return Err(anyhow!("mw19 fastfiles are not read yet"));
    }

    let mut at = 0xE0usize;
    while at + 4 <= raw.len() && u32at(raw, at) != IWC {
        at += 1;
    }
    if at + 4 > raw.len() {
        return Err(anyhow!("no iwc marker"));
    }
    at += 4;

    let mut secure = false;
    if at + 4 <= raw.len() && u32at(raw, at) == IWFF {
        secure = true;
        at += RSA_CHUNK * 2;
    }
    if at + 8 > raw.len() {
        return Err(anyhow!("fastfile ends before its compression header"));
    }
    let compression = raw[at + 7];
    at += 8;

    let mut data: Vec<u8> = Vec::with_capacity(header.size as usize);
    let mut index = 0usize;
    loop {
        if secure && (index & 0x1FF) == 0x1FF {
            if at + RSA_CHUNK > raw.len() {
                break;
            }
            at += RSA_CHUNK;
        }
        if at + 12 > raw.len() {
            break;
        }
        let packed = u32at(raw, at) as usize;
        let plain = u32at(raw, at + 4) as usize;
        at += 12;
        if packed == 0 {
            break;
        }
        let aligned = (packed + 3) & !3usize;
        if at + aligned > raw.len() {
            break;
        }
        let src = &raw[at..at + packed];
        let base = data.len();
        data.resize(base + plain, 0);
        match compression {
            1 => {
                let n = packed.min(plain);
                data[base..base + n].copy_from_slice(&src[..n]);
            }
            4 | 5 => {
                lz4_flex::block::decompress_into(src, &mut data[base..base + plain])
                    .map_err(|e| anyhow!("lz4: {e}"))?;
            }
            _ => {
                let oodle = oodle.ok_or_else(|| anyhow!("oodle needed, none loaded"))?;
                oodle.run(src, &mut data[base..base + plain])?;
            }
        }
        at += aligned;
        index += 1;
    }

    Ok(Zone { header, data })
}
