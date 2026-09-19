use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::hm::catalog::{Entry, Source};
use crate::hm::export::{Format, Options, flac, layout, manifest, ogg, wav};
use crate::hm::game::Mount;
use crate::hm::sound::{decode, opus};
use crate::hm::storage::space;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobState {
    Waiting,
    Running,
    Done,
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct Job {
    pub label: String,
    pub state: JobState,
}

/// How many sounds a run has to be under for the queue to keep a row for every
/// one of them.
///
/// A row is a name and a state, and the window draws them all. At a few
/// thousand that is a list somebody can actually read; at a hundred and twenty
/// thousand it is a megabyte of strings nobody scrolls, copied every frame. A
/// run above this line keeps counts, the last few names and the failures, which
/// is what a run that size is actually watched by.
pub const LISTED: usize = 4000;

/// How many finished names the bulk card keeps behind it.
pub const RECENT: usize = 6;

/// How many failures are kept with what went wrong. Past this the count still
/// rises; the reasons stop being collected, because they repeat.
pub const FAILURES: usize = 200;

#[derive(Default)]
pub struct Progress {
    pub jobs: Vec<Job>,
    pub done: usize,
    pub failed: usize,
    pub total: usize,
    pub running: bool,
    pub output: Option<PathBuf>,
    /// A run too big to list every row of: counts and a tail instead.
    pub bulk: bool,
    /// When the run began, for the rate and what is left of it.
    pub started: Option<Instant>,
    /// The last few sounds written, newest last.
    pub recent: VecDeque<String>,
    /// What went wrong, and on which sound. Capped: a run that fails
    /// everything should not cost more memory than the run itself.
    pub failures: Vec<(String, String)>,
    /// What has actually landed on disk.
    pub bytes: u64,
    /// How many threads are writing.
    pub workers: usize,
    /// Set when the queue stopped itself because the disk it is writing to
    /// ran out of room. The work is paused, not abandoned: clearing this and
    /// unpausing carries on from the file it stopped at.
    pub stalled: Option<Stall>,
}

#[derive(Clone, Debug)]
pub struct Stall {
    pub path: PathBuf,
    pub free: u64,
    pub want: u64,
}

impl Progress {
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.done + self.failed) as f32 / self.total as f32
        }
    }
}

pub struct Queue {
    pub progress: Arc<Mutex<Progress>>,
    pub cancel: Arc<AtomicBool>,
    pub paused: Arc<AtomicBool>,
}

impl Default for Queue {
    fn default() -> Queue {
        Queue {
            progress: Arc::new(Mutex::new(Progress::default())),
            cancel: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Queue {
    /// How many threads write at once.
    ///
    /// Reading a blob out of a container takes the mount's lock, so the reads
    /// happen one after another whatever this says; decoding and encoding do
    /// not, and on a flac or an mp3 run that is nearly all of the work. Two
    /// fewer than the machine has, so the window and the player still get a
    /// core while a hundred thousand sounds are being written.
    fn workers() -> usize {
        std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(2).clamp(1, 8))
            .unwrap_or(2)
    }

    pub fn start(
        &self,
        mount: Arc<Mutex<Mount>>,
        entries: Vec<Entry>,
        packages: Vec<String>,
        game: String,
        root: PathBuf,
        options: Options,
    ) {
        let total = entries.len();
        let bulk = total > LISTED;
        let workers = Self::workers();
        {
            let mut progress = self.progress.lock().unwrap();
            // A run small enough to read keeps a row for every sound. A bulk
            // run keeps none: see `LISTED`.
            progress.jobs = match bulk {
                true => Vec::new(),
                false => entries
                    .iter()
                    .map(|entry| Job {
                        label: entry.display(),
                        state: JobState::Waiting,
                    })
                    .collect(),
            };
            progress.done = 0;
            progress.failed = 0;
            progress.total = total;
            progress.running = true;
            progress.output = Some(root.clone());
            progress.stalled = None;
            progress.bulk = bulk;
            progress.started = Some(Instant::now());
            progress.recent.clear();
            progress.failures.clear();
            progress.bytes = 0;
            progress.workers = workers;
        }
        self.cancel.store(false, Ordering::Relaxed);
        self.paused.store(false, Ordering::Relaxed);

        let progress = self.progress.clone();
        let cancel = self.cancel.clone();
        let paused = self.paused.clone();

        // One thread to run the pool, so the caller is never held up and the
        // manifest is written once, after the last worker has stopped.
        std::thread::spawn(move || {
            let entries = Arc::new(entries);
            let packages = Arc::new(packages);
            let options = Arc::new(options);
            let root = Arc::new(root);
            let next = Arc::new(AtomicUsize::new(0));
            let rows = Arc::new(Mutex::new(Vec::<manifest::Row>::new()));

            let mut hands = Vec::with_capacity(workers);
            for _ in 0..workers {
                let entries = entries.clone();
                let packages = packages.clone();
                let options = options.clone();
                let root = root.clone();
                let next = next.clone();
                let rows = rows.clone();
                let progress = progress.clone();
                let cancel = cancel.clone();
                let paused = paused.clone();
                let mount = mount.clone();
                let game = game.clone();
                hands.push(std::thread::spawn(move || {
                    let mut mine: Vec<manifest::Row> = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(entry) = entries.get(index) else {
                            break;
                        };
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        // A disk can fill up while the queue is running, and a
                        // run that fails every remaining file is no use to
                        // anybody. Every so often, and whenever the next file
                        // is a big one, the room is measured again; if it has
                        // gone, the queue pauses itself and says so, and waits
                        // for the user to make room or cancel.
                        if index % 16 == 0 || entry.bytes > 8 * 1024 * 1024 {
                            let want =
                                super::estimate(std::slice::from_ref(entry), options.format);
                            if let Some(room) = space::shortfall(&root, want) {
                                let mut lock = progress.lock().unwrap();
                                lock.stalled = Some(Stall {
                                    path: room.path.clone(),
                                    free: room.free,
                                    want: room.needed(),
                                });
                                drop(lock);
                                paused.store(true, Ordering::Relaxed);
                            }
                        }
                        while paused.load(Ordering::Relaxed) && !cancel.load(Ordering::Relaxed) {
                            std::thread::sleep(std::time::Duration::from_millis(80));
                        }
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        {
                            let mut lock = progress.lock().unwrap();
                            if let Some(job) = lock.jobs.get_mut(index) {
                                job.state = JobState::Running;
                            }
                        }

                        let package = packages
                            .get(entry.package.0 as usize)
                            .cloned()
                            .unwrap_or_else(|| "unknown".into());
                        let result = run_one(&mount, entry, &package, &root, &options);

                        let mut lock = progress.lock().unwrap();
                        match result {
                            Ok(path) => {
                                if let Some(job) = lock.jobs.get_mut(index) {
                                    job.state = JobState::Done;
                                }
                                lock.done += 1;
                                lock.bytes += std::fs::metadata(&path)
                                    .map(|about| about.len())
                                    .unwrap_or(0);
                                // The tail the bulk card shows: a few names, so
                                // a long run looks like it is moving through the
                                // library rather than sitting still.
                                lock.recent.push_back(entry.display());
                                while lock.recent.len() > RECENT {
                                    lock.recent.pop_front();
                                }
                                if options.write_manifest {
                                    mine.push(manifest::Row {
                                        name: entry.display(),
                                        game: game.clone(),
                                        category: entry.category.label().to_string(),
                                        package,
                                        source: match entry.source {
                                            Source::Stream { key } => format!("{key:016x}"),
                                            Source::Bank { package, index } => {
                                                format!("bank {}:{index}", package.0)
                                            }
                                        },
                                        output: path.to_string_lossy().to_string(),
                                        format: options.format.label().to_string(),
                                        rate: entry.rate,
                                        channels: entry.channels,
                                        seconds: entry.seconds(),
                                        at: stamp(),
                                    });
                                }
                            }
                            Err(error) => {
                                if let Some(job) = lock.jobs.get_mut(index) {
                                    job.state = JobState::Failed(error.clone());
                                }
                                lock.failed += 1;
                                if lock.failures.len() < FAILURES {
                                    lock.failures.push((entry.display(), error));
                                }
                            }
                        }
                    }
                    if !mine.is_empty() {
                        rows.lock().unwrap().extend(mine);
                    }
                }));
            }
            for hand in hands {
                let _ = hand.join();
            }

            if options.write_manifest {
                let rows = rows.lock().unwrap();
                if !rows.is_empty() {
                    let _ = manifest::write(&root.join("manifest.json"), &rows);
                }
            }
            let mut lock = progress.lock().unwrap();
            lock.running = false;
        });
    }

    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn toggle_pause(&self) {
        let now = self.paused.load(Ordering::Relaxed);
        self.paused.store(!now, Ordering::Relaxed);
    }
}

fn stamp() -> String {
    crate::hm::storage::stamp()
}

/// Write bytes out as they stand, making the folder first.
fn put(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

pub fn run_one(
    mount: &Arc<Mutex<Mount>>,
    entry: &Entry,
    package: &str,
    root: &std::path::Path,
    options: &Options,
) -> Result<PathBuf, String> {
    let raw = {
        let mut lock = mount.lock().map_err(|_| "mount is poisoned".to_string())?;
        let Mount { store, oodle, .. } = &mut *lock;
        store
            .read(entry.package, entry.index as usize, oodle.as_ref())
            .map_err(|e| e.to_string())?
    };

    let path = layout::path_for(
        root,
        entry,
        package,
        options.layout,
        options.normalise_names,
        options.format.extension(),
    );
    if options.skip_duplicates && path.exists() {
        return Ok(path);
    }

    match options.format {
        Format::Raw => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&path, &raw).map_err(|e| e.to_string())?;
        }
        Format::Wav => {
            let about = decode::sniff(&raw).ok_or_else(|| "no audio found".to_string())?;
            let samples = decode::decode(&raw, about).map_err(|e| e.to_string())?;
            wav::write(&path, &samples).map_err(|e| e.to_string())?;
        }
        // A sound already in the format asked for is copied, not encoded
        // again: most of what harmony reads is flac to begin with, and passing
        // it through is both faster and more honest than a re-encode that
        // could only lose something.
        Format::Flac => match flac::passthrough(&raw) {
            Some(bytes) => put(&path, bytes)?,
            None => {
                let about = decode::sniff(&raw).ok_or_else(|| "no audio found".to_string())?;
                let samples = decode::decode(&raw, about).map_err(|e| e.to_string())?;
                flac::write(&path, &samples).map_err(|e| e.to_string())?;
            }
        },
        Format::Ogg => {
            let stream = opus::probe(&raw).ok_or_else(|| "not an opus stream".to_string())?;
            let packets: Vec<&[u8]> = opus::packets_of(&raw, &stream).collect();
            ogg::write(&path, &packets, stream.channels, opus::FRAME as u32)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(path)
}
