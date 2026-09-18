//! CASC: the content store Battle.net installs a game into.
//!
//! There are no containers in such an install at all. Every asset is a blob in
//! one of the `Data\data\data.NNN` archives, addressed by an encoding key, and
//! the paths are in a manifest which is itself one of those blobs. Reading it
//! means four things in order: the local index, the encoding table, the root
//! manifest, and the blocks each file is wrapped in.
//!
//! Harmony reads it and never writes to it: the game's own files are left
//! exactly as they are.

pub mod blte;
pub mod encoding;
pub mod idx;
pub mod tvfs;

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

/// The 30 bytes an archive puts in front of every blob it holds.
const ENTRY_HEADER: usize = 30;

pub struct Storage {
    root: PathBuf,
    data: PathBuf,
    index: idx::Index,
    pub files: Vec<tvfs::File>,
    /// The product code from `.build.info`: `lazr` is modern warfare 2
    /// campaign remastered.
    pub product: String,
    pub build: Option<String>,
}

impl Storage {
    /// Open the storage in a game folder.
    pub fn open(root: &Path, report: &mut dyn FnMut(usize, usize, &str)) -> Result<Storage> {
        let data = root.join("Data").join("data");
        report(0, 4, "reading the index");
        let index = idx::read_folder(&data)?;

        report(1, 4, "reading the build");
        let info = std::fs::read_to_string(root.join(".build.info"))?;
        let build = build_info(&info);
        let config = build
            .get("Build Key")
            .ok_or_else(|| anyhow!("no build key in .build.info"))?;
        let product = build
            .get("Product")
            .cloned()
            .unwrap_or_else(|| "unknown".into());
        let version = build.get("Version").cloned();

        let config = config_file(root, config)?;
        let fields = config_fields(&config);

        let mut storage = Storage {
            root: root.to_path_buf(),
            data,
            index,
            files: Vec::new(),
            product,
            build: version,
        };

        report(2, 4, "reading the encoding table");
        // `encoding = <content key> <encoding key>`: the second one is what the
        // archives are addressed by, so no lookup is needed to find it.
        let encoding_keys = fields
            .get("encoding")
            .ok_or_else(|| anyhow!("no encoding table in the build config"))?;
        let encoding_ekey = encoding_keys
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| anyhow!("the build config names no encoding key"))?;
        let table = encoding::parse(&storage.read_key(&hex(encoding_ekey)?)?)?;

        report(3, 4, "reading the file tree");
        let root_ckey = fields
            .get("vfs-root")
            .or_else(|| fields.get("root"))
            .and_then(|value| value.split_whitespace().next())
            .ok_or_else(|| anyhow!("the build config names no root"))?;
        let root_ekey = table
            .ekey_of(&hex(root_ckey)?)
            .ok_or_else(|| anyhow!("the encoding table does not list the root"))?;
        let manifest = storage.read_key(&root_ekey)?;
        storage.files = tvfs::parse(&manifest)?;
        report(4, 4, "read");
        Ok(storage)
    }

    /// Everything in the storage whose path ends one of these ways.
    pub fn with_extension(&self, wanted: &[&str]) -> Vec<&tvfs::File> {
        self.files
            .iter()
            .filter(|file| {
                wanted
                    .iter()
                    .any(|ext| file.path.ends_with(&format!(".{ext}")))
            })
            .collect()
    }

    /// Read one file out of the storage by its encoding key.
    pub fn read_key(&self, ekey: &[u8]) -> Result<Vec<u8>> {
        let place = self
            .index
            .find(ekey)
            .ok_or_else(|| anyhow!("that key is not in this install"))?;
        let path = self.data.join(format!("data.{:03}", place.archive));
        let mut file = std::fs::File::open(&path)?;
        file.seek(SeekFrom::Start(place.offset))?;
        let mut raw = vec![0u8; place.size as usize];
        file.read_exact(&mut raw)?;
        if raw.len() <= ENTRY_HEADER {
            return Err(anyhow!("{} holds nothing at that offset", path.display()));
        }
        blte::decode(&raw[ENTRY_HEADER..])
    }

    /// Read one file out of the storage by its path.
    pub fn read_path(&self, path: &str) -> Result<Vec<u8>> {
        let wanted = path.to_ascii_lowercase();
        let file = self
            .files
            .iter()
            .find(|file| file.path == wanted)
            .ok_or_else(|| anyhow!("{path} is not in this install"))?;
        self.read_key(&file.ekey)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// `.build.info` is a two line table: a header of `Name!TYPE:SIZE` columns and
/// the row for the installed build.
fn build_info(text: &str) -> HashMap<String, String> {
    let mut lines = text.lines();
    let (Some(head), Some(row)) = (lines.next(), lines.next()) else {
        return HashMap::new();
    };
    head.split('|')
        .map(|column| column.split('!').next().unwrap_or(column).to_string())
        .zip(row.split('|').map(|value| value.to_string()))
        .collect()
}

/// Config files live under `Data\config\xx\yy\<hash>`, by the first two pairs
/// of hex digits of their own name.
fn config_file(root: &Path, hash: &str) -> Result<String> {
    if hash.len() < 4 {
        return Err(anyhow!("{hash} is not a config hash"));
    }
    let path = root
        .join("Data")
        .join("config")
        .join(&hash[0..2])
        .join(&hash[2..4])
        .join(hash);
    Ok(std::fs::read_to_string(path)?)
}

/// A config file is `key = value` lines and comments.
fn config_fields(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        .collect()
}

fn hex(text: &str) -> Result<Vec<u8>> {
    if text.len() % 2 != 0 {
        return Err(anyhow!("{text} is not a hash"));
    }
    (0..text.len())
        .step_by(2)
        .map(|at| {
            u8::from_str_radix(&text[at..at + 2], 16)
                .map_err(|_| anyhow!("{text} is not a hash"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_info_reads_its_two_lines() {
        let text = "Branch!STRING:0|Product!STRING:0\nus|lazr\n";
        let fields = build_info(text);
        assert_eq!(fields.get("Product").map(String::as_str), Some("lazr"));
    }

    #[test]
    fn a_config_is_key_equals_value() {
        let fields = config_fields("# Build Configuration\n\nroot = abc\nencoding = one two\n");
        assert_eq!(fields.get("root").map(String::as_str), Some("abc"));
        assert_eq!(fields.get("encoding").map(String::as_str), Some("one two"));
    }

    #[test]
    fn table_offsets_widen_with_the_table() {
        assert_eq!(tvfs::offset_width_for_test(0xFF), 1);
        assert_eq!(tvfs::offset_width_for_test(0x1_0000), 3);
    }
}
