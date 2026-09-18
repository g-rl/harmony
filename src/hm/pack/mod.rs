pub mod casc;
pub mod iwd;
pub mod kapi;
pub mod oodle;
pub mod sab;
pub mod store;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

use crate::hm::pack::kapi::{Entry, Package};
use crate::hm::pack::oodle::Oodle;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PackageId(pub u32);

#[derive(Clone, Debug)]
pub struct PackageInfo {
    pub id: PackageId,
    pub name: String,
    pub path: PathBuf,
    pub entries: usize,
    pub version: u16,
    pub kind: u64,
}

#[derive(Default)]
pub struct PackageSet {
    packages: Vec<Package>,
    pub info: Vec<PackageInfo>,
    index: HashMap<u64, (u32, u32)>,
}

impl PackageSet {
    pub fn mount(&mut self, path: &Path) -> Result<PackageId> {
        let package = Package::open(path)?;
        let id = PackageId(self.packages.len() as u32);
        let info = PackageInfo {
            id,
            name: package.name(),
            path: package.path.clone(),
            entries: package.entries.len(),
            version: package.header.version,
            kind: package.header.kind,
        };
        for (i, entry) in package.entries.iter().enumerate() {
            self.index.entry(entry.key).or_insert((id.0, i as u32));
        }
        self.packages.push(package);
        self.info.push(info);
        Ok(id)
    }

    pub fn len(&self) -> usize {
        self.packages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }

    pub fn keys(&self) -> usize {
        self.index.len()
    }

    pub fn entries_of(&self, id: PackageId) -> &[Entry] {
        &self.packages[id.0 as usize].entries
    }

    pub fn name_of(&self, id: PackageId) -> &str {
        &self.info[id.0 as usize].name
    }

    pub fn contains(&self, key: u64) -> bool {
        self.index.contains_key(&key)
    }

    pub fn read(&mut self, id: PackageId, index: usize, oodle: Option<&Oodle>) -> Result<Vec<u8>> {
        let package = self
            .packages
            .get_mut(id.0 as usize)
            .ok_or_else(|| anyhow!("no such package"))?;
        let entry = *package
            .entries
            .get(index)
            .ok_or_else(|| anyhow!("no such entry"))?;
        package.read(entry, oodle)
    }

    pub fn read_key(&mut self, key: u64, oodle: Option<&Oodle>) -> Result<Vec<u8>> {
        let (package, index) = *self
            .index
            .get(&key)
            .ok_or_else(|| anyhow!("key {key:016x} is in no package"))?;
        self.read(PackageId(package), index as usize, oodle)
    }
}
