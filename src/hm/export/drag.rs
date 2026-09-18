//! Writing the files a drag is about to hand over.
//!
//! `CF_HDROP` passes paths, not bytes, so whatever is dragged out of harmony
//! has to exist on disk before the drag starts. The files go to a scratch
//! folder, and the ones nobody took are deleted when the drag ends.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::hm::catalog::Entry;
use crate::hm::export::{Options, layout, queue};
use crate::hm::game::Mount;

#[derive(Default)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub paths: Vec<PathBuf>,
    pub failed: Option<String>,
    pub finished: bool,
}

impl Progress {
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.done as f32 / self.total as f32
        }
    }
}

/// A batch being written for a drag, flat into one folder so the drop lands as
/// a set of files rather than a tree.
pub struct Making {
    pub progress: Arc<Mutex<Progress>>,
}

impl Making {
    pub fn start(
        mount: Arc<Mutex<Mount>>,
        entries: Vec<Entry>,
        packages: Vec<String>,
        options: Options,
        into: PathBuf,
    ) -> Making {
        let progress = Arc::new(Mutex::new(Progress {
            total: entries.len(),
            ..Progress::default()
        }));
        let shared = progress.clone();

        std::thread::spawn(move || {
            let mut flat = options.clone();
            flat.layout = layout::Layout::Flat;
            flat.preserve_paths = false;
            flat.write_manifest = false;
            // A drop of a file that is already there should hand over the file,
            // not skip it and hand over nothing.
            flat.skip_duplicates = false;

            if let Err(error) = std::fs::create_dir_all(&into) {
                let mut lock = shared.lock().unwrap();
                lock.failed = Some(error.to_string());
                lock.finished = true;
                return;
            }

            let mut trouble: Option<String> = None;
            for entry in &entries {
                let package = packages
                    .get(entry.package.0 as usize)
                    .cloned()
                    .unwrap_or_else(|| "unknown".into());
                match queue::run_one(&mount, entry, &package, &into, &flat) {
                    Ok(path) => {
                        let mut lock = shared.lock().unwrap();
                        lock.done += 1;
                        lock.paths.push(path);
                    }
                    Err(error) => {
                        trouble.get_or_insert(error);
                        let mut lock = shared.lock().unwrap();
                        lock.done += 1;
                    }
                }
            }

            let mut lock = shared.lock().unwrap();
            // One failure out of many is not worth stopping a drag over; all of
            // them failing means there is nothing to drag.
            if lock.paths.is_empty() {
                lock.failed = Some(trouble.unwrap_or_else(|| "nothing was written".into()));
            }
            lock.finished = true;
        });

        Making { progress }
    }

    /// How far along the writing is, and what came of it once it is done.
    pub fn poll(&self) -> Option<Result<Vec<PathBuf>, String>> {
        let lock = self.progress.lock().ok()?;
        if !lock.finished {
            return None;
        }
        match &lock.failed {
            Some(error) => Some(Err(error.clone())),
            None => Some(Ok(lock.paths.clone())),
        }
    }

    pub fn fraction(&self) -> f32 {
        self.progress
            .lock()
            .map(|lock| lock.fraction())
            .unwrap_or(0.0)
    }
}
