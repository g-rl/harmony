//! The titles from before the kapi era.
//!
//! World at War and Modern Warfare 2 keep named wavs in `.iwd` zips, Black Ops
//! II keeps banks in `sound\*.sabs`, and Ghosts and Advanced Warfare stream
//! from `zone\**\soundfile*.pak`. One description covers all of them: which
//! folders to look in, which files count, and which container reader to use.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

use crate::hm::game::{Fingerprint, Mount, Progress, Support, Title, TitleId};
use crate::hm::pack::store::{BothSet, FfSet, IwdSet, PakSet, SabSet, Store};
use crate::hm::zone::ZoneSet;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Iwd,
    Sab,
    Pak,
    /// Fastfiles, which is where modern warfare 3 keeps its audio.
    Ff,
    /// Archives and fastfiles together, which is what modern warfare 2 needs:
    /// the streamed audio is in the archives, the loaded audio is in the
    /// zones, and neither is a copy of the other.
    Both,
}

pub struct Legacy {
    pub id: TitleId,
    pub label: &'static str,
    pub status: Support,
    pub kind: Kind,
    /// Folders under the install root that are searched, two levels deep.
    pub roots: &'static [&'static str],
    /// Extensions that count as a container for this title.
    pub exts: &'static [&'static str],
    /// A container's file name has to contain one of these, when given.
    pub stems: &'static [&'static str],
    /// Something in the install root whose name proves which game this is.
    pub markers: &'static [&'static str],
    /// A marker that rules the title out: another game's binaries.
    pub not: &'static [&'static str],
    /// Whether a marker is required rather than merely convincing.
    ///
    /// A title whose root folder is the install root itself would otherwise
    /// match every other game's install, because one level down is where the
    /// other games keep their `zone\` folder.
    pub strict: bool,
}

/// The install root itself, written as a root folder.
///
/// A title whose audio sits beside the executable rather than in a folder of
/// its own says so with this, and the window prints it as `install folder`.
pub const HERE: &str = ".";

/// Files under `folder`, two levels deep, that match the extensions and stems.
fn containers_in(root: &Path, title: &Legacy) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for folder in title.roots {
        let base = root.join(folder);
        let Ok(entries) = std::fs::read_dir(&base) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let Ok(inner) = std::fs::read_dir(&path) else {
                    continue;
                };
                for deeper in inner.flatten() {
                    take(&mut found, deeper.path(), title);
                }
            } else {
                take(&mut found, path, title);
            }
        }
    }
    found.sort();
    found
}

fn take(found: &mut Vec<PathBuf>, path: PathBuf, title: &Legacy) {
    let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_lowercase()) else {
        return;
    };
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if !title.exts.iter().any(|want| *want == extension) {
        return;
    }
    if !title.stems.is_empty() && !title.stems.iter().any(|stem| name.contains(stem)) {
        return;
    }
    found.push(path);
}

/// Which build of the game is installed beside these containers.
///
/// Modern Warfare 2 and Modern Warfare 3 were both re-released as sixty-four
/// bit executables years after the fact. The containers did not change, so
/// nothing in the audio says which build this is; the binaries do, in the two
/// bytes of their pe header that name the machine they were built for.
/// It has to be the game's own executable: an installer, a launcher or a
/// cleaner sitting in the same folder is nobody's build but its own.
fn bitness(root: &Path, markers: &[&str]) -> Option<&'static str> {
    let mut exes: Vec<PathBuf> = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase() == "exe")
                .unwrap_or(false)
        })
        .collect();
    exes.sort();
    let named = |path: &Path| {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        markers.iter().any(|marker| name.contains(marker))
    };
    let machine = exes
        .iter()
        .filter(|path| named(path))
        .find_map(|path| pe_machine(path))
        .or_else(|| exes.iter().find_map(|path| pe_machine(path)))?;
    Some(match machine {
        0x8664 | 0xAA64 => "64-bit",
        _ => "32-bit",
    })
}

/// The machine an executable was built for, out of its pe header.
fn pe_machine(path: &Path) -> Option<u16> {
    use std::io::Read;

    let mut head = [0u8; 0x400];
    let read = std::fs::File::open(path).ok()?.read(&mut head).ok()?;
    if read < 0x40 || &head[..2] != b"MZ" {
        return None;
    }
    let at = u32::from_le_bytes(head[0x3C..0x40].try_into().ok()?) as usize;
    if at + 6 > read || &head[at..at + 4] != b"PE\0\0" {
        return None;
    }
    Some(u16::from_le_bytes(head[at + 4..at + 6].try_into().ok()?))
}

/// Does any file in the install root carry one of these names?
fn marked(root: &Path, names: &[&str]) -> bool {
    if names.is_empty() {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        names.iter().any(|want| name.contains(want))
    })
}

impl Title for Legacy {
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
        self.exts
    }

    fn fingerprint(&self, root: &Path) -> Option<Fingerprint> {
        if marked(root, self.not) {
            return None;
        }
        let found = containers_in(root, self);
        if found.is_empty() {
            return None;
        }
        let marker = marked(root, self.markers);
        if self.strict && !marker {
            return None;
        }
        let score = if marker { 95 } else { 55 };
        Some(Fingerprint {
            title: self.id,
            score,
            roots: self.roots.iter().map(|r| root.join(r)).collect(),
            reason: format!(
                "{} {} containers{}",
                found.len(),
                self.exts.join(" / "),
                if marker { ", and this game's files" } else { "" }
            ),
            build: bitness(root, self.markers).map(|word| word.to_string()),
        })
    }

    fn mount(&self, root: &Path, report: &mut dyn Progress) -> Result<Mount> {
        let paths = containers_in(root, self);
        if paths.is_empty() {
            return Err(anyhow!(
                "no {} containers under {}",
                self.exts.join(" / "),
                self.roots.join(", ")
            ));
        }
        let total = paths.len();
        let mut store = match self.kind {
            Kind::Iwd => Store::Iwd(IwdSet::default()),
            Kind::Sab => Store::Sab(SabSet::default()),
            Kind::Pak => Store::Pak(PakSet::default()),
            Kind::Ff => Store::Ff(FfSet::default()),
            Kind::Both => Store::Both(BothSet::default()),
        };
        // A zone is a couple of hundred megabytes of zlib, and what is in one
        // does not change until the game is patched. Whatever was learned last
        // time is taken up here, and anything new is written back below.
        match &mut store {
            Store::Ff(set) => set.remember(self.id.key()),
            Store::Both(set) => set.remember(self.id.key()),
            _ => {}
        }
        for (i, path) in paths.iter().enumerate() {
            // The container's own name: the window puts this in front of the
            // person waiting, and "mounting" told them nothing.
            let what = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_else(|| "mounting".into());
            report.step(i, total, &what);
            match &mut store {
                Store::Iwd(set) => {
                    let _ = set.mount(path);
                }
                Store::Sab(set) => {
                    let _ = set.mount(path);
                }
                Store::Pak(set) => {
                    let _ = set.mount(path);
                }
                Store::Ff(set) => {
                    // A zone with no sounds in it, or a signed one harmony
                    // has no keys for, is simply not a package.
                    let _ = set.mount(path);
                }
                Store::Both(set) => {
                    // An archive with no audio in it and a zone with no sounds
                    // in it are both simply not packages.
                    let _ = set.mount(path);
                }
                Store::Kapi(_) | Store::Casc(_) => unreachable!(),
            }
        }
        match &mut store {
            Store::Ff(set) => set.keep(),
            Store::Both(set) => {
                set.seal();
                set.keep();
            }
            _ => {}
        }
        report.step(total, total, "mounted");
        Ok(Mount {
            title: self.id,
            root: root.to_path_buf(),
            store,
            zones: ZoneSet::default(),
            oodle: None,
            build: bitness(root, self.markers).map(|word| word.to_string()),
        })
    }

    fn sound_pool(&self) -> u64 {
        0
    }
}

pub fn all() -> Vec<Box<dyn Title>> {
    vec![
        Box::new(Legacy {
            id: TitleId::T4,
            label: "world at war",
            status: Support::Expected,
            kind: Kind::Iwd,
            roots: &["main", "zone"],
            exts: &["iwd"],
            stems: &[],
            markers: &["codwaw"],
            not: &["iw3", "iw4", "iw6", "t6", "blackops", "iw5"],
            strict: false,
        }),
        Box::new(Legacy {
            id: TitleId::Iw4,
            label: "modern warfare 2",
            status: Support::Verified,
            // The archives under `main\` hold the streamed audio, and the
            // fastfiles under `zone\` hold what each level loads with it:
            // roughly a third again as many sounds, and none of them a copy.
            kind: Kind::Both,
            roots: &["main", "zone"],
            exts: &["iwd", "ff"],
            stems: &[],
            markers: &["iw4"],
            not: &["codwaw", "iw3", "iw6", "t6", "blackops", "iw5"],
            strict: false,
        }),
        Box::new(Legacy {
            id: TitleId::T5,
            label: "black ops",
            status: Support::Verified,
            kind: Kind::Iwd,
            // Black ops keeps its audio where world at war and modern warfare
            // 2 keep theirs — `main\iw_NN.iwd` — but the wavs inside are not
            // riff files: a header of the game's own, then ms-adpcm. The iwd
            // reader is the same; only the sound header differs.
            roots: &["main", "zone"],
            exts: &["iwd"],
            stems: &[],
            markers: &["blackops"],
            not: &["codwaw", "iw3", "iw4", "iw6", "iw5"],
            strict: false,
        }),
        Box::new(Legacy {
            id: TitleId::T7,
            label: "black ops iii",
            status: Support::Verified,
            kind: Kind::Sab,
            // Black Ops III keeps its banks two folders down, under a folder
            // per language: `zone\snd\all` holds everything the game shares
            // and `zone\snd\en` the spoken lines, with whichever other
            // languages are installed beside them.
            roots: &["zone/snd"],
            exts: &["sabs", "sabl"],
            stems: &[],
            markers: &["blackops3", "boiii", "t7"],
            not: &[],
            strict: true,
        }),
        Box::new(Legacy {
            id: TitleId::T6,
            label: "black ops ii",
            status: Support::Expected,
            kind: Kind::Sab,
            roots: &["sound", "zone"],
            exts: &["sabs", "sabl"],
            stems: &[],
            markers: &["t6"],
            not: &[],
            strict: false,
        }),
        Box::new(Legacy {
            id: TitleId::Iw5,
            label: "modern warfare 3",
            status: Support::Partial,
            kind: Kind::Ff,
            // Nothing in modern warfare 3's `.iwd` archives is audio: every
            // sound is inside a fastfile under `zone\`, one zlib stream each.
            roots: &["zone"],
            exts: &["ff"],
            stems: &[],
            markers: &["iw5"],
            not: &["iw6", "codwaw", "blackops"],
            strict: true,
        }),
        Box::new(Legacy {
            id: TitleId::Iw7,
            label: "infinite warfare",
            status: Support::Verified,
            kind: Kind::Sab,
            // Infinite Warfare keeps its banks beside the executable, one
            // `.sabl` and one `.sabs` per zone, and a copy of the spoken ones
            // under a folder per installed language: `cp_final.sabl` at the
            // root, `english\eng_cp_final.sabl` beside it. Searching the root
            // two levels deep takes in whichever languages are installed.
            roots: &[HERE],
            exts: &["sabs", "sabl"],
            stems: &[],
            markers: &["iw7"],
            not: &["t6", "iw6", "s1_"],
            strict: true,
        }),
        Box::new(Legacy {
            id: TitleId::Iw3,
            label: "call of duty 4",
            status: Support::Verified,
            kind: Kind::Both,
            // Call of Duty 4 keeps named wavs and mp3s in `main\iw_NN.iwd`,
            // the same zips world at war and modern warfare 2 use. Nothing
            // about the archive says which game it belongs to, so the title's
            // own binaries have to be there.
            roots: &["main", "zone"],
            exts: &["iwd", "ff"],
            stems: &[],
            markers: &["iw3"],
            not: &["iw4", "iw5", "iw6", "codwaw", "blackops", "t6"],
            strict: true,
        }),
        Box::new(Legacy {
            id: TitleId::Iw6,
            label: "ghosts",
            status: Support::Partial,
            kind: Kind::Pak,
            roots: &["zone"],
            exts: &["pak"],
            stems: &["soundfile"],
            markers: &["iw6"],
            not: &[],
            strict: false,
        }),
        Box::new(Legacy {
            id: TitleId::H1,
            label: "modern warfare remastered",
            status: Support::Partial,
            kind: Kind::Pak,
            // Remastered keeps its paks beside the executable, and a copy of
            // the voice ones under a folder per installed language:
            // `soundfile12.pak` at the root, `english\eng_soundfile12.pak`
            // beside it. Searching the root two levels deep picks up every
            // language that is installed without harmony having to know their
            // names — french, german, whatever the account bought.
            roots: &[HERE],
            exts: &["pak"],
            stems: &["soundfile"],
            markers: &["h1_", "h1-mod", "modernwarfareremastered", "mwr_"],
            not: &["iw6", "codwaw"],
            strict: true,
        }),
        Box::new(Legacy {
            id: TitleId::S1,
            label: "advanced warfare",
            status: Support::Partial,
            kind: Kind::Pak,
            roots: &["zone"],
            exts: &["pak"],
            stems: &["soundfile"],
            markers: &["s1_", "s1x", "s1mp", "s1sp"],
            not: &["iw6"],
            strict: false,
        }),
    ]
}
