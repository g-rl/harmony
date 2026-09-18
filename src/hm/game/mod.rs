pub mod jup;
pub mod legacy;
pub mod titles;

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::hm::pack::oodle::Oodle;
use crate::hm::pack::store::Store;
use crate::hm::zone::ZoneSet;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TitleId {
    Jup,
    Iw9,
    Iw8,
    T10,
    T11,
    T7,
    T6,
    T5,
    Iw7,
    Iw5,
    S1,
    H1,
    H2,
    Iw6,
    Iw4,
    Iw3,
    T4,
    Unknown,
}

impl TitleId {
    /// The engine's own short name for the title, which is what harmony keys
    /// its caches and settings on.
    pub fn key(self) -> &'static str {
        match self {
            TitleId::Jup => "jup",
            TitleId::Iw9 => "iw9",
            TitleId::Iw8 => "iw8",
            TitleId::T10 => "t10",
            TitleId::T11 => "t11",
            TitleId::T7 => "t7",
            TitleId::T6 => "t6",
            TitleId::T5 => "t5",
            TitleId::Iw7 => "iw7",
            TitleId::Iw5 => "iw5",
            TitleId::S1 => "s1",
            TitleId::H1 => "h1",
            TitleId::H2 => "h2",
            TitleId::Iw6 => "iw6",
            TitleId::Iw4 => "iw4",
            TitleId::Iw3 => "iw3",
            TitleId::T4 => "t4",
            TitleId::Unknown => "unknown",
        }
    }

    /// What players call the game, short enough for a tab.
    pub fn abbr(self) -> &'static str {
        match self {
            TitleId::Jup => "mwiii",
            TitleId::Iw9 => "mwii",
            TitleId::Iw8 => "mw19",
            TitleId::T10 => "bo6",
            TitleId::T11 => "bo7",
            TitleId::T7 => "bo3",
            TitleId::T6 => "bo2",
            TitleId::T5 => "bo1",
            TitleId::Iw7 => "iw",
            TitleId::Iw5 => "mw3",
            TitleId::S1 => "aw",
            TitleId::H1 => "mwr",
            TitleId::H2 => "mw2cr",
            TitleId::Iw6 => "ghosts",
            TitleId::Iw4 => "mw2",
            TitleId::Iw3 => "cod4",
            TitleId::T4 => "waw",
            TitleId::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Support {
    Verified,
    Expected,
    Partial,
    DetectOnly,
    None,
}

impl Support {
    pub fn label(self) -> &'static str {
        match self {
            Support::Verified => "verified",
            Support::Expected => "expected",
            Support::Partial => "partial",
            Support::DetectOnly => "detect only",
            Support::None => "none",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Fingerprint {
    pub title: TitleId,
    pub score: u8,
    pub roots: Vec<PathBuf>,
    pub reason: String,
    pub build: Option<String>,
}

pub struct Mount {
    pub title: TitleId,
    pub root: PathBuf,
    pub store: Store,
    pub zones: ZoneSet,
    pub oodle: Option<Oodle>,
    pub build: Option<String>,
}

pub trait Progress {
    fn step(&mut self, done: usize, total: usize, what: &str);
}

impl<F: FnMut(usize, usize, &str)> Progress for F {
    fn step(&mut self, done: usize, total: usize, what: &str) {
        self(done, total, what)
    }
}

pub trait Title: Send + Sync {
    fn id(&self) -> TitleId;
    fn label(&self) -> &'static str;
    fn status(&self) -> Support;
    fn sound_roots(&self) -> &'static [&'static str];
    fn containers(&self) -> &'static [&'static str];
    fn fingerprint(&self, root: &Path) -> Option<Fingerprint>;
    fn mount(&self, root: &Path, report: &mut dyn Progress) -> Result<Mount>;
    fn sound_pool(&self) -> u64;
    /// What the tab says.
    fn abbr(&self) -> &'static str {
        self.id().abbr()
    }
}

pub fn registry() -> Vec<Box<dyn Title>> {
    titles::all()
}

pub fn detect(root: &Path) -> Vec<Fingerprint> {
    let mut found: Vec<Fingerprint> = registry()
        .iter()
        .filter_map(|title| title.fingerprint(root))
        .filter(|print| print.score > 0)
        .collect();
    found.sort_by_key(|print| std::cmp::Reverse(print.score));
    found
}

pub fn title_for(id: TitleId) -> Option<Box<dyn Title>> {
    registry().into_iter().find(|title| title.id() == id)
}
