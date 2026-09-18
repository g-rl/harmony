//! `.iwd` archives: the zip files the older id-derived engines ship their raw
//! assets in. World at War and Modern Warfare 2 keep their sounds here as plain
//! `sound/...` paths, which means these titles come with names.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

const END_OF_DIRECTORY: u32 = 0x0605_4b50;
const DIRECTORY_ENTRY: u32 = 0x0201_4b50;
const LOCAL_HEADER: u32 = 0x0403_4b50;
/// The tail the end-of-directory record can hide in, comment included.
const TAIL: usize = 66_000;

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub header_at: u64,
    pub packed: u64,
    pub plain: u64,
    pub method: u16,
}

pub struct Archive {
    pub path: PathBuf,
    pub entries: Vec<Entry>,
    file: File,
}

fn u16at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap())
}

fn u32at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

/// Is this a sound file worth cataloguing?
pub fn is_audio(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".wav", ".mp3", ".flac", ".ogg"]
        .iter()
        .any(|end| lower.ends_with(end))
}

impl Archive {
    pub fn open(path: &Path) -> Result<Archive> {
        let mut file = File::open(path)?;
        let size = file.metadata()?.len();
        let take = TAIL.min(size as usize);
        file.seek(SeekFrom::Start(size - take as u64))?;
        let mut tail = vec![0u8; take];
        file.read_exact(&mut tail)?;

        let at = (0..tail.len().saturating_sub(21))
            .rev()
            .find(|i| u32at(&tail, *i) == END_OF_DIRECTORY)
            .ok_or_else(|| anyhow!("no zip directory in {}", path.display()))?;
        let count = u16at(&tail, at + 10) as usize;
        let directory_at = u32at(&tail, at + 16) as u64;
        let directory_size = u32at(&tail, at + 12) as usize;

        file.seek(SeekFrom::Start(directory_at))?;
        let mut directory = vec![0u8; directory_size];
        file.read_exact(&mut directory)?;

        let mut entries = Vec::with_capacity(count);
        let mut cursor = 0usize;
        while cursor + 46 <= directory.len() {
            if u32at(&directory, cursor) != DIRECTORY_ENTRY {
                break;
            }
            let method = u16at(&directory, cursor + 10);
            let packed = u32at(&directory, cursor + 20) as u64;
            let plain = u32at(&directory, cursor + 24) as u64;
            let name_len = u16at(&directory, cursor + 28) as usize;
            let extra_len = u16at(&directory, cursor + 30) as usize;
            let comment_len = u16at(&directory, cursor + 32) as usize;
            let header_at = u32at(&directory, cursor + 42) as u64;
            let name_at = cursor + 46;
            if name_at + name_len > directory.len() {
                break;
            }
            let name = String::from_utf8_lossy(&directory[name_at..name_at + name_len])
                .replace('\\', "/")
                .to_ascii_lowercase();
            cursor = name_at + name_len + extra_len + comment_len;
            if name.ends_with('/') {
                continue;
            }
            entries.push(Entry {
                name,
                header_at,
                packed,
                plain,
                method,
            });
        }

        Ok(Archive {
            path: path.to_path_buf(),
            entries,
            file,
        })
    }

    pub fn name(&self) -> String {
        self.path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    /// Read one member out. Stored and deflated members are both handled; a zip
    /// made with anything else says so rather than handing back rubbish.
    pub fn read(&mut self, index: usize) -> Result<Vec<u8>> {
        let entry = self
            .entries
            .get(index)
            .ok_or_else(|| anyhow!("no such member"))?
            .clone();

        self.file.seek(SeekFrom::Start(entry.header_at))?;
        let mut head = [0u8; 30];
        self.file.read_exact(&mut head)?;
        if u32at(&head, 0) != LOCAL_HEADER {
            return Err(anyhow!("{} has no local header", entry.name));
        }
        let skip = u16at(&head, 26) as u64 + u16at(&head, 28) as u64;
        self.file
            .seek(SeekFrom::Start(entry.header_at + 30 + skip))?;

        match entry.method {
            0 => {
                let mut packed = vec![0u8; entry.packed as usize];
                self.file.read_exact(&mut packed)?;
                Ok(packed)
            }
            8 => {
                let mut out = Vec::with_capacity(entry.plain as usize);
                let window = std::io::BufReader::new((&mut self.file).take(entry.packed));
                flate2::read::DeflateDecoder::new(window).read_to_end(&mut out)?;
                Ok(out)
            }
            other => Err(anyhow!("{} uses compression {other}", entry.name)),
        }
    }

    /// The first bytes of a member, for working out what it is without paying
    /// for the whole of it. A deflated member is inflated only as far as the
    /// bytes asked for, so this stays cheap either way.
    pub fn read_head(&mut self, index: usize, want: usize) -> Result<Vec<u8>> {
        let entry = self
            .entries
            .get(index)
            .ok_or_else(|| anyhow!("no such member"))?
            .clone();

        self.file.seek(SeekFrom::Start(entry.header_at))?;
        let mut head = [0u8; 30];
        self.file.read_exact(&mut head)?;
        if u32at(&head, 0) != LOCAL_HEADER {
            return Err(anyhow!("{} has no local header", entry.name));
        }
        let skip = u16at(&head, 26) as u64 + u16at(&head, 28) as u64;
        self.file
            .seek(SeekFrom::Start(entry.header_at + 30 + skip))?;

        match entry.method {
            0 => {
                let take = want.min(entry.packed as usize);
                let mut out = vec![0u8; take];
                self.file.read_exact(&mut out)?;
                Ok(out)
            }
            8 => {
                let mut out = Vec::with_capacity(want);
                let window = std::io::BufReader::new((&mut self.file).take(entry.packed));
                flate2::read::DeflateDecoder::new(window)
                    .take(want as u64)
                    .read_to_end(&mut out)?;
                Ok(out)
            }
            other => Err(anyhow!("{} uses compression {other}", entry.name)),
        }
    }
}
