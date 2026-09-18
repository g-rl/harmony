pub mod category;
pub mod group;
pub mod hash;
pub mod mode;
pub mod names;

use std::collections::BTreeMap;

use crate::hm::catalog::category::Category;
use crate::hm::game::TitleId;
use crate::hm::pack::PackageId;
use crate::hm::sound::Codec;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SoundId(pub u32);

#[derive(Clone, Debug)]
pub enum Name {
    Resolved(String),
    Hash(u64),
    Key(u64),
    /// Where a sound sits, for containers that carry no id at all.
    ///
    /// The sound paks are one stream after another with nothing between them,
    /// so there is no hash to print: the key harmony files them under is one
    /// it worked out itself. Printing that as `_9f3c...` reads like a name the
    /// game gave it, which it is not. `soundfile12#00042` is the truth.
    Slot(String),
}

impl Name {
    pub fn text(&self) -> String {
        match self {
            Name::Resolved(name) => name.clone(),
            Name::Hash(hash) => format!("_{hash:016x}"),
            Name::Key(key) => format!("_{key:016x}"),
            Name::Slot(where_it_is) => where_it_is.clone(),
        }
    }

    pub fn resolved(&self) -> bool {
        matches!(self, Name::Resolved(_))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Stream { key: u64 },
    Bank { package: PackageId, index: u32 },
}

#[derive(Clone, Debug, Default)]
pub struct Facets {
    pub weapon: Option<String>,
    pub character: Option<String>,
    pub map: Option<String>,
    pub kind: Option<String>,
}

/// The bucket a name falls in, or `unnamed` when there is no name to read.
pub fn sub_for(name: &Name) -> &'static str {
    match name.resolved() {
        true => category::sub_of(&name.text()),
        false => "unnamed",
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub id: SoundId,
    pub name: Name,
    /// The name folded to lowercase once, when the entry is made.
    ///
    /// Search reads it on every entry for every term of every keystroke; at
    /// fifty thousand sounds, folding it there instead cost more than the
    /// search itself.
    pub lower: String,
    pub source: Source,
    pub package: PackageId,
    pub index: u32,
    pub codec: Codec,
    pub rate: u32,
    pub channels: u8,
    pub frames: u64,
    pub bytes: u64,
    pub language: Option<String>,
    pub category: Category,
    /// The finer bucket, worked out once when the name is set.
    ///
    /// A sound with no name has none: `_f6a6b431ac13033b` contains `ac130`
    /// and would otherwise file itself under aircraft. Nothing is read out of
    /// a hash.
    pub sub: &'static str,
    pub facets: Facets,
    pub favorite: bool,
    pub tags: Vec<String>,
}

impl Entry {
    /// Give this sound a name, keeping the folded copy search reads in step
    /// and the bucket with it.
    pub fn rename(&mut self, name: Name) {
        self.lower = name.text().to_ascii_lowercase();
        self.sub = sub_for(&name);
        self.name = name;
    }

    /// What search matches against: the name, folded, without allocating.
    pub fn text(&self) -> &str {
        &self.lower
    }

    /// The number harmony knows this sound by, across scans and between runs:
    /// what favorites, tags and collections are kept under.
    ///
    /// A stream has one already. A bank entry does not, so its name is hashed
    /// where it has one — names outlive a repack, positions do not — and its
    /// place in the bank is used where it has none.
    pub fn key(&self) -> u64 {
        match self.source {
            Source::Stream { key } => key,
            Source::Bank { package, index } => match &self.name {
                Name::Resolved(name) => hash::fnv1a64(name),
                // The id the bank itself carries, so the cache keeps it and a
                // name list can still be matched against it after a reload.
                Name::Key(key) => *key,
                _ => hash::fnv1a64(&format!("bank {}:{index}", package.0)),
            },
        }
    }

    pub fn seconds(&self) -> f32 {
        if self.rate == 0 {
            0.0
        } else {
            self.frames as f32 / self.rate as f32
        }
    }

    pub fn display(&self) -> String {
        self.name.text()
    }
}

#[derive(Default)]
pub struct Catalog {
    pub title: Option<TitleId>,
    pub entries: Vec<Entry>,
    pub scanned_at: Option<String>,
}

impl Catalog {
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn push(&mut self, entry: Entry) {
        self.entries.push(entry);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn by_category(&self) -> BTreeMap<Category, usize> {
        let mut counts = BTreeMap::new();
        for entry in &self.entries {
            *counts.entry(entry.category).or_insert(0) += 1;
        }
        counts
    }

    pub fn by_language(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for entry in &self.entries {
            let key = entry.language.clone().unwrap_or_else(|| "shared".into());
            *counts.entry(key).or_insert(0) += 1;
        }
        counts
    }

    pub fn named(&self) -> usize {
        self.entries.iter().filter(|e| e.name.resolved()).count()
    }

    pub fn total_bytes(&self) -> u64 {
        self.entries.iter().map(|e| e.bytes).sum()
    }
}

pub fn language_of(package: &str) -> Option<String> {
    const CODES: &[(&str, &str)] = &[
        ("eng_", "english"),
        ("fra_", "french"),
        ("ger_", "german"),
        ("ita_", "italian"),
        ("spa_", "spanish"),
        ("rus_", "russian"),
        ("pol_", "polish"),
        ("jpn_", "japanese"),
        ("kor_", "korean"),
        ("kof_", "korean"),
        ("kop_", "portuguese"),
        ("chi_", "chinese"),
        ("ww_", "worldwide"),
        // Black ops 4 and cold war spell them in two letters.
        ("en_", "english"),
        ("fr_", "french"),
        ("fj_", "french"),
        ("ge_", "german"),
        ("it_", "italian"),
        ("es_", "spanish"),
        ("ea_", "spanish"),
        ("ru_", "russian"),
        ("po_", "polish"),
        ("bp_", "portuguese"),
        ("ko_", "korean"),
        ("ms_", "chinese"),
    ];
    let lower = package.to_ascii_lowercase();
    CODES
        .iter()
        .find(|(prefix, _)| lower.starts_with(prefix))
        .map(|(_, name)| (*name).to_string())
}
