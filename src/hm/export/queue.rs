use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::hm::catalog::{Entry, Source};
use crate::hm::export::{Format, Options, layout, manifest, ogg, wav};
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

#[derive(Default)]
pub struct Progress {
    pub jobs: Vec<Job>,
    pub done: usize,
    pub failed: usize,
    pub total: usize,
    pub running: bool,
    pub output: Option<PathBuf>,
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
    pub fn start(
        &self,
        mount: Arc<Mutex<Mount>>,
        entries: Vec<Entry>,
        packages: Vec<String>,
        game: String,
        root: PathBuf,
        options: Options,
    ) {
        {
            let mut progress = self.progress.lock().unwrap();
            progress.jobs = entries
                .iter()
                .map(|entry| Job {
                    label: entry.display(),
                    state: JobState::Waiting,
                })
                .collect();
            progress.done = 0;
            progress.failed = 0;
            progress.total = entries.len();
            progress.running = true;
            progress.output = Some(root.clone());
            progress.stalled = None;
        }
        self.cancel.store(false, Ordering::Relaxed);
        self.paused.store(false, Ordering::Relaxed);

        let progress = self.progress.clone();
        let cancel = self.cancel.clone();
        let paused = self.paused.clone();

        std::thread::spawn(move || {
            let mut rows: Vec<manifest::Row> = Vec::new();
            let mut written: Vec<PathBuf> = Vec::new();

            for (index, entry) in entries.iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                // A disk can fill up while the queue is running, and a run that
                // fails every remaining file is no use to anybody. Every so
                // often, and whenever the next file is a big one, the room is
                // measured again; if it has gone, the queue pauses itself and
                // says so, and waits for the user to make room or cancel.
                if index % 16 == 0 || entry.bytes > 8 * 1024 * 1024 {
                    let want = super::estimate(std::slice::from_ref(entry), options.format);
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
                        rows.push(manifest::Row {
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
                        written.push(path);
                    }
                    Err(error) => {
                        if let Some(job) = lock.jobs.get_mut(index) {
                            job.state = JobState::Failed(error);
                        }
                        lock.failed += 1;
                    }
                }
            }

            if options.write_manifest && !rows.is_empty() {
                let _ = manifest::write(&root.join("manifest.json"), &rows);
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
        Format::Ogg => {
            let stream = opus::probe(&raw).ok_or_else(|| "not an opus stream".to_string())?;
            let packets: Vec<&[u8]> = opus::packets_of(&raw, &stream).collect();
            ogg::write(&path, &packets, stream.channels, opus::FRAME as u32)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(path)
}
