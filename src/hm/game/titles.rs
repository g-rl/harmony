use std::path::Path;

use anyhow::{Result, anyhow};

use crate::hm::game::jup::{Jup, build_from_bootstrap, kapi_survey, packages_in};
use crate::hm::game::{Fingerprint, Mount, Progress, Support, Title, TitleId};
use crate::hm::pack::store::Store;
use crate::hm::pack::{PackageSet, oodle::Oodle};
use crate::hm::zone::{ZoneSet, jup as pools};

pub fn all() -> Vec<Box<dyn Title>> {
    let mut titles: Vec<Box<dyn Title>> = vec![
        Box::new(Jup),
        Box::new(Kapi {
            id: TitleId::Iw9,
            label: "modern warfare ii",
            status: Support::Expected,
            marker: "sp22",
            roots: &["zone", "sp22", "mp22"],
            version: 21,
        }),
        Box::new(Kapi {
            id: TitleId::Iw8,
            label: "modern warfare 2019",
            status: Support::DetectOnly,
            marker: "zone",
            roots: &["zone"],
            version: 10,
        }),
        Box::new(Kapi {
            id: TitleId::T10,
            label: "black ops 6",
            status: Support::DetectOnly,
            marker: "bo6",
            roots: &["zone"],
            version: 25,
        }),
        Box::new(Kapi {
            id: TitleId::T11,
            label: "black ops 7",
            status: Support::DetectOnly,
            marker: "bo7",
            roots: &["zone"],
            version: 26,
        }),
        Box::new(Casc {
            id: TitleId::H2,
            label: "modern warfare 2 campaign remastered",
            product: "lazr",
        }),
    ];
    titles.extend(crate::hm::game::legacy::all());
    titles
}

/// Modern Warfare 2 Campaign Remastered, and any other Call of Duty installed
/// through Battle.net the same way.
///
/// There are no containers in this install at all. Everything is inside CASC,
/// Blizzard's own content store: `Data\data\data.NNN` archives addressed by
/// the `.idx` files beside them, with a `.build.info` at the root naming the
/// product. Reading it means an index reader, a BLTE decoder and this game's
/// own root table before a single asset is in hand, so harmony recognises the
/// install and says plainly that it cannot read it yet, rather than pretending
/// the folder is empty.
pub struct Casc {
    pub id: TitleId,
    pub label: &'static str,
    /// The product code Battle.net knows this game by, as `.build.info` spells
    /// it: `lazr` is modern warfare 2 campaign remastered.
    pub product: &'static str,
}

const CASC_ROOTS: &[&str] = &["Data"];
const CASC_CONTAINERS: &[&str] = &["idx", "index"];

impl Title for Casc {
    fn id(&self) -> TitleId {
        self.id
    }

    fn label(&self) -> &'static str {
        self.label
    }

    fn status(&self) -> Support {
        Support::Partial
    }

    fn sound_roots(&self) -> &'static [&'static str] {
        CASC_ROOTS
    }

    fn containers(&self) -> &'static [&'static str] {
        CASC_CONTAINERS
    }

    fn fingerprint(&self, root: &Path) -> Option<Fingerprint> {
        let info = std::fs::read_to_string(root.join(".build.info")).ok()?;
        let lower = info.to_ascii_lowercase();
        if !lower.contains(self.product) {
            return None;
        }
        let archives = std::fs::read_dir(root.join("Data").join("data"))
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| {
                        entry
                            .file_name()
                            .to_string_lossy()
                            .to_ascii_lowercase()
                            .starts_with("data.")
                    })
                    .count()
            })
            .unwrap_or(0);
        // The version is on the last line of `.build.info`, in its own column.
        let build = lower
            .lines()
            .last()
            .and_then(|line| {
                line.split('|')
                    .find(|field| field.matches('.').count() == 3 && field.starts_with(|c: char| c.is_ascii_digit()))
            })
            .map(|version| version.to_string());
        Some(Fingerprint {
            title: self.id,
            score: 96,
            roots: vec![root.join("Data").join("data")],
            reason: format!("battle.net casc storage, {archives} archives, product {}", self.product),
            build,
        })
    }

    fn mount(&self, root: &Path, report: &mut dyn Progress) -> Result<Mount> {
        let mut step = |at: usize, of: usize, what: &str| report.step(at, of, what);
        let storage = std::sync::Arc::new(crate::hm::pack::casc::Storage::open(root, &mut step)?);
        let build = storage.build.clone();
        // The sound paks inside are the same streams ghosts and advanced
        // warfare keep on disk, under the same name.
        let set = crate::hm::pack::store::CascSet::open(storage, &["soundfile"]);
        if set.info.is_empty() {
            return Err(anyhow!("no sound paks in this casc storage"));
        }
        report.step(1, 1, "mounted");
        Ok(Mount {
            title: self.id,
            root: root.to_path_buf(),
            store: Store::Casc(set),
            zones: crate::hm::zone::ZoneSet::default(),
            oodle: None,
            build,
        })
    }

    fn sound_pool(&self) -> u64 {
        0
    }
}

pub struct Kapi {
    pub id: TitleId,
    pub label: &'static str,
    pub status: Support,
    pub marker: &'static str,
    pub roots: &'static [&'static str],
    pub version: u16,
}

const CONTAINERS: &[&str] = &["xsub", "xpak"];

impl Title for Kapi {
    fn id(&self) -> TitleId {
        self.id
    }

    fn label(&self) -> &'static str {
        self.label
    }

    fn status(&self) -> Support {
        self.status
    }

    fn sound_roots(&self) -> &'static [&'static str] {
        self.roots
    }

    fn containers(&self) -> &'static [&'static str] {
        CONTAINERS
    }

    fn fingerprint(&self, root: &Path) -> Option<Fingerprint> {
        if root.join("cod23").is_dir() {
            return None;
        }
        // Black Ops III writes `.xpak` too, with the same header version as
        // modern warfare 2019, and keeps its audio nowhere near them. Its own
        // sound folder says plainly which install this is.
        if root.join("zone").join("snd").is_dir() {
            return None;
        }
        let (count, version) = kapi_survey(root, self.roots, CONTAINERS);
        if count == 0 {
            return None;
        }
        let marker = root.join(self.marker).exists();
        let score = match (version == Some(self.version), marker) {
            (true, true) => 95,
            (true, false) => 80,
            (false, true) => 45,
            (false, false) => 10,
        };
        Some(Fingerprint {
            title: self.id,
            score,
            roots: self.roots.iter().map(|r| root.join(r)).collect(),
            reason: format!(
                "{count} kapi packages, header version {}",
                version.map(|v| v.to_string()).unwrap_or("?".into())
            ),
            build: build_from_bootstrap(root),
        })
    }

    fn mount(&self, root: &Path, report: &mut dyn Progress) -> Result<Mount> {
        let oodle = Oodle::find(root).ok();
        let paths = packages_in(root, self.roots, CONTAINERS);
        if paths.is_empty() {
            return Err(anyhow!("no kapi packages under this install"));
        }
        let mut packages = PackageSet::default();
        let total = paths.len();
        for (i, path) in paths.iter().enumerate() {
            report.step(i, total, "mounting");
            let _ = packages.mount(path);
        }
        report.step(total, total, "mounted");
        Ok(Mount {
            title: self.id,
            root: root.to_path_buf(),
            store: Store::Kapi(packages),
            zones: ZoneSet::default(),
            oodle,
            build: build_from_bootstrap(root),
        })
    }

    fn sound_pool(&self) -> u64 {
        pools::ASSET_SNDASSET
    }
}
