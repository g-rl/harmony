use std::path::PathBuf;

use crate::hm::catalog::{Entry, Name};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layout {
    CategoryPackage,
    LanguageCategory,
    Package,
    Flat,
}

pub const ALL: &[Layout] = &[
    Layout::CategoryPackage,
    Layout::LanguageCategory,
    Layout::Package,
    Layout::Flat,
];

impl Layout {
    pub fn label(self) -> &'static str {
        match self {
            Layout::CategoryPackage => "category/package",
            Layout::LanguageCategory => "language/category",
            Layout::Package => "package",
            Layout::Flat => "flat",
        }
    }
}

/// A name every filesystem will take.
///
/// Spaces and square brackets are kept: they are legal everywhere harmony
/// writes, and a hashed sound is written as `zmb_tomb.all [f6a6b431ac13033b]`,
/// which is only readable with them.
pub fn safe(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' | '.' | ' ' | '[' | ']' => c,
            _ => '_',
        })
        .collect()
}

/// Drop a trailing audio extension, so `shot.wav` written as a wav does not
/// come out as `shot.wav.wav`.
pub fn strip_extension(stem: &str) -> String {
    const KNOWN: &[&str] = &[".wav", ".mp3", ".flac", ".ogg", ".opus", ".raw"];
    let lower = stem.to_ascii_lowercase();
    for end in KNOWN {
        if lower.ends_with(end) {
            return stem[..stem.len() - end.len()].to_string();
        }
    }
    stem.to_string()
}

/// What a sound is called on disk.
///
/// A name is itself. A hash on its own is sixteen characters that say nothing
/// about where it came from, and a folder of them is unreadable, so the
/// container goes in front of it: `zmb_tomb.all [f6a6b431ac13033b]`. A sound
/// that is only a place in a pak already carries that place in its name.
pub fn stem_of(entry: &Entry, package: &str) -> String {
    match &entry.name {
        Name::Key(hash) | Name::Hash(hash) => format!("{package} [{hash:016x}]"),
        _ => entry.display(),
    }
}

pub fn path_for(
    root: &std::path::Path,
    entry: &Entry,
    package: &str,
    layout: Layout,
    preserve_paths: bool,
    normalise: bool,
    extension: &str,
) -> PathBuf {
    let display = stem_of(entry, package);
    let (folders, stem) = match display.rsplit_once('/') {
        Some((head, tail)) if preserve_paths => (head.to_string(), tail.to_string()),
        Some((_, tail)) => (String::new(), tail.to_string()),
        None => (String::new(), display.clone()),
    };
    let stem = if normalise { safe(&stem) } else { stem };
    // Names that came out of an archive already carry an extension, and the one
    // being written may not be the one they carry.
    let stem = strip_extension(&stem);

    let mut path = root.to_path_buf();
    match layout {
        Layout::CategoryPackage => {
            path.push(entry.category.label());
            path.push(safe(package));
        }
        Layout::LanguageCategory => {
            path.push(safe(entry.language.as_deref().unwrap_or("shared")));
            path.push(entry.category.label());
        }
        Layout::Package => path.push(safe(package)),
        Layout::Flat => {}
    }
    if !folders.is_empty() && layout != Layout::Flat {
        for part in folders.split('/') {
            if !part.is_empty() {
                path.push(safe(part));
            }
        }
    }
    path.push(format!("{stem}.{extension}"));
    path
}
