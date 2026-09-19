use std::path::PathBuf;

use crate::hm::catalog::{Entry, Name};

/// How deep a folder tree an extraction builds.
///
/// One ladder, shallowest first: every rung adds a level above the one before
/// it. `Flat` writes the file and nothing else, which is what is wanted when a
/// handful of sounds are being pulled out to be listened to; `SoundPath` keeps
/// the folders the sound's own name carries, so `zombies/foley/gestures` comes
/// out the way the game spells it; the three above that put the container, the
/// category or the language in front of it, for the runs that take thousands of
/// sounds at once and need somewhere to put them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layout {
    Flat,
    SoundPath,
    Package,
    CategoryPackage,
    LanguageCategory,
}

pub const ALL: &[Layout] = &[
    Layout::Flat,
    Layout::SoundPath,
    Layout::Package,
    Layout::CategoryPackage,
    Layout::LanguageCategory,
];

impl Layout {
    pub fn label(self) -> &'static str {
        match self {
            Layout::Flat => "file only",
            Layout::SoundPath => "sound path",
            Layout::Package => "package",
            Layout::CategoryPackage => "category/package",
            Layout::LanguageCategory => "language/category",
        }
    }

    /// What this rung says in the one line under the chips.
    pub fn about(self) -> &'static str {
        match self {
            Layout::Flat => "straight into the export folder",
            Layout::SoundPath => "the folders the sound's own name carries",
            Layout::Package => "the container it came out of, then its own folders",
            Layout::CategoryPackage => "what kind of sound it is, then its container",
            Layout::LanguageCategory => "the language it is spoken in, then its kind",
        }
    }

    /// Does this rung keep the folders inside the sound's own name?
    ///
    /// Every one but the flat one does. A sound called
    /// `zombies/foley/gestures/vm_gesture_walkie` is a path the game itself
    /// wrote, and throwing it away puts thousands of files in one folder.
    pub fn keeps_paths(self) -> bool {
        self != Layout::Flat
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
    normalise: bool,
    extension: &str,
) -> PathBuf {
    let display = stem_of(entry, package);
    let (folders, stem) = match display.rsplit_once('/') {
        Some((head, tail)) if layout.keeps_paths() => (head.to_string(), tail.to_string()),
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
        Layout::SoundPath | Layout::Flat => {}
    }
    for part in folders.split('/') {
        if !part.is_empty() {
            path.push(safe(part));
        }
    }
    path.push(format!("{stem}.{extension}"));
    path
}

/// The path one sound would be written to, relative to the export folder, for
/// the line under the chips. Nothing is written; this is only what it would
/// look like.
pub fn preview(entry: &Entry, package: &str, layout: Layout, normalise: bool, extension: &str) -> String {
    let path = path_for(std::path::Path::new(""), entry, package, layout, normalise, extension);
    path.components()
        .map(|part| part.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hm::catalog::category::Category;
    use crate::hm::catalog::{Entry, Facets, Name, SoundId, Source};
    use crate::hm::pack::PackageId;
    use crate::hm::sound::Codec;

    fn sound(name: &str) -> Entry {
        Entry {
            id: SoundId(0),
            name: Name::Resolved(name.into()),
            lower: name.to_ascii_lowercase(),
            source: Source::Bank {
                package: PackageId(0),
                index: 0,
            },
            package: PackageId(0),
            index: 0,
            codec: Codec::Flac,
            rate: 48000,
            channels: 1,
            frames: 4800,
            bytes: 1024,
            language: Some("english".into()),
            category: Category::Foley,
            sub: "",
            facets: Facets::default(),
            favorite: false,
            tags: Vec::new(),
        }
    }

    /// Every rung is the one below it with another level on top, and the flat
    /// one is the file by itself: the whole point of the ladder.
    #[test]
    fn the_rungs_go_one_deeper_each() {
        let entry = sound("zombies/foley/gestures/vm_gesture_walkie");
        let shape = |layout| preview(&entry, "common_cp.sabl", layout, true, "wav");
        assert_eq!(shape(Layout::Flat), "vm_gesture_walkie.wav");
        assert_eq!(
            shape(Layout::SoundPath),
            "zombies/foley/gestures/vm_gesture_walkie.wav"
        );
        assert_eq!(
            shape(Layout::Package),
            "common_cp.sabl/zombies/foley/gestures/vm_gesture_walkie.wav"
        );
        assert_eq!(
            shape(Layout::CategoryPackage),
            "foley/common_cp.sabl/zombies/foley/gestures/vm_gesture_walkie.wav"
        );
        assert_eq!(
            shape(Layout::LanguageCategory),
            "english/foley/zombies/foley/gestures/vm_gesture_walkie.wav"
        );
    }

    /// A sound with no name of its own still lands under its key, and the flat
    /// rung still puts it straight in the export folder.
    #[test]
    fn a_key_is_written_under_its_package() {
        let mut entry = sound("unused");
        entry.name = Name::Key(0xf6a6b431ac13033b);
        assert_eq!(
            preview(&entry, "zmb_tomb.all", Layout::Flat, true, "wav"),
            "zmb_tomb.all [f6a6b431ac13033b].wav"
        );
    }
}
