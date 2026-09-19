use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::hm::catalog::{Entry, Source};
use crate::hm::console::{self, Channel, Level};
use crate::hm::export::{Format, Options, flac, layout, liblog, manifest, ogg, wav};
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

/// How often a running log is rewritten.
const CHECKPOINT: std::time::Duration = std::time::Duration::from_secs(20);

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
    /// Where the log of this run is being kept, for the runs that keep one.
    pub logged: Option<PathBuf>,
    /// What the run that is going is called.
    pub label: String,
    /// What this run would be written down as if it were put down half way.
    /// Only a library run carries one.
    pub resume: Option<crate::hm::storage::Resume>,
    /// The runs behind this one, in the order they will start.
    pub waiting: Vec<Waiting>,
    /// Set when the queue stopped itself because the disk it is writing to
    /// ran out of room. The work is paused, not abandoned: clearing this and
    /// unpausing carries on from the file it stopped at.
    pub stalled: Option<Stall>,
    /// How long the run that has just ended took.
    ///
    /// While a run is going this is empty and the clock is read off `started`.
    /// The moment it ends the time is written here and stops moving, because
    /// "took 22m 54s" should be what it took rather than a stopwatch nobody
    /// remembered to stop.
    pub spent: Option<f32>,
    /// Which lane this belongs to: 1 is the queue that has always been there,
    /// and a split view opens 2, 3 and so on beside it.
    pub lane: usize,
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

/// One export, everything it needs to run, waiting its turn.
///
/// A run carries its own mount, so two games can be lined up behind each
/// other: the second one does not borrow anything from the first.
pub struct Run {
    pub id: u64,
    /// What the window calls this run: `bo3 · 27,605 sounds`.
    pub label: String,
    pub mount: Arc<Mutex<Mount>>,
    pub entries: Vec<Entry>,
    pub packages: Vec<String>,
    pub game: String,
    pub root: PathBuf,
    pub options: Options,
    pub log: Option<liblog::Log>,
    /// What to write down if this run is put down half way. Only a library run
    /// has one: it owns a folder of its own, which is what tells a resumed run
    /// what is already there.
    pub resume: Option<crate::hm::storage::Resume>,
}

/// A run that has not started yet, as the window sees it.
#[derive(Clone, Debug)]
pub struct Waiting {
    pub id: u64,
    pub label: String,
    pub total: usize,
}

static NEXT_RUN: AtomicU64 = AtomicU64::new(1);

/// How many lanes are open.
///
/// Two lanes writing at once are still one machine, one disk and one window,
/// so what each of them takes is divided by this rather than every lane asking
/// for everything there is.
pub static LANES: AtomicUsize = AtomicUsize::new(1);

pub fn next_id() -> u64 {
    NEXT_RUN.fetch_add(1, Ordering::Relaxed)
}

pub struct Queue {
    pub progress: Arc<Mutex<Progress>>,
    /// Cancels the run that is going, not the ones behind it.
    pub cancel: Arc<AtomicBool>,
    pub paused: Arc<AtomicBool>,
    /// Runs that have not started. Sending a second export while one is going
    /// adds to this rather than throwing the first one away.
    pending: Arc<Mutex<VecDeque<Run>>>,
    /// Whether a thread is working through the line. Kept under the same lock
    /// as `pending`, so a run cannot be added at the moment the last one ends
    /// and be left sitting there with nobody to start it.
    driving: Arc<Mutex<bool>>,
    /// Stop after the run that is going, rather than starting the next.
    pub hold: Arc<AtomicBool>,
    /// Which lane this queue is: 1 unless a split view opened it.
    pub lane: usize,
}

impl Default for Queue {
    fn default() -> Queue {
        Queue {
            progress: Arc::new(Mutex::new(Progress::default())),
            cancel: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
            pending: Arc::new(Mutex::new(VecDeque::new())),
            driving: Arc::new(Mutex::new(false)),
            hold: Arc::new(AtomicBool::new(false)),
            lane: 1,
        }
    }
}

impl Queue {
    /// A queue that knows which lane it is, for the window title and the log.
    pub fn in_lane(lane: usize) -> Queue {
        let queue = Queue {
            lane,
            ..Queue::default()
        };
        if let Ok(mut lock) = queue.progress.lock() {
            lock.lane = lane;
        }
        queue
    }
}

impl Queue {
    /// How many threads write at once.
    ///
    /// Reading a blob out of a container takes the mount's lock, so the reads
    /// happen one after another whatever this says; decoding and encoding do
    /// not, and on a flac run that is nearly all of the work. Two fewer than
    /// the machine has, so the window and the player still get a core while a
    /// hundred thousand sounds are being written.
    pub fn workers() -> usize {
        let lanes = LANES.load(Ordering::Relaxed).max(1);
        std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(2).clamp(1, 8))
            .unwrap_or(2)
            .div_ceil(lanes)
            .max(1)
    }

    /// Put a run in the line.
    ///
    /// Nothing going: it starts. Something going: it waits, and the one that
    /// is running is not disturbed — which is the whole point, because the
    /// alternative is an afternoon of writing thrown away by a click.
    ///
    /// Returns true when this run started straight away.
    pub fn submit(&self, run: Run) -> bool {
        // Written down before it is even in the line. A run that is still
        // waiting when the lights go out was still asked for, and picking it
        // up later should not depend on harmony having been closed politely.
        if let Some(note) = run.resume.as_ref() {
            crate::hm::storage::save_resume(note);
        }
        console::note(
            Channel::Export,
            Level::Info,
            format!("lane {}: {} joined the line", self.lane, run.label),
            format!("{} sounds", run.entries.len()),
        );
        let mut pending = self.pending.lock().unwrap();
        let mut driving = self.driving.lock().unwrap();
        pending.push_back(run);
        self.note_waiting(&pending);
        // Said here rather than on the driver thread: a caller that asks
        // straight away whether anything is running should be told yes,
        // instead of racing the thread that is about to start.
        if let Ok(mut lock) = self.progress.lock() {
            lock.running = true;
        }
        if *driving {
            return false;
        }
        *driving = true;
        drop(driving);
        drop(pending);
        self.drive();
        true
    }

    /// The thread that works through the line, one run at a time.
    fn drive(&self) {
        let progress = self.progress.clone();
        let cancel = self.cancel.clone();
        let paused = self.paused.clone();
        let pending = self.pending.clone();
        let driving = self.driving.clone();
        let hold = self.hold.clone();
        let waiting_note = self.progress.clone();
        let lane = self.lane;

        std::thread::spawn(move || {
            loop {
                // Nothing left to do, and the flag goes down under the same
                // lock a new run would be added under: a run cannot arrive in
                // the moment between finding the line empty and saying so.
                {
                    let line = pending.lock().unwrap();
                    let mut going = driving.lock().unwrap();
                    if line.is_empty() {
                        *going = false;
                        if let Ok(mut lock) = progress.lock() {
                            lock.running = false;
                        }
                        break;
                    }
                }
                // Held: the run that was going has already finished, so
                // nothing is interrupted. The line simply does not move until
                // the hold comes off.
                if hold.load(Ordering::Relaxed) {
                    if let Ok(mut lock) = progress.lock() {
                        lock.running = false;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(120));
                    continue;
                }
                let run = {
                    let mut line = pending.lock().unwrap();
                    let Some(run) = line.pop_front() else {
                        continue;
                    };
                    let left: Vec<Waiting> = line
                        .iter()
                        .map(|run| Waiting {
                            id: run.id,
                            label: run.label.clone(),
                            total: run.entries.len(),
                        })
                        .collect();
                    if let Ok(mut lock) = waiting_note.lock() {
                        lock.waiting = left;
                    }
                    run
                };
                execute(run, &progress, &cancel, &paused, lane);
            }
        });
    }

    /// Tell the window what is still waiting.
    fn note_waiting(&self, pending: &VecDeque<Run>) {
        if let Ok(mut lock) = self.progress.lock() {
            lock.waiting = pending
                .iter()
                .map(|run| Waiting {
                    id: run.id,
                    label: run.label.clone(),
                    total: run.entries.len(),
                })
                .collect();
        }
    }

    /// Move a waiting run to the front, so it is the next one to start.
    pub fn promote(&self, id: u64) {
        let mut pending = self.pending.lock().unwrap();
        if let Some(at) = pending.iter().position(|run| run.id == id)
            && let Some(run) = pending.remove(at)
        {
            pending.push_front(run);
        }
        self.note_waiting(&pending);
    }

    /// Take a waiting run out of the line and hand it over whole, so it can be
    /// started somewhere else — the split view, which lifts something out of
    /// the line and runs it beside what is already going rather than after it.
    pub fn take(&self, id: u64) -> Option<Run> {
        let mut pending = self.pending.lock().unwrap();
        let at = pending.iter().position(|run| run.id == id)?;
        let run = pending.remove(at);
        self.note_waiting(&pending);
        run
    }

    /// Take a run out of the line without touching the one that is going.
    pub fn drop_waiting(&self, id: u64) {
        let mut pending = self.pending.lock().unwrap();
        pending.retain(|run| run.id != id);
        self.note_waiting(&pending);
    }

    /// What every run still waiting would be written down as, for a window
    /// that is closing on them: the ones in the line have got nowhere yet, so
    /// their notes are exactly what they were sent with.
    pub fn notes(&self) -> Vec<crate::hm::storage::Resume> {
        self.pending
            .lock()
            .map(|line| line.iter().filter_map(|run| run.resume.clone()).collect())
            .unwrap_or_default()
    }

    /// Empty the line. The run that is going keeps going.
    pub fn clear_waiting(&self) {
        let mut pending = self.pending.lock().unwrap();
        pending.clear();
        self.note_waiting(&pending);
    }

    /// Stop the run that is going. The line carries on with the next one.
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Stop everything: the run that is going and everything behind it.
    pub fn stop_all(&self) {
        self.clear_waiting();
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn toggle_pause(&self) {
        let now = self.paused.load(Ordering::Relaxed);
        self.paused.store(!now, Ordering::Relaxed);
    }

    /// Hold the line after the run that is going, or let it carry on.
    pub fn toggle_hold(&self) {
        let now = self.hold.load(Ordering::Relaxed);
        self.hold.store(!now, Ordering::Relaxed);
    }

    pub fn holding(&self) -> bool {
        self.hold.load(Ordering::Relaxed)
    }
}

/// Run one export to the end.
///
/// Every field of `Progress` that describes a run is set here, at the top,
/// because the run before this one left its own numbers behind and the window
/// reads them both out of the same place.
fn execute(
    run: Run,
    progress: &Arc<Mutex<Progress>>,
    cancel: &Arc<AtomicBool>,
    paused: &Arc<AtomicBool>,
    lane: usize,
) {
    let Run {
        label,
        mount,
        entries,
        packages,
        game,
        root,
        options,
        log,
        resume,
        ..
    } = run;

    // Kept out of the lock so the checkpoints below can write it down without
    // reaching back into the window's copy every twenty seconds.
    let note = resume.clone();
    let shown = label.clone();

    let total = entries.len();
    let bulk = total > LISTED;
    let workers = Queue::workers();
    {
        let mut lock = progress.lock().unwrap();
        // A run small enough to read keeps a row for every sound. A bulk run
        // keeps none: see `LISTED`.
        lock.jobs = match bulk {
            true => Vec::new(),
            false => entries
                .iter()
                .map(|entry| Job {
                    label: entry.display(),
                    state: JobState::Waiting,
                })
                .collect(),
        };
        lock.done = 0;
        lock.failed = 0;
        lock.total = total;
        lock.running = true;
        lock.output = Some(root.clone());
        lock.stalled = None;
        lock.bulk = bulk;
        lock.started = Some(Instant::now());
        lock.spent = None;
        lock.lane = lane;
        lock.recent.clear();
        lock.failures.clear();
        lock.bytes = 0;
        lock.workers = workers;
        lock.logged = log.as_ref().map(|log| log.path().to_path_buf());
        lock.label = label;
        lock.resume = resume;
    }
    cancel.store(false, Ordering::Relaxed);
    paused.store(false, Ordering::Relaxed);
    console::deep(
        Channel::Export,
        Level::Info,
        format!("lane {lane}: {shown} started"),
        vec![
            format!("into    {}", root.to_string_lossy().to_ascii_lowercase()),
            format!("format  {}", options.format.label()),
            format!("tree    {}", options.layout.label()),
            format!("threads {workers}"),
            format!("sounds  {total}"),
        ],
    );

    let entries = Arc::new(entries);
    let packages = Arc::new(packages);
    let options = Arc::new(options);
    let root = Arc::new(root);
    let next = Arc::new(AtomicUsize::new(0));
    let rows = Arc::new(Mutex::new(Vec::<manifest::Row>::new()));
    // Every failure, not the capped handful the window shows: a log is read
    // afterwards, when the whole list is the point.
    let fails = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let began = Instant::now();

    let mut log = log;
    if let Some(log) = log.as_mut() {
        log.flush();
    }

    let mut hands = Vec::with_capacity(workers);
    for _ in 0..workers {
        let entries = entries.clone();
        let packages = packages.clone();
        let options = options.clone();
        let root = root.clone();
        let next = next.clone();
        let rows = rows.clone();
        let fails = fails.clone();
        let progress = progress.clone();
        let cancel = cancel.clone();
        let paused = paused.clone();
        let mount = mount.clone();
        let game = game.clone();
        hands.push(std::thread::spawn(move || {
            let mut mine: Vec<manifest::Row> = Vec::new();
            let mut mine_failed: Vec<(String, String)> = Vec::new();
            loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(entry) = entries.get(index) else {
                    break;
                };
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
                        // The tail the bulk card shows: a few names, so a long
                        // run looks like it is moving through the library
                        // rather than sitting still.
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
                            lock.failures.push((entry.display(), error.clone()));
                        }
                        mine_failed.push((entry.display(), error));
                    }
                }
            }
            if !mine.is_empty() {
                rows.lock().unwrap().extend(mine);
            }
            if !mine_failed.is_empty() {
                fails.lock().unwrap().extend(mine_failed);
            }
        }));
    }

    // While the pool runs, what has happened so far is put down every so
    // often: the log, so a run that is killed outright still leaves something
    // readable, and the resume note, so the run can be picked up from where it
    // actually got to. Nothing here waits for a polite shutdown — a crash, a
    // pulled plug or a killed process all leave the same note behind.
    let mut wrote = Instant::now();
    while !hands.iter().all(|hand| hand.is_finished()) {
        std::thread::sleep(std::time::Duration::from_millis(250));
        if wrote.elapsed() < CHECKPOINT {
            continue;
        }
        wrote = Instant::now();
        let (done, failed) = {
            let lock = progress.lock().unwrap();
            (lock.done, lock.failed)
        };
        if let Some(note) = note.as_ref() {
            crate::hm::storage::save_resume(&crate::hm::storage::Resume {
                done,
                failed,
                total,
                at: stamp(),
                ..note.clone()
            });
        }
        if let Some(log) = log.as_mut() {
            let mut so_far = log.clone();
            so_far.say(format!(
                "still running: {done} written, {failed} failed, {total} in after {}",
                liblog::spell(began.elapsed().as_secs_f32())
            ));
            so_far.flush();
        }
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

    let (done, failed) = {
        let mut lock = progress.lock().unwrap();
        // The clock stops here rather than carrying on being read off the
        // moment the run began.
        lock.spent = Some(began.elapsed().as_secs_f32());
        (lock.done, lock.failed)
    };
    let stopped = cancel.load(Ordering::Relaxed);

    // A run that reached the end has nothing left to pick up, so its note
    // goes. A run that was stopped — cancelled, out of room, the window shut
    // on it, the power pulled — keeps one, with what it actually got through.
    if let Some(note) = note.as_ref() {
        let note = crate::hm::storage::Resume {
            done,
            failed,
            total,
            at: stamp(),
            ..note.clone()
        };
        match !stopped && done + failed >= total {
            true => crate::hm::storage::clear_resume(&note),
            false => crate::hm::storage::save_resume(&note),
        }
    }

    console::deep(
        Channel::Export,
        match (stopped, failed) {
            (true, _) => Level::Warn,
            (false, 0) => Level::Good,
            (false, _) => Level::Warn,
        },
        format!(
            "lane {lane}: {shown} {}",
            match stopped {
                true => "stopped",
                false => "finished",
            }
        ),
        vec![
            format!("written {done} of {total}, {failed} failed"),
            format!("took    {}", liblog::spell(began.elapsed().as_secs_f32())),
            format!("into    {}", root.to_string_lossy().to_ascii_lowercase()),
            match stopped && note.is_some() {
                true => "written down: it can be picked up from the same folder".to_string(),
                false => String::new(),
            },
        ]
        .into_iter()
        .filter(|row| !row.is_empty())
        .collect(),
    );

    if let Some(log) = log.as_mut() {
        log.blank();
        log.say(match stopped {
            true => "cancelled",
            false => "finished",
        });
        log.say(format!("written   {done}"));
        log.say(format!("failed    {failed}"));
        log.say(format!("asked for {total}"));
        log.say(format!("took      {}", liblog::spell(began.elapsed().as_secs_f32())));
        log.say(format!("ended     {}", stamp()));
        let fails = fails.lock().unwrap();
        if !fails.is_empty() {
            log.blank();
            log.say(format!("what failed ({})", fails.len()));
            for (name, why) in fails.iter() {
                log.say(format!("  {name}  {why}"));
            }
        }
        log.flush();
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
