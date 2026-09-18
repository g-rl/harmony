pub mod assets;
pub mod iw5;
pub mod jup;
pub mod xfile;

use std::path::{Path, PathBuf};

use crate::hm::pack::oodle::Oodle;

#[derive(Clone, Debug)]
pub struct ZoneInfo {
    pub path: PathBuf,
    pub name: String,
    pub flavour: xfile::Flavour,
    pub size: u64,
    pub assets: usize,
    pub sounds: usize,
}

#[derive(Default)]
pub struct ZoneSet {
    pub zones: Vec<ZoneInfo>,
}

impl ZoneSet {
    pub fn survey(&mut self, path: &Path, sound_type: u64, oodle: Option<&Oodle>) -> bool {
        let Ok(zone) = xfile::open(path, oodle) else {
            return false;
        };
        let inventory = assets::inventory(&zone.data);
        self.zones.push(ZoneInfo {
            path: path.to_path_buf(),
            name: path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
            flavour: zone.header.flavour,
            size: zone.header.size,
            assets: inventory.as_ref().map(|i| i.assets).unwrap_or(0),
            sounds: inventory
                .as_ref()
                .and_then(|i| i.by_type.get(&sound_type).copied())
                .unwrap_or(0),
        });
        true
    }

    pub fn sounds(&self) -> usize {
        self.zones.iter().map(|z| z.sounds).sum()
    }
}
