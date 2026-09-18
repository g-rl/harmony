//! Which part of the game a sound belongs to.
//!
//! Every Call of Duty keeps its modes apart in the names: `mp_` maps, `zmb_`
//! banks, `nazi_zombi_` levels. Nothing declares it, so it is read off the path
//! and the package, one token at a time — a substring test would put every
//! `amp` in multiplayer and every `so_far` in spec ops.

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Mode {
    Campaign,
    Multiplayer,
    Zombies,
    SpecOps,
    Warzone,
    Shared,
}

pub const ALL: &[Mode] = &[
    Mode::Campaign,
    Mode::Multiplayer,
    Mode::Zombies,
    Mode::SpecOps,
    Mode::Warzone,
    Mode::Shared,
];

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Campaign => "campaign",
            Mode::Multiplayer => "multiplayer",
            Mode::Zombies => "zombies",
            Mode::SpecOps => "spec ops",
            Mode::Warzone => "warzone",
            Mode::Shared => "shared",
        }
    }
}

/// Tokens that mean a mode outright, wherever they appear as a whole word.
const TOKENS: &[(Mode, &[&str])] = &[
    (
        Mode::Zombies,
        &["zm", "zmb", "zom", "zombie", "zombies", "zombi", "zmbs"],
    ),
    (Mode::Warzone, &["wz", "warzone", "br", "battleroyale"]),
    (
        Mode::SpecOps,
        &["so", "specops", "spec", "coop", "firefight", "survival"],
    ),
    (Mode::Multiplayer, &["mp", "mpl", "multiplayer", "mps", "pvp"]),
    (Mode::Campaign, &["sp", "spl", "campaign", "cp", "single"]),
];

/// Longer marks that are read as substrings, because they are distinctive
/// enough that no other word contains them.
const MARKS: &[(Mode, &[&str])] = &[
    (Mode::Zombies, &["nazi_zombi", "zombie", "zombi", "zmb_"]),
    (Mode::Warzone, &["warzone"]),
    (Mode::SpecOps, &["specops", "spec_ops", "firefight"]),
    (Mode::Multiplayer, &["multiplayer"]),
    (Mode::Campaign, &["campaign"]),
];

fn tokens(text: &str) -> Vec<&str> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect()
}

/// The mode a single string points at, if any.
pub fn read(text: &str) -> Option<Mode> {
    let lower = text.to_ascii_lowercase();
    for (mode, marks) in MARKS {
        if marks.iter().any(|mark| lower.contains(mark)) {
            return Some(*mode);
        }
    }
    let parts = tokens(&lower);
    for (mode, wanted) in TOKENS {
        if parts.iter().any(|part| wanted.contains(part)) {
            return Some(*mode);
        }
    }
    None
}

/// What a sound belongs to, going by its own name first and the package it
/// came out of second. A bank named for a mode says so for everything in it;
/// a name that says otherwise is the better answer for that one sound.
pub fn of(name: &str, package: &str) -> Mode {
    read(name)
        .or_else(|| read(package))
        .unwrap_or(Mode::Shared)
}

pub fn parse(label: &str) -> Option<Mode> {
    let lower = label.to_ascii_lowercase();
    ALL.iter()
        .copied()
        .find(|mode| mode.label() == lower)
        .or_else(|| read(&lower))
}
