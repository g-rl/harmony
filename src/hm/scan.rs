use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use crate::hm::catalog::category::Category;
use crate::hm::catalog::{Entry, Facets, Name, SoundId, Source, category, language_of};
use crate::hm::game::{Mount, Title, TitleId};
use crate::hm::pack::PackageId;
use crate::hm::pack::kapi::Package;
use crate::hm::pack::oodle::Oodle;
use crate::hm::pack::store::Store;
use crate::hm::sound::{Codec, opus};

/// How much of a blob the scanner reads before deciding what it is: enough for
/// a header, a seek table of a few hundred packets and the first packets.
pub const HEAD: usize = 0x800;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Depth {
    /// Only the packages a title keeps its localised audio in. That is the
    /// dialogue and nothing else, which is why it finishes in seconds.
    Quick,
    /// Every package, but each is sampled first and dropped when the sample
    /// turns up no audio at all.
    Deep,
    /// Every entry of every package, however long that takes.
    Full,
}

pub const DEPTHS: &[Depth] = &[Depth::Quick, Depth::Deep, Depth::Full];

/// The depth a cache file says it was written at.
pub fn depth_of(label: &str) -> Depth {
    match label {
        "full" => Depth::Full,
        "deep" => Depth::Deep,
        _ => Depth::Quick,
    }
}

/// How many entries of a package the deep pass looks at before deciding
/// whether the package is worth reading whole.
const SAMPLE: usize = 400;

impl Depth {
    pub fn label(self) -> &'static str {
        match self {
            Depth::Quick => "quick",
            Depth::Deep => "deep",
            Depth::Full => "full",
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            Depth::Quick => "voice packages only, seconds",
            Depth::Deep => "every package, all categories, minutes",
            Depth::Full => "every entry, longest",
        }
    }
}

pub enum Msg {
    Progress {
        done: usize,
        total: usize,
        what: String,
    },
    Found(Vec<Entry>),
    Ready(Box<Mount>),
    Failed(String),
    /// The scan was written to its cache file, off the ui thread, by the
    /// relay that saw every entry go past.
    Cached { sounds: usize },
    /// A refresh compared the containers on disk with the ones the cached scan
    /// was read from.
    Checked { changed: usize, total: usize },
    /// There was not enough room to write it. The catalogue is still good.
    NoRoom(crate::hm::storage::space::Room),
    CacheFailed(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// A whole scan: mount, walk, catalogue, cache.
    Scan,
    /// Only opening the containers again, behind a catalogue that came back
    /// from a cache.
    Mount,
}

pub struct Job {
    /// Which game this job is for. Several can run at once, one per game, and
    /// only the one on screen has its sounds kept.
    pub title: TitleId,
    pub kind: Kind,
    pub depth: Depth,
    pub rx: Receiver<Msg>,
    pub cancel: Arc<AtomicBool>,
    pub done: Arc<AtomicUsize>,
    pub total: Arc<AtomicUsize>,
}

impl Job {
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// The scan's own row for the cache file, built as the entry goes past on its
/// way to the window rather than from the catalogue afterwards.
fn cached_of(entry: &Entry) -> crate::hm::storage::CachedSound {
    crate::hm::storage::CachedSound {
        key: entry.key(),
        package: entry.package.0,
        index: entry.index,
        channels: entry.channels,
        frames: entry.frames,
        bytes: entry.bytes,
        language: entry.language.clone(),
        category: entry.category.label().to_string(),
        name: match &entry.name {
            Name::Resolved(name) => Some(name.clone()),
            _ => None,
        },
        rate: entry.rate,
        codec: entry.codec.label().to_string(),
        bank: matches!(entry.source, Source::Bank { .. }),
        slot: matches!(entry.name, Name::Slot(_)),
    }
}

/// Everything the scan produced passes through here on its way to the window.
///
/// The window used to build the cache itself, at the end, out of a finished
/// catalogue: fifty thousand rows of json on the ui thread, which is exactly
/// the freeze it looked like. The relay sees the same entries stream past, so
/// it can keep the rows as it goes and write the file when the walk is done,
/// on this thread, while the window carries on drawing.
fn relay(
    rx: Receiver<Msg>,
    tx: Sender<Msg>,
    game: String,
    depth: Depth,
    cancel: Arc<AtomicBool>,
) {
    use crate::hm::storage;

    let mut rows: Vec<storage::CachedSound> = Vec::new();
    let mut packages: Vec<String> = Vec::new();
    let mut files: Vec<storage::CachedFile> = Vec::new();
    let mut keys = 0usize;
    let mut failed = false;

    while let Ok(message) = rx.recv() {
        match &message {
            Msg::Found(batch) => rows.extend(batch.iter().map(cached_of)),
            Msg::Ready(mount) => {
                packages = mount.store.info().iter().map(|i| i.name.clone()).collect();
                files = mount
                    .store
                    .info()
                    .iter()
                    .filter_map(|info| storage::stamp_of(&info.path))
                    .collect();
                keys = mount.store.keys();
            }
            Msg::Failed(_) => failed = true,
            _ => {}
        }
        if tx.send(message).is_err() {
            // The window has gone: nothing left to cache for.
            return;
        }
    }

    // A cancelled or failed scan is a partial one, and a partial catalogue
    // written as if it were whole is worse than no cache at all.
    if failed || cancel.load(Ordering::Relaxed) || rows.is_empty() {
        return;
    }

    let want = storage::space::cache_size(rows.len());
    if let Some(room) = storage::space::shortfall(&storage::cache_dir(), want) {
        let _ = tx.send(Msg::NoRoom(room));
        return;
    }
    let sounds = rows.len();
    match storage::save_cache(&storage::Cached {
        game,
        depth: depth.label().to_string(),
        at: storage::stamp(),
        packages,
        keys,
        sounds: rows,
        files,
    }) {
        Ok(_) => {
            let _ = tx.send(Msg::Cached { sounds });
        }
        Err(error) => {
            let _ = tx.send(Msg::CacheFailed(error));
        }
    }
}

/// Cheap test on the first bytes of a blob, so the scanner can skip everything
/// that is not audio without decompressing it whole.
///
/// Packed sounds carry a 32 byte header: eight zero bytes, then `-4` as an
/// `i64`, then the size. Stored sounds have no header at all and open straight
/// into their seek table: a zero word followed by rising byte offsets.
pub fn looks_like_sound(head: &[u8]) -> bool {
    packed_head(head) || stored_head(head)
}

fn packed_head(head: &[u8]) -> bool {
    head.len() >= 16
        && head[0..8].iter().all(|b| *b == 0)
        && head[8..16] == [0xFC, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]
}

fn stored_head(head: &[u8]) -> bool {
    const WANTED: usize = 6;
    if head.len() < (WANTED + 1) * 4 {
        return false;
    }
    let word = |i: usize| u32::from_le_bytes(head[i * 4..i * 4 + 4].try_into().unwrap());
    if word(0) != 0 || word(1) == 0 {
        return false;
    }
    // A seek table rises, and its steps are packet sizes: never huge.
    (1..=WANTED).all(|i| {
        let step = word(i).wrapping_sub(word(i - 1));
        (2..=8192).contains(&step)
    })
}

fn audio_package(name: &str) -> bool {
    language_of(name).is_some()
}

pub fn start(title: Box<dyn Title>, root: PathBuf, depth: Depth) -> Job {
    let (tx, rx) = channel();
    let (raw_tx, raw_rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicUsize::new(0));
    let total = Arc::new(AtomicUsize::new(0));
    let id = title.id();

    let job = Job {
        title: id,
        kind: Kind::Scan,
        depth,
        rx,
        cancel: cancel.clone(),
        done: done.clone(),
        total: total.clone(),
    };

    // Everything the walk produces goes to the relay, which forwards it to the
    // window and keeps a copy for the cache file.
    let game = id.key().to_string();
    let watching = cancel.clone();
    std::thread::spawn(move || relay(raw_rx, tx, game, depth, watching));

    std::thread::spawn(move || {
        let tx = raw_tx;
        let mut report = |at: usize, of: usize, what: &str| {
            let _ = tx.send(Msg::Progress {
                done: at,
                total: of,
                what: what.to_string(),
            });
        };
        let mut mount = match title.mount(&root, &mut report) {
            Ok(mount) => mount,
            Err(error) => {
                let _ = tx.send(Msg::Failed(error.to_string()));
                return;
            }
        };

        match &mount.store {
            // Kapi entries say nothing about themselves, so every one of them
            // has to be read far enough to be recognised.
            Store::Kapi(_) => walk_kapi(&mount, &root, depth, &tx, &cancel, &done, &total),
            // Every other container carries a table: the catalogue is a read of
            // those tables, and takes about as long as opening the files did.
            _ => walk_tables(&mut mount, &tx, &cancel, &done, &total),
        }

        let _ = tx.send(Msg::Ready(Box::new(mount)));
    });

    job
}

/// What a sound is, as far as anything known about it says.
///
/// A name settles it outright. Without one there is still the package it came
/// from and the shape of the audio: the localised packages are dialogue, long
/// stereo is music, long mono is ambience, and a sound too short to be either
/// is interface or a one-shot.
pub fn classify(name: &Name, language: Option<&String>, channels: u8, seconds: f32) -> Category {
    if name.resolved() {
        let guess = category::of(&name.text());
        if guess != Category::Misc {
            return guess;
        }
    }
    if language.is_some() {
        return Category::Voice;
    }
    match (channels, seconds) {
        (2.., s) if s >= 25.0 => Category::Music,
        (2.., s) if s >= 6.0 => Category::Ambient,
        (_, s) if s >= 12.0 => Category::Ambient,
        (_, s) if s > 0.0 && s < 0.35 => Category::Ui,
        _ => Category::Misc,
    }
}

fn walk_kapi(
    mount: &Mount,
    root: &std::path::Path,
    depth: Depth,
    tx: &Sender<Msg>,
    cancel: &Arc<AtomicBool>,
    done: &Arc<AtomicUsize>,
    total: &Arc<AtomicUsize>,
) {
    let shared = match Oodle::find(root) {
        Ok(loaded) => Some(Arc::new(loaded)),
        Err(_) => None,
    };

    let targets: Vec<(PackageId, PathBuf, String, usize)> = mount
        .store
        .info()
        .iter()
        .filter(|info| info.entries > 0)
        .filter(|info| depth != Depth::Quick || audio_package(&info.name))
        .map(|info| (info.id, info.path.clone(), info.name.clone(), info.entries))
        .collect();

    let work: usize = targets.iter().map(|t| t.3).sum();
    total.store(work, Ordering::Relaxed);

    let workers = std::thread::available_parallelism()
        .map(|n| n.get().clamp(1, 8))
        .unwrap_or(4);
    let queue = Arc::new(std::sync::Mutex::new(targets));

    std::thread::scope(|scope| {
        for _ in 0..workers {
            let queue = queue.clone();
            let tx: Sender<Msg> = tx.clone();
            let cancel = cancel.clone();
            let done = done.clone();
            let oodle = shared.clone();
            scope.spawn(move || {
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    let next = { queue.lock().unwrap().pop() };
                    let Some((id, path, name, _)) = next else {
                        return;
                    };
                    let Ok(mut package) = Package::open(&path) else {
                        continue;
                    };
                    let language = language_of(&name);
                    let mut found: Vec<Entry> = Vec::new();

                    // Walk the package the way it lies on disk. The hash table
                    // is in key order, and jumping about a package this size
                    // costs far more than the reads themselves.
                    let mut order: Vec<(usize, crate::hm::pack::kapi::Entry)> =
                        package.entries.clone().into_iter().enumerate().collect();
                    order.sort_by_key(|(_, entry)| entry.offset);

                    // Most packages hold no audio whatsoever. A deep scan spends
                    // a few hundred reads finding that out instead of hours
                    // reading a package through.
                    if depth == Depth::Deep
                        && !worth_reading(&mut package, &order, oodle.as_deref())
                    {
                        done.fetch_add(order.len(), Ordering::Relaxed);
                        continue;
                    }

                    for (index, entry) in order {
                        if cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        done.fetch_add(1, Ordering::Relaxed);
                        if entry.size < 64 {
                            continue;
                        }
                        let Ok(head) = package.read_head(entry, oodle.as_deref(), HEAD) else {
                            continue;
                        };
                        if !looks_like_sound(&head) {
                            continue;
                        }
                        // The head usually carries the whole seek table, so most
                        // sounds never have to be read in full.
                        let stream = match opus::probe_head(&head) {
                            Some(stream) => stream,
                            None => {
                                let Ok(blob) = package.read(entry, oodle.as_deref()) else {
                                    continue;
                                };
                                let Some(stream) = opus::probe(&blob) else {
                                    continue;
                                };
                                stream
                            }
                        };
                        let name_text = Name::Key(entry.key);
                        let seconds = stream.frames as f32 / opus::RATE as f32;
                        found.push(Entry {
                            id: SoundId(0),
                            category: classify(
                                &name_text,
                                language.as_ref(),
                                stream.channels,
                                seconds,
                            ),
                            sub: crate::hm::catalog::sub_for(&name_text),
                            lower: name_text.text().to_ascii_lowercase(),
                            name: name_text,
                            source: Source::Stream { key: entry.key },
                            package: id,
                            index: index as u32,
                            codec: Codec::Opus,
                            rate: opus::RATE,
                            channels: stream.channels,
                            frames: stream.frames,
                            bytes: entry.size as u64,
                            language: language.clone(),
                            facets: Facets::default(),
                            favorite: false,
                            tags: Vec::new(),
                        });
                        if found.len() >= 256 {
                            let _ = tx.send(Msg::Found(std::mem::take(&mut found)));
                        }
                    }
                    if !found.is_empty() {
                        let _ = tx.send(Msg::Found(found));
                    }
                }
            });
        }
    });
}

/// The catalogue of a container that already knows what it holds: iwd archives,
/// sab banks and sound paks. Nothing is decoded here.
fn walk_tables(
    mount: &mut Mount,
    tx: &Sender<Msg>,
    cancel: &Arc<AtomicBool>,
    done: &Arc<AtomicUsize>,
    total: &Arc<AtomicUsize>,
) {
    let packages: Vec<(PackageId, String, usize)> = mount
        .store
        .info()
        .iter()
        .map(|info| (info.id, info.name.clone(), info.entries))
        .collect();
    total.store(packages.iter().map(|p| p.2).sum(), Ordering::Relaxed);

    for (id, package_name, _) in packages {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let language = language_of(&package_name);
        let listed = mount.store.listing(id);
        let mut found: Vec<Entry> = Vec::with_capacity(listed.len());
        for item in listed {
            done.fetch_add(1, Ordering::Relaxed);
            // A container that carries no ids at all has nothing to print but
            // the place the sound sits in it. The key harmony files it under is
            // one it worked out itself, and showing that as `_9f3c…` reads like
            // an id the game gave it.
            let name = match (&item.name, item.keyed) {
                (Some(text), _) => Name::Resolved(text.clone()),
                (None, true) => Name::Key(item.key),
                (None, false) => Name::Slot(format!("{package_name}#{:05}", item.index)),
            };
            let seconds = if item.rate > 0 {
                item.frames as f32 / item.rate as f32
            } else {
                0.0
            };
            // The language here is the archive the sound sits in, and in these
            // containers a localised archive holds the whole game, not just its
            // dialogue (mw2 keeps every asset in `localized_english_iwNN`). So it
            // is recorded as the language and kept out of the categorising.
            let category = classify(&name, None, item.channels, seconds);
            found.push(Entry {
                id: SoundId(0),
                category,
                sub: crate::hm::catalog::sub_for(&name),
                lower: name.text().to_ascii_lowercase(),
                name,
                source: Source::Bank {
                    package: id,
                    index: item.index,
                },
                package: id,
                index: item.index,
                codec: item.codec,
                rate: item.rate,
                channels: item.channels,
                frames: item.frames,
                bytes: item.bytes,
                language: language.clone(),
                facets: Facets::default(),
                favorite: false,
                tags: Vec::new(),
            });
            if found.len() >= 512 {
                let _ = tx.send(Msg::Found(std::mem::take(&mut found)));
            }
        }
        if !found.is_empty() {
            let _ = tx.send(Msg::Found(found));
        }
    }
}

pub fn title_key(title: TitleId) -> &'static str {
    title.key()
}

/// Look at a spread of a package's entries and say whether any of them is a
/// sound. Packages are built by asset kind, so a package with audio in it has
/// audio all through it and a sample finds it quickly.
fn worth_reading(
    package: &mut Package,
    order: &[(usize, crate::hm::pack::kapi::Entry)],
    oodle: Option<&Oodle>,
) -> bool {
    let usable: Vec<crate::hm::pack::kapi::Entry> = order
        .iter()
        .map(|(_, entry)| *entry)
        .filter(|entry| entry.size >= 64)
        .collect();
    if usable.len() <= SAMPLE {
        return true;
    }
    let step = usable.len() / SAMPLE;
    for entry in usable.iter().step_by(step.max(1)) {
        let Ok(head) = package.read_head(*entry, oodle, HEAD) else {
            continue;
        };
        if looks_like_sound(&head) {
            return true;
        }
    }
    false
}

/// Mount a title without scanning it, for when the catalogue came from the
/// cache and only the packages are missing.
/// Open the containers again and say whether any of them changed since the
/// cached scan read them.
///
/// This is what refresh does: a game that has been patched has containers of a
/// different size or date, and a game that has not is left alone rather than
/// rescanned for nothing. The mount it makes is kept either way, so the sounds
/// are playable straight afterwards.
pub fn check(title: Box<dyn Title>, root: PathBuf, expected: Vec<crate::hm::storage::CachedFile>) -> Job {
    let (tx, rx) = channel();
    let job = Job {
        title: title.id(),
        kind: Kind::Mount,
        depth: Depth::Quick,
        rx,
        cancel: Arc::new(AtomicBool::new(false)),
        done: Arc::new(AtomicUsize::new(0)),
        total: Arc::new(AtomicUsize::new(0)),
    };
    std::thread::spawn(move || {
        let mut report = |at: usize, of: usize, what: &str| {
            let _ = tx.send(Msg::Progress {
                done: at,
                total: of,
                what: what.to_string(),
            });
        };
        let mount = match title.mount(&root, &mut report) {
            Ok(mount) => mount,
            Err(error) => {
                let _ = tx.send(Msg::Failed(error.to_string()));
                return;
            }
        };
        let now: Vec<crate::hm::storage::CachedFile> = mount
            .store
            .info()
            .iter()
            .filter_map(|info| crate::hm::storage::stamp_of(&info.path))
            .collect();
        // A container that is new, gone, resized or restamped counts as one
        // change; an install nobody has touched comes back with none.
        let changed = now
            .iter()
            .filter(|file| !expected.contains(file))
            .count()
            .max(expected.len().saturating_sub(now.len()));
        let total = now.len();
        let _ = tx.send(Msg::Checked { changed, total });
        let _ = tx.send(Msg::Ready(Box::new(mount)));
    });
    job
}

pub fn mount_only(title: Box<dyn Title>, root: PathBuf) -> Job {

    let (tx, rx) = channel();
    let job = Job {
        title: title.id(),
        kind: Kind::Mount,
        depth: Depth::Quick,
        rx,
        cancel: Arc::new(AtomicBool::new(false)),
        done: Arc::new(AtomicUsize::new(0)),
        total: Arc::new(AtomicUsize::new(0)),
    };
    std::thread::spawn(move || {
        let mut report = |at: usize, of: usize, what: &str| {
            let _ = tx.send(Msg::Progress {
                done: at,
                total: of,
                what: what.to_string(),
            });
        };
        match title.mount(&root, &mut report) {
            Ok(mount) => {
                let _ = tx.send(Msg::Ready(Box::new(mount)));
            }
            Err(error) => {
                let _ = tx.send(Msg::Failed(error.to_string()));
            }
        }
    });
    job
}
