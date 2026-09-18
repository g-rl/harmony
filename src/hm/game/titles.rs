use std::path::Path;

use anyhow::{Result, anyhow};

use crate::hm::game::jup::{Jup, build_from_bootstrap, kapi_survey, packages_in};
use crate::hm::game::{Fingerprint, Mount, Progress, Support, Title, TitleId};
use crate::hm::pack::store::Store;
use crate::hm::pack::{PackageSet, oodle::Oodle};
use crate::hm::zone::{ZoneSet, jup as pools};

pub fn all() -> Vec<Box<dyn Title>> {
    let mut titles: Vec<Box<dyn Title>> = vec![
        // The 2026 title keeps everything under `cod26\`, the way modern
        // warfare iii keeps it under `cod23\`: no `zone\` at all.
        Box::new(Kapi {
            id: TitleId::Rex,
            label: "modern warfare 4",
            status: Support::Partial,
            marker: "cod26",
            roots: &["cod26"],
            version: 23,
        }),
        Box::new(Jup),
        Box::new(Kapi {
            id: TitleId::Iw9,
            label: "modern warfare ii",
            status: Support::Expected,
            marker: "sp22",
            roots: &["zone", "sp22", "mp22"],
            version: 21,
        }),
        // Black Ops Cold War keeps the same `.xsub` and `.xpak` packages under
        // `zone\`, with sixteen byte rows in the table rather than twenty.
        Box::new(Kapi {
            id: TitleId::T9,
            label: "black ops cold war",
            status: Support::Partial,
            marker: "BlackOpsColdWar.exe",
            roots: &["zone"],
            version: 16,
        }),
        Box::new(Kapi {
            id: TitleId::Iw8,
            label: "modern warfare 2019",
            status: Support::Partial,
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
            status: Support::Partial,
            marker: "bo7",
            roots: &["zone"],
            version: 26,
        }),
        Box::new(Casc {
            id: TitleId::H2,
            label: "modern warfare 2 campaign remastered",
            status: Support::Partial,
            product: "lazr",
            kind: CascKind::Pak,
        }),
        // Black Ops 4 is Black Ops III's engine on Battle.net: the same sab
        // banks under `zone\snd\<language>\`, inside casc rather than on disk.
        Box::new(Casc {
            id: TitleId::T8,
            label: "black ops 4",
            status: Support::Expected,
            product: "b04",
            kind: CascKind::Sab,
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
    pub status: Support,
    /// The product code Battle.net knows this game by, as `.build.info` spells
    /// it: `lazr` is modern warfare 2 campaign remastered, `b04` black ops 4.
    pub product: &'static str,
    pub kind: CascKind,
}

/// What the blobs in the storage are.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CascKind {
    /// `soundfile*.pak` streams, carved by their flac magic.
    Pak,
    /// Sab banks under `zone/snd/`, with tables of their own.
    Sab,
}

const CASC_ROOTS: &[&str] = &["Data"];
const CASC_CONTAINERS: &[&str] = &["idx", "index"];
const CASC_SAB_ROOTS: &[&str] = &["zone/snd"];
const CASC_SAB_CONTAINERS: &[&str] = &["sabs", "sabl"];

/// Does `.build.info` name this product? The product column is blank in some
/// installs, but the cdn path is always `tpr/<product>`.
fn casc_product(info: &str, product: &str) -> bool {
    let lower = info.to_ascii_lowercase();
    lower.contains(&format!("tpr/{product}")) || lower.contains(&format!("|{product}|"))
}

impl Title for Casc {
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
        match self.kind {
            CascKind::Pak => CASC_ROOTS,
            CascKind::Sab => CASC_SAB_ROOTS,
        }
    }

    fn containers(&self) -> &'static [&'static str] {
        match self.kind {
            CascKind::Pak => CASC_CONTAINERS,
            CascKind::Sab => CASC_SAB_CONTAINERS,
        }
    }

    fn fingerprint(&self, root: &Path) -> Option<Fingerprint> {
        let info = std::fs::read_to_string(root.join(".build.info")).ok()?;
        if !casc_product(&info, self.product) {
            return None;
        }
        let lower = info.to_ascii_lowercase();
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
        let store = match self.kind {
            // The sound paks inside are the same streams ghosts and advanced
            // warfare keep on disk, under the same name.
            CascKind::Pak => {
                let set = crate::hm::pack::store::CascSet::open(storage, &["soundfile"]);
                if set.info.is_empty() {
                    return Err(anyhow!("no sound paks in this casc storage"));
                }
                Store::Casc(set)
            }
            // The banks are read the way black ops iii's are, tables first:
            // the header out of the front block and the tables out of the
            // last ones, with the audio between left in the storage until a
            // sound is played.
            CascKind::Sab => {
                let banks: Vec<crate::hm::pack::casc::tvfs::File> = storage
                    .with_extension(CASC_SAB_CONTAINERS)
                    .into_iter()
                    .filter(|file| CASC_SAB_ROOTS.iter().any(|root| file.path.starts_with(root)))
                    .cloned()
                    .collect();
                if banks.is_empty() {
                    return Err(anyhow!("no sab banks in this casc storage"));
                }
                let mut set = crate::hm::pack::store::SabSet::default();
                let total = banks.len();
                for (i, file) in banks.iter().enumerate() {
                    let what = file
                        .path
                        .rsplit('/')
                        .next()
                        .and_then(|name| name.rsplit_once('.'))
                        .map(|(stem, _)| stem.to_string())
                        .unwrap_or_else(|| "mounting".into());
                    report.step(i, total, &what);
                    // A bank with nothing in it, or one harmony cannot read,
                    // is simply not a package.
                    let _ = set.mount_casc(storage.clone(), file);
                }
                if set.info.is_empty() {
                    return Err(anyhow!("no readable sab banks in this casc storage"));
                }
                Store::Sab(set)
            }
        };
        report.step(1, 1, "mounted");
        Ok(Mount {
            title: self.id,
            root: root.to_path_buf(),
            store,
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
        // Only the title whose roots include it claims a `cod26\` install.
        if root.join("cod26").is_dir() && !self.roots.contains(&"cod26") {
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
