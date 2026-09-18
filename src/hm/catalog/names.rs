use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

use crate::hm::catalog::hash::HashFn;

#[derive(Default)]
pub struct NameDb {
    by_hash: HashMap<u64, String>,
    by_key: HashMap<u64, String>,
    pub sources: Vec<String>,
}

impl NameDb {
    pub fn len(&self) -> usize {
        self.by_hash.len() + self.by_key.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn name_of_hash(&self, hash: u64) -> Option<&str> {
        self.by_hash.get(&hash).map(|s| s.as_str())
    }

    pub fn name_of_key(&self, key: u64) -> Option<&str> {
        self.by_key.get(&key).map(|s| s.as_str())
    }

    /// The name for a key, whichever map holds it and whichever of the masks
    /// it was written down under.
    ///
    /// A package keeps sixty bits of an asset hash and the published databases
    /// keep sixty three, so the same name can be filed under three different
    /// numbers. Asking under all of them costs a few lookups and saves the
    /// user from a list that silently matches nothing.
    pub fn lookup(&self, key: u64) -> Option<&str> {
        for value in [key, key & crate::hm::catalog::hash::ASSET_MASK, key & crate::hm::catalog::hash::NAME_MASK] {
            if let Some(name) = self.by_key.get(&value).or_else(|| self.by_hash.get(&value)) {
                return Some(name.as_str());
            }
        }
        None
    }

    pub fn insert_key(&mut self, key: u64, name: String) {
        self.by_key.insert(key, name);
    }

    pub fn load_wordlist(&mut self, path: &Path, hash: HashFn) -> Result<usize> {
        let text = std::fs::read_to_string(path)?;
        let mut added = 0usize;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let value = hash.of(line);
            let name = line.to_ascii_lowercase();
            for key in [
                value,
                value & crate::hm::catalog::hash::ASSET_MASK,
                value & crate::hm::catalog::hash::NAME_MASK,
            ] {
                self.by_hash.entry(key).or_insert_with(|| name.clone());
            }
            added += 1;
        }
        self.sources.push(format!(
            "{} ({added})",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
        Ok(added)
    }

    pub fn load_pairs(&mut self, path: &Path) -> Result<usize> {
        let text = std::fs::read_to_string(path)?;
        let mut added = 0usize;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((left, right)) = line.split_once(',').or_else(|| line.split_once('\t')) else {
                continue;
            };
            let left = left.trim().trim_start_matches("0x");
            let Ok(value) = u64::from_str_radix(left, 16) else {
                continue;
            };
            let name = right.trim().to_ascii_lowercase();
            // Filed under the hash as written and under both masks, so a list
            // written for one game's tables still answers for another's.
            for key in [
                value,
                value & crate::hm::catalog::hash::ASSET_MASK,
                value & crate::hm::catalog::hash::NAME_MASK,
            ] {
                self.by_hash.entry(key).or_insert_with(|| name.clone());
                self.by_key.entry(key).or_insert_with(|| name.clone());
            }
            added += 1;
        }
        self.sources.push(format!(
            "{} ({added})",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
        Ok(added)
    }
}
