use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::hm::analysis::{self, Stats, peaks::Peaks};
use crate::hm::catalog::group::{GroupBy, Node};
use crate::hm::catalog::hash::HashFn;
use crate::hm::catalog::names::NameDb;
use crate::hm::catalog::{Catalog, Entry, Name, SoundId, category::Category, group};
use crate::hm::discord::{Presence, Rpc};
use crate::hm::export::{Options, queue::Queue};
use crate::hm::game::{Fingerprint, Mount, Support, TitleId, detect, title_for};
use crate::hm::query::{Query, eval, parse};
use crate::hm::scan::{self, Depth};
use crate::hm::sound::{Samples, decode};
use crate::hm::storage::{self, Settings, space};
use crate::hm::ui::{self, Pane, View, theme};

pub struct Loaded {
    pub id: SoundId,
    pub samples: Samples,
    pub peaks: Peaks,
    pub stats: Stats,
    pub columns: Vec<Vec<f32>>,
}

#[derive(Default, Clone)]
pub struct Mounted {
    pub packages: Vec<String>,
    pub keys: usize,
    /// What kind of container this install turned out to use.
    pub container: String,
    /// The version of the game these packages came out of, where the install
    /// says: it names the folder a whole library is written into, so two
    /// patches of the same game do not land on top of each other.
    pub build: Option<String>,
}

pub struct State {
    pub settings: Settings,
    pub rpc: Rpc,
    pub started: i64,
    pub icon: Option<egui::TextureHandle>,

    pub root: Option<PathBuf>,
    pub detected: Vec<Fingerprint>,
    pub title: Option<TitleId>,

    pub mount: Option<Arc<Mutex<Mount>>>,
    pub mounted: Mounted,
    pub catalog: Catalog,
    pub names: std::sync::Arc<NameDb>,
    /// A matching pass running on a worker: keys go out, names come back.
    pub naming: Option<Naming>,

    pub depth: Depth,

    pub query_text: String,
    pub query: Query,
    pub filtered: Vec<usize>,
    pub tree: Node,
    pub tree_pick: Option<String>,
    pub group_by: GroupBy,
    pub view: View,
    pub pane: Pane,

    pub cursor: Option<usize>,
    pub selection: BTreeSet<usize>,
    pub loaded: Option<Loaded>,
    /// The sound held as "b" for comparison against whatever is selected.
    pub against: Option<Loaded>,
    pub similar: Vec<(f32, usize)>,
    pub recent: Vec<SoundId>,
    pub favorites: BTreeSet<u64>,
    pub tags: HashMap<u64, Vec<String>>,
    pub collections: BTreeMap<String, BTreeSet<u64>>,
    pub collection_pick: Option<String>,
    pub collection_name: String,
    pub preset_name: String,

    /// The scans running right now, one per game. Switching tabs does not stop
    /// them: only the game on screen keeps its sounds, and the rest write
    /// their caches and are read back when their tab is opened.
    pub jobs: Vec<scan::Job>,
    /// What each running job is doing, for the line along the bottom.
    pub work: HashMap<TitleId, Work>,
    /// A folder being fingerprinted on a worker thread.
    pub looking: Option<Looking>,
    /// A cache being read and turned back into entries, a slice per frame.
    pub loading: Option<Loading>,
    /// Games put down but not forgotten, newest last.
    pub shelf: Vec<Shelved>,
    /// A sound being read, decoded and measured on a worker.
    pub sounding: Option<Sounding>,
    /// True until the first folder and cache are back, while the window
    /// stands behind a veil saying so.
    pub booting: bool,
    /// Name lists being read on a worker.
    pub names_rx: Option<std::sync::mpsc::Receiver<NameDb>>,
    /// What the disks said, a few seconds ago. Reading them on every frame is
    /// a syscall and a directory listing per panel, which is not free.
    pub disk: Disk,
    /// The filtered list is behind the catalogue and needs rebuilding.
    pub dirty: bool,
    /// When it was last rebuilt, so a scan pouring entries in does not rebuild
    /// it on every frame.
    pub filtered_at: std::time::Instant,

    pub transport: Option<crate::hm::player::Transport>,
    pub queue: Queue,
    pub options: Options,
    pub output: Option<PathBuf>,

    pub status: String,
    pub error: Option<String>,
    pub show_queue: bool,
    /// When the export now running began, so discord counts the run rather
    /// than the session.
    pub export_since: Option<i64>,
    /// When the presence line was last pushed, and whether the last push was
    /// an export: together they keep a long run moving on discord without
    /// asking every frame.
    pub presence_at: i64,
    pub was_exporting: bool,
    /// Work that stopped because there was not enough room for it, and what
    /// to do again once there is.
    pub blocked: Option<Blocked>,
    /// The carry in progress: the files, the card over the window, and the
    /// writing that may still be going on behind it.
    pub dragging_out: Option<DragOut>,
}

/// What the disks harmony writes to last said.
///
/// The folder card and the detail panel both want free space, and both are
/// drawn every frame; asking the filesystem that often is wasted work, so the
/// answer is taken every few seconds and kept.
#[derive(Clone)]
pub struct Disk {
    pub checked: std::time::Instant,
    pub cache_free: Option<u64>,
    pub cache_held: u64,
    pub scratch_free: Option<u64>,
    pub export_free: Option<u64>,
}

impl Default for Disk {
    fn default() -> Disk {
        Disk {
            // Far enough back that the first frame takes a reading.
            checked: std::time::Instant::now() - std::time::Duration::from_secs(60),
            cache_free: None,
            cache_held: 0,
            scratch_free: None,
            export_free: None,
        }
    }
}

/// What one running job is doing.
#[derive(Clone)]
pub struct Work {
    pub done: usize,
    pub total: usize,
    pub what: String,
    pub depth: Depth,
    pub kind: scan::Kind,
    pub found: usize,
}

/// A folder being fingerprinted. Detection reads directories, which on a big
/// install is slow enough to drop frames, so it happens on a thread and the
/// window says what it is doing meanwhile.
pub struct Looking {
    pub root: PathBuf,
    /// The game whose tab asked for this folder, which wins over the
    /// highest-scoring print when both are found in it.
    pub want: Option<TitleId>,
    pub rx: std::sync::mpsc::Receiver<Vec<Fingerprint>>,
}

/// A cached catalogue coming back from disk.
///
/// The file is read and parsed on a thread; the entries are built from it a
/// slice at a time, so a fifty thousand sound catalogue arrives over a few
/// frames instead of stopping the window for all of them.
/// A game that was open a moment ago, kept whole.
///
/// Reading a cache back is fifty megabytes of json and opening an install is
/// two hundred containers; doing both again because a tab was clicked twice is
/// the pause this avoids. Only the last few are kept, and only what was
/// already in hand — nothing is read to fill this.
pub struct Shelved {
    pub title: TitleId,
    /// The folder it was read out of. A game pointed somewhere else is a
    /// different game's worth of sounds, so what is kept here no longer
    /// answers for it.
    pub root: Option<PathBuf>,
    pub entries: Vec<crate::hm::catalog::Entry>,
    pub mounted: Mounted,
    pub mount: Option<Arc<Mutex<Mount>>>,
    pub depth: Depth,
    pub scanned_at: Option<String>,
    /// The list as it was being looked at, and the tree beside it.
    ///
    /// Working these out again is a pass over every sound in the game and a
    /// tree built from nothing, which is the pause a person sees when they
    /// click back. They are only worth keeping while they still answer the
    /// same question, so what was being asked is kept with them.
    pub filtered: Vec<usize>,
    pub tree: Node,
    pub asked: Asked,
    pub cursor: Option<usize>,
}

/// What the list was showing: the search, the view, the grouping and whatever
/// branch or collection was picked.
#[derive(Clone, PartialEq, Eq)]
pub struct Asked {
    pub query: String,
    pub view: View,
    pub group_by: GroupBy,
    pub tree_pick: Option<String>,
    pub collection_pick: Option<String>,
}

/// How many games are kept. Three is the back-and-forth a person actually
/// does; more is memory held for a tab nobody returns to.
const SHELF: usize = 3;

pub struct Loading {
    pub title: TitleId,
    pub depth: Depth,
    pub rx: std::sync::mpsc::Receiver<Option<Box<storage::Cached>>>,
    pub cache: Option<Box<storage::Cached>>,
    pub at: usize,
}

/// A sound on its way in.
///
/// Reading a blob, decompressing it, decoding it and measuring it is the whole
/// of a file's length of work: a six minute mp3 is sixteen million samples, and
/// doing that between two frames is a window that stops dead every time a row
/// is clicked. It happens on a worker, and the waveform appears when it lands.
pub struct Sounding {
    pub index: usize,
    pub id: SoundId,
    pub name: String,
    /// What harmony was busy with when this sound was clicked.
    ///
    /// Reading a sound out of a container that is still being opened waits on
    /// that container, so the panel says which one rather than going blank and
    /// coming back a moment later with no explanation.
    pub area: String,
    pub play: bool,
    pub against: bool,
    pub rx: std::sync::mpsc::Receiver<Option<Box<Loaded>>>,
}

/// How many cached sounds are turned into entries in one frame.
const SLICE: usize = 6000;

/// Work harmony will not start because the disk it writes to has not got the
/// room, held until the user makes room, moves the folder, or gives it up.
#[derive(Clone, Debug)]
pub struct Blocked {
    /// What was being attempted, in the words the window uses: `cache`,
    /// `extract`, `drag out`.
    pub what: String,
    pub room: crate::hm::storage::space::Room,
    pub again: Again,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Again {
    Cache,
    Extract { all: bool },
    /// The whole library, into a folder of its own.
    Library,
    DragOut,
}

/// What an export says for itself on discord while it runs.
pub struct Running {
    pub details: String,
    pub what: String,
}

/// A drag of sounds out of harmony and into something else.
pub struct DragOut {
    pub paths: Vec<PathBuf>,
    pub name: String,
    /// Whether these files were written for the drag, and so are harmony's to
    /// delete when nobody takes them.
    pub temp: bool,
    /// Whether the card has been through a whole frame yet. The drag blocks the
    /// ui thread, so it waits for the card to have actually reached the screen.
    pub painted: bool,
    pub making: Option<crate::hm::export::drag::Making>,
}

impl State {
    pub fn new() -> State {
        let settings = storage::load();
        // The cache and scratch folders are read from settings by code with no
        // settings to hand, so they are published before anything writes.
        storage::use_folders(&settings);
        let rpc = if settings.discord {
            Rpc::new(&settings.discord_app_id)
        } else {
            Rpc::disabled()
        };
        let mut options = Options::default();
        options.format = settings.format();
        options.layout = settings.layout();
        options.normalise_names = settings.normalise_names;
        options.skip_duplicates = settings.skip_duplicates;
        options.write_manifest = settings.write_manifest;

        let favorites = settings
            .favorites
            .iter()
            .filter_map(|k| u64::from_str_radix(k, 16).ok())
            .collect();
        let collections: BTreeMap<String, BTreeSet<u64>> = settings
            .collections
            .iter()
            .map(|(name, keys)| {
                (
                    name.clone(),
                    keys.iter()
                        .filter_map(|key| u64::from_str_radix(key, 16).ok())
                        .collect(),
                )
            })
            .collect();
        let tags: HashMap<u64, Vec<String>> = settings
            .tags
            .iter()
            .filter_map(|(key, tags)| {
                u64::from_str_radix(key, 16).ok().map(|key| (key, tags.clone()))
            })
            .collect();

        let mut state = State {
            root: settings.last_root.clone(),
            output: settings.output.clone(),
            settings,
            rpc,
            started: crate::hm::discord::now_ms(),
            icon: None,
            detected: Vec::new(),
            title: None,
            mount: None,
            mounted: Mounted::default(),
            catalog: Catalog::default(),
            names: std::sync::Arc::new(NameDb::default()),
            naming: None,
            depth: Depth::Quick,
            query_text: String::new(),
            query: Query::default(),
            filtered: Vec::new(),
            tree: Node::default(),
            tree_pick: None,
            group_by: GroupBy::Category,
            view: View::Detailed,
            pane: Pane::Waveform,
            cursor: None,
            selection: BTreeSet::new(),
            loaded: None,
            against: None,
            similar: Vec::new(),
            recent: Vec::new(),
            favorites,
            tags,
            collections,
            collection_pick: None,
            collection_name: String::new(),
            preset_name: String::new(),
            jobs: Vec::new(),
            work: HashMap::new(),
            booting: false,
            names_rx: None,
            disk: Disk::default(),
            looking: None,
            loading: None,
            shelf: Vec::new(),
            sounding: None,
            dirty: false,
            filtered_at: std::time::Instant::now(),
            transport: crate::hm::player::Transport::open().ok(),
            queue: Queue::default(),
            options,
            status: "pick a game folder".into(),
            error: None,
            show_queue: false,
            export_since: None,
            presence_at: 0,
            was_exporting: false,
            blocked: None,
            dragging_out: None,
        };
        crate::hm::window::dragout::sweep();
        // Name lists can be millions of lines, and the last folder has to be
        // fingerprinted before anything is known about it. Neither happens on
        // the ui thread: the window goes up first, behind a veil, and fills in.
        let lists = state.settings.name_files.clone();
        if !lists.is_empty() {
            state.booting = true;
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let mut names = NameDb::default();
                read_names(&mut names, lists);
                let _ = tx.send(names);
            });
            state.names_rx = Some(rx);
        }
        if let Some(root) = state.root.clone() {
            state.booting = true;
            state.look(root);
        }
        state
    }

    /// Point harmony at a folder.
    ///
    /// Fingerprinting reads directories, and on a big install that is slow
    /// enough to drop frames, so it happens on a thread. `want` is the game
    /// whose tab asked, which wins over the highest-scoring print when the
    /// folder holds more than one: ghosts and advanced warfare look alike from
    /// the outside, and the tab that was clicked is the answer.
    pub fn look(&mut self, root: PathBuf) {
        self.look_for(root, None);
    }

    pub fn look_for(&mut self, root: PathBuf, want: Option<TitleId>) {
        self.shelve();
        self.forget_open();
        self.root = Some(root.clone());
        self.detected.clear();
        self.title = None;
        self.status = format!(
            "looking at {}",
            crate::hm::discord::clip(&root.to_string_lossy().to_ascii_lowercase(), 40)
        );
        let (tx, rx) = std::sync::mpsc::channel();
        let asked = root.clone();
        std::thread::spawn(move || {
            let _ = tx.send(detect(&asked));
        });
        self.looking = Some(Looking { root, want, rx });
    }

    /// The fingerprints came back.
    fn pump_looking(&mut self) {
        let Some(looking) = self.looking.as_ref() else {
            return;
        };
        let Ok(found) = looking.rx.try_recv() else {
            return;
        };
        let Looking { root, want, .. } = self.looking.take().expect("just looked");

        self.detected = found;
        self.settings.last_root = Some(root.clone());
        self.title = match want.filter(|id| self.detected.iter().any(|p| p.title == *id)) {
            Some(id) => Some(id),
            None => self.detected.first().map(|print| print.title),
        };
        // Only a game this folder proves is here is remembered by it. A weak
        // print, meaning the right kind of container and none of that game's
        // own files, is enough to offer and not enough to send a tab back here
        // later: that is how the ghosts tab used to land in advanced warfare.
        for print in &self.detected {
            if print.score >= 90 || Some(print.title) == want {
                self.settings
                    .roots
                    .insert(print.title.key().to_string(), root.clone());
            }
        }
        let asked_for = want.filter(|id| !self.detected.iter().any(|p| p.title == *id));
        self.status = match (asked_for, self.title.and_then(title_for)) {
            // The folder was picked for one game and holds another: say which,
            // rather than quietly opening something else.
            (Some(id), Some(here)) => format!(
                "that folder holds {}, not {}",
                here.label(),
                title_label(id)
            ),
            (Some(id), None) => format!("no {} in that folder", title_label(id)),
            (None, Some(title)) => format!("{} detected", title.label()),
            (None, None) => "nothing harmony reads is in that folder".into(),
        };
        storage::save(&self.settings);
        // A game read out of this same folder a moment ago is still in hand.
        if !self.title.is_some_and(|id| self.unshelve(id)) {
            self.load_cache();
        }
        // Whatever this folder turned out to be, discord is told: the tab that
        // is open is the one the line names.
        self.push_presence();
    }

    /// Put down whatever install was open. Switching games with a catalogue
    /// still loaded would leave the last game's rows under the new game's name.
    fn forget_open(&mut self) {
        // Scans already running are left alone: they are writing their own
        // caches, and this game's rows are simply not kept while its tab is
        // not the one on screen.
        self.loading = None;
        self.catalog.clear();
        self.catalog.scanned_at = None;
        self.catalog.title = None;
        self.mount = None;
        self.mounted = Mounted::default();
        self.filtered.clear();
        self.tree = Node::default();
        self.tree_pick = None;
        self.selection.clear();
        self.cursor = None;
        self.loaded = None;
        self.against = None;
        self.similar.clear();
        self.recent.clear();
        if let Some(transport) = self.transport.as_mut() {
            transport.stop();
        }
    }

    /// The folder a game was last read from, if it is still there.

    /// Point the scan caches at another folder, and carry what is already
    /// written over to it. A cache is rebuildable, so nothing is overwritten at
    /// the far end; anything in the way is left where it is.
    pub fn set_cache_dir(&mut self, folder: Option<PathBuf>) {
        let from = storage::cache_dir();
        self.settings.cache_dir = folder;
        storage::use_folders(&self.settings);
        storage::save(&self.settings);
        let to = storage::cache_dir();
        let moved = storage::move_caches(&from, &to);
        self.status = match moved {
            0 => format!("caching to {}", to.to_string_lossy().to_ascii_lowercase()),
            n => format!(
                "caching to {} - {n} moved",
                to.to_string_lossy().to_ascii_lowercase()
            ),
        };
    }

    /// Point the scratch folder a drag out uses somewhere else. Whatever is in
    /// the old one is nobody's: it is swept at startup anyway.
    pub fn set_temp_dir(&mut self, folder: Option<PathBuf>) {
        self.settings.temp_dir = folder;
        storage::use_folders(&self.settings);
        storage::save(&self.settings);
        self.status = format!(
            "scratch files go to {}",
            crate::hm::window::dragout::scratch_dir()
                .to_string_lossy()
                .to_ascii_lowercase()
        );
    }

    pub fn set_output_dir(&mut self, folder: PathBuf) {
        self.output = Some(folder.clone());
        self.settings.output = Some(folder);
        storage::save(&self.settings);
    }

    /// Try the work that was stopped for want of room again, now that the user
    /// says there is some. If there still is not, it simply blocks again.
    pub fn retry_blocked(&mut self) {
        let Some(blocked) = self.blocked.take() else {
            return;
        };
        match blocked.again {
            Again::Cache => self.save_cache(),
            Again::Extract { all } => self.extract(all),
            Again::Library => self.extract_library(),
            // A carry is a gesture, not a job: there is nothing to resume, so
            // the way back is to drag the rows again.
            Again::DragOut => self.status = "drag the rows out again".into(),
        }
    }

    /// Send the blocked work somewhere else: the folder picker, pointed at
    /// whichever folder was too full.
    pub fn repoint_blocked(&mut self) {
        let Some(blocked) = self.blocked.clone() else {
            return;
        };
        let Some(folder) = rfd::FileDialog::new()
            .set_title("somewhere with more room")
            .pick_folder()
        else {
            return;
        };
        match blocked.again {
            Again::Cache => self.set_cache_dir(Some(folder)),
            Again::Extract { .. } | Again::Library => self.set_output_dir(folder),
            Again::DragOut => self.set_temp_dir(Some(folder)),
        }
        // The folder moved, so the work is tried where it now points.
        self.retry_blocked();
    }

    /// Put the open game down where it can be picked up again.
    ///
    /// A game still being read is not shelved: half a catalogue kept as if it
    /// were whole is worse than reading it again.
    fn shelve(&mut self) {
        let (Some(title), true) = (self.title, self.loading.is_none()) else {
            return;
        };
        if self.catalog.is_empty() {
            return;
        }
        self.shelf.retain(|shelved| shelved.title != title);
        let asked = self.asked();
        self.shelf.push(Shelved {
            title,
            root: self.root.clone(),
            entries: std::mem::take(&mut self.catalog.entries),
            mounted: std::mem::take(&mut self.mounted),
            mount: self.mount.clone(),
            depth: self.depth,
            scanned_at: self.catalog.scanned_at.clone(),
            filtered: std::mem::take(&mut self.filtered),
            tree: std::mem::take(&mut self.tree),
            asked,
            cursor: self.cursor,
        });
        while self.shelf.len() > SHELF {
            self.shelf.remove(0);
        }
    }

    /// Pick a game back up, if it is on the shelf and was read out of the
    /// folder that is open now. Says whether it was.
    fn unshelve(&mut self, id: TitleId) -> bool {
        let here = self.root.clone();
        let Some(at) = self
            .shelf
            .iter()
            .position(|shelved| shelved.title == id && shelved.root == here)
        else {
            return false;
        };
        let shelved = self.shelf.remove(at);
        self.catalog.entries = shelved.entries;
        self.catalog.title = Some(id);
        self.catalog.scanned_at = shelved.scanned_at;
        self.mounted = shelved.mounted;
        self.mount = shelved.mount;
        self.depth = shelved.depth;
        self.status = format!(
            "{} sounds, still here",
            ui::widgets::tally(self.catalog.len())
        );
        // The list is only reused where it still answers what is being asked:
        // change the search or the view while another game is open and this
        // one is worked out again, as it has to be.
        if shelved.asked == self.asked() {
            self.filtered = shelved.filtered;
            self.tree = shelved.tree;
            self.cursor = shelved.cursor;
            self.dirty = false;
        } else {
            self.refilter();
        }
        true
    }

    /// What the list is being asked for right now.
    fn asked(&self) -> Asked {
        Asked {
            query: self.query_text.clone(),
            view: self.view,
            group_by: self.group_by,
            tree_pick: self.tree_pick.clone(),
            collection_pick: self.collection_pick.clone(),
        }
    }

    /// Forget what is kept for a game whose catalogue is about to change.
    fn unshelf(&mut self, id: TitleId) {
        self.shelf.retain(|shelved| shelved.title != id);
    }

    pub fn root_for(&self, id: TitleId) -> Option<PathBuf> {
        self.settings
            .roots
            .get(id.key())
            .filter(|path| path.is_dir())
            .cloned()
    }

    /// What a tab does when it is clicked.
    ///
    /// The game is either in the folder that is open, or in one harmony has
    /// been shown before, or nowhere yet — and in that last case the click is
    /// the ask for it, rather than a button that does nothing.
    pub fn switch(&mut self, id: TitleId) {
        if self.title == Some(id) {
            return;
        }
        // Already in the folder that is open: only the view changes, and the
        // catalogue comes back from this game's own cache.
        if self.detected.iter().any(|print| print.title == id) {
            self.shelve();
            self.title = Some(id);
            self.catalog.clear();
            self.filtered.clear();
            self.cursor = None;
            self.selection.clear();
            self.mount = None;
            self.mounted = Mounted::default();
            if !self.unshelve(id) {
                self.load_cache();
            }
            self.push_presence();
            return;
        }
        // Somewhere harmony has been shown before. Fingerprinting it happens on
        // a thread, and the rest follows when it comes back.
        if let Some(root) = self.root_for(id) {
            self.look_for(root, Some(id));
            return;
        }
        self.pick_folder_for(id);
    }

    /// Ask for a game's folder by name, and say so plainly if what came back is
    /// not that game.
    pub fn pick_folder_for(&mut self, id: TitleId) {
        let Some(folder) = rfd::FileDialog::new()
            .set_title(format!("where is {}?", title_label(id)))
            .pick_folder()
        else {
            return;
        };
        self.look_for(folder, Some(id));
    }

    pub fn pick_folder(&mut self) {
        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
            self.look(folder);
        }
    }

    /// Keep the size the window was left at, so it opens the same next time.
    /// Only once the pointer is up: a resize is hundreds of frames, and each
    /// one of them is not worth a write to disk.
    pub fn remember_size(&mut self, ctx: &egui::Context) {
        if ctx.input(|state| state.pointer.any_down()) {
            return;
        }
        let Some(size) = crate::hm::window::chrome::size(ctx) else {
            return;
        };
        let same = self.settings.window_size.is_some_and(|was| {
            (was[0] - size[0]).abs() < 1.0 && (was[1] - size[1]).abs() < 1.0
        });
        if same {
            return;
        }
        self.settings.window_size = Some(size);
        storage::save(&self.settings);
    }

    /// Ask for name lists. Harmony ships none: a list is either `hash,name`
    /// pairs or one name per line, and the game itself only stores the hashes.
    pub fn pick_names(&mut self) {
        let picked = rfd::FileDialog::new()
            .add_filter("name lists", &["txt", "csv", "lst", "wni"])
            .pick_files();
        if let Some(paths) = picked {
            self.load_names(paths);
        }
    }

    /// Take on more name lists.
    ///
    /// Reading them is file work and hashing, so it happens on a thread and
    /// the whole set is read together: the database is replaced rather than
    /// added to, which keeps one path for both this and startup.
    pub fn load_names(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            if !self.settings.name_files.contains(&path) {
                self.settings.name_files.push(path);
            }
        }
        storage::save(&self.settings);
        let lists = self.settings.name_files.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut names = NameDb::default();
            read_names(&mut names, lists);
            let _ = tx.send(names);
        });
        self.names_rx = Some(rx);
        self.status = "reading name lists".into();
    }

    /// Start matching the catalogue against the names, on a worker.
    ///
    /// Every sound that is still a key goes out; whatever comes back is put on
    /// in batches by [`State::pump_naming`].
    pub fn apply_names(&mut self) {
        if self.names.is_empty() || self.catalog.is_empty() {
            return;
        }
        let keys: Vec<(u32, u64)> = self
            .catalog
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| !entry.name.resolved())
            // A bank entry keeps the container's id in its name, so both kinds
            // of sound are looked up. A slot has no id to look up at all.
            .filter_map(|(index, entry)| match (entry.source, &entry.name) {
                (crate::hm::catalog::Source::Stream { key }, _) => Some((index as u32, key)),
                (_, Name::Key(key)) => Some((index as u32, *key)),
                _ => None,
            })
            .collect();
        if keys.is_empty() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let names = self.names.clone();
        let asked = keys.len();
        std::thread::spawn(move || match_names(names, keys, tx));
        self.naming = Some(Naming {
            rx,
            asked,
            found: 0,
            done: false,
        });
        self.status = format!("matching {} keys", ui::widgets::tally(asked));
    }

    /// Put a batch of matched names on, as it arrives.
    fn pump_naming(&mut self) {
        let Some(naming) = self.naming.as_mut() else {
            return;
        };
        let mut applied = 0usize;
        loop {
            match naming.rx.try_recv() {
                Ok(batch) => {
                    if batch.is_empty() {
                        naming.done = true;
                        continue;
                    }
                    for (index, found) in batch {
                        let Some(entry) = self.catalog.entries.get_mut(index as usize) else {
                            continue;
                        };
                        let name = Name::Resolved(found);
                        entry.category = scan::classify(
                            &name,
                            entry.language.as_ref(),
                            entry.channels,
                            entry.seconds(),
                        );
                        entry.rename(name);
                        applied += 1;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(_) => {
                    naming.done = true;
                    break;
                }
            }
        }
        let naming = self.naming.as_mut().expect("naming");
        naming.found += applied;
        if applied > 0 {
            self.dirty = true;
        }
        if naming.done {
            let found = naming.found;
            self.naming = None;
            self.status = match found {
                0 => "no names matched these sounds".to_string(),
                found => format!("{} names matched", ui::widgets::tally(found)),
            };
            // Keep them: a matched name in the cache is a name the next run
            // does not have to look up again.
            if found > 0 {
                self.save_cache();
            }
            self.dirty = true;
        }
    }

    pub fn print(&self) -> Option<&Fingerprint> {
        let title = self.title?;
        self.detected.iter().find(|print| print.title == title)
    }

    /// Write what is in the catalogue to its cache file.
    ///
    /// A scan caches itself as it runs, from the thread that read it, so this
    /// is only the way back from a cache that could not be written the first
    /// time. The rows are built here and the json and the write happen on a
    /// worker, because both are far too slow to do between frames.
    pub fn save_cache(&mut self) {
        let (Some(id), false) = (self.title, self.catalog.is_empty()) else {
            return;
        };
        let want = space::cache_size(self.catalog.len());
        if let Some(room) = space::shortfall(&storage::cache_dir(), want) {
            self.blocked = Some(Blocked {
                what: "cache this scan".into(),
                room,
                again: Again::Cache,
            });
            self.status = "not enough room to cache the scan".into();
            return;
        }
        let cache = storage::Cached {
            game: id.key().to_string(),
            depth: self.depth.label().to_string(),
            at: storage::stamp(),
            packages: self.mounted.packages.clone(),
            keys: self.mounted.keys,
            // Re-saving from the window has no mount to stat, so the file
            // stamps a refresh compares against come back empty and the next
            // refresh simply rescans.
            files: Vec::new(),
            sounds: self
                .catalog
                .entries
                .iter()
                .map(|entry| storage::CachedSound {
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
                    bank: matches!(entry.source, crate::hm::catalog::Source::Bank { .. }),
                    slot: matches!(entry.name, Name::Slot(_)),
                })
                .collect(),
        };
        self.blocked = None;
        self.status = "caching the scan".into();
        std::thread::spawn(move || {
            let _ = storage::save_cache(&cache);
        });
    }

    /// Bring back the deepest cached scan of this game.
    ///
    /// Reading and parsing the file is a worker's job: the biggest of them is
    /// fifty megabytes of json, and parsing that on the ui thread is a frozen
    /// window for most of a second. What comes back is turned into entries a
    /// slice at a time, so the list fills in as it arrives.
    pub fn load_cache(&mut self) {
        let Some(id) = self.title else {
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let key = id.key().to_string();
        std::thread::spawn(move || {
            let found = [Depth::Full, Depth::Deep, Depth::Quick]
                .into_iter()
                .find_map(|depth| storage::load_cache(&key, depth.label()).map(|c| (depth, c)));
            let _ = tx.send(found.map(|(depth, cache)| {
                Box::new(storage::Cached {
                    depth: depth.label().to_string(),
                    ..cache
                })
            }));
        });
        self.loading = Some(Loading {
            title: id,
            depth: self.depth,
            rx,
            cache: None,
            at: 0,
        });
        self.status = "reading the cached scan".into();
    }

    /// Turn the next slice of a cached scan into entries.
    fn pump_loading(&mut self) {
        let Some(loading) = self.loading.as_mut() else {
            return;
        };
        if loading.cache.is_none() {
            match loading.rx.try_recv() {
                Ok(Some(cache)) => {
                    loading.depth = scan::depth_of(&cache.depth);
                    loading.cache = Some(cache);
                }
                Ok(None) => {
                    // Nothing cached for this game: an empty list, not an error.
                    self.loading = None;
                    self.status = "no cached scan yet: press scan".into();
                    return;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(_) => {
                    self.loading = None;
                    return;
                }
            }
        }

        let Some(loading) = self.loading.take() else {
            return;
        };
        let Loading {
            title,
            depth,
            rx,
            cache,
            at,
        } = loading;
        let Some(cache) = cache else {
            return;
        };
        if Some(title) != self.title {
            // The user moved on while this was being read.
            return;
        }

        if at == 0 {
            self.catalog.clear();
            self.catalog.title = Some(title);
            self.catalog.scanned_at = Some(cache.at.clone());
            self.mounted.packages = cache.packages.clone();
            self.mounted.keys = cache.keys;
            self.depth = depth;
        }

        let end = (at + SLICE).min(cache.sounds.len());
        for index in at..end {
            let entry = self.entry_of(&cache.sounds[index]);
            self.catalog.push(entry);
        }
        self.dirty = true;

        if end < cache.sounds.len() {
            self.status = format!(
                "{} of {} cached sounds",
                ui::widgets::tally(end),
                ui::widgets::tally(cache.sounds.len())
            );
            self.loading = Some(Loading {
                title,
                depth,
                rx,
                cache: Some(cache),
                at: end,
            });
            return;
        }

        self.status = format!(
            "{} sounds from the {} scan of {}",
            ui::widgets::tally(self.catalog.len()),
            depth.label(),
            cache.at
        );
        self.refilter();

        // The catalogue is back, but nothing is mounted: do that in the
        // background so sounds can be played and extracted.
        if self.mount.is_none()
            && !self.jobs.iter().any(|job| job.title == title)
            && let (Some(root), Some(reader)) = (self.root.clone(), title_for(title))
        {
            self.jobs.push(scan::mount_only(reader, root));
        }
    }

    /// One cached row, back as an entry.
    /// What the container a sound came out of is called.
    fn package_name(&self, package: u32) -> &str {
        self.mounted
            .packages
            .get(package as usize)
            .map(|name| name.as_str())
            .unwrap_or("unknown")
    }

    fn entry_of(&self, sound: &storage::CachedSound) -> Entry {
        // A name the container carried is already in the cache; one that came
        // from a name list is looked up again, in case the list has changed
        // since the scan.
        let resolved = sound.name.clone().or_else(|| {
            self.names
                .lookup(sound.key)
                .map(|found| found.to_string())
        });
        let name = match (resolved, sound.slot) {
            (Some(found), _) => Name::Resolved(found),
            (None, false) => Name::Key(sound.key),
            // Where it sits, the same as the scan showed it.
            (None, true) => Name::Slot(format!(
                "{}#{:05}",
                self.package_name(sound.package),
                sound.index
            )),
        };
        let category = match name.resolved() {
            true => crate::hm::catalog::category::of(&name.text()),
            false => crate::hm::catalog::category::parse(&sound.category),
        };
        let package = crate::hm::pack::PackageId(sound.package);
        Entry {
            id: SoundId(self.catalog.entries.len() as u32),
            sub: crate::hm::catalog::sub_for(&name),
            source: match sound.bank {
                true => crate::hm::catalog::Source::Bank {
                    package,
                    index: sound.index,
                },
                false => crate::hm::catalog::Source::Stream { key: sound.key },
            },
            lower: name.text().to_ascii_lowercase(),
            name,
            package,
            index: sound.index,
            codec: match sound.codec.is_empty() {
                true => crate::hm::sound::Codec::Opus,
                false => crate::hm::sound::codec_of(&sound.codec),
            },
            rate: match sound.rate {
                0 => crate::hm::sound::opus::RATE,
                rate => rate,
            },
            channels: sound.channels,
            frames: sound.frames,
            bytes: sound.bytes,
            language: sound.language.clone(),
            category,
            facets: crate::hm::catalog::Facets::default(),
            favorite: self.favorites.contains(&sound.key),
            tags: self.tags.get(&sound.key).cloned().unwrap_or_default(),
        }
    }

    pub fn forget_cache(&mut self) {
        if let Some(id) = self.title {
            self.unshelf(id);
            for depth in scan::DEPTHS {
                storage::drop_cache(id.key(), depth.label());
            }
            self.status = "cached scans forgotten".into();
        }
    }

    /// Start a scan of the game that is open.
    ///
    /// A scan of another game already running is left alone: it is writing its
    /// own cache and will be there when its tab is opened.
    pub fn start_scan(&mut self) {
        let (Some(root), Some(id)) = (self.root.clone(), self.title) else {
            self.error = Some("no game folder".into());
            return;
        };
        let Some(title) = title_for(id) else {
            self.error = Some("no reader for this title".into());
            return;
        };
        if self.jobs.iter().any(|job| job.title == id && job.kind == scan::Kind::Scan) {
            self.status = "that game is already being scanned".into();
            return;
        }
        // A mount started for the cached catalogue is about to be redone by the
        // scan itself, so it can go, and whatever was kept of this game is
        // about to be out of date.
        self.drop_jobs(id);
        self.unshelf(id);
        self.loading = None;
        self.catalog.clear();
        self.catalog.title = Some(id);
        self.filtered.clear();
        self.cursor = None;
        self.selection.clear();
        self.mount = None;
        self.error = None;
        self.work.insert(
            id,
            Work {
                done: 0,
                total: 0,
                what: "opening".into(),
                depth: self.depth,
                kind: scan::Kind::Scan,
                found: 0,
            },
        );
        self.status = format!("{} scan of {} started", self.depth.label(), id.abbr());
        self.jobs.push(scan::start(title, root, self.depth));
    }

    /// Check the install against the cached scan, and rescan only if it moved.
    ///
    /// A game that has been patched has containers of a different size or date;
    /// one that has not is left alone. Either way the containers are opened
    /// again, so the sounds are playable when it finishes. It all happens on a
    /// worker: the window keeps drawing.
    pub fn refresh(&mut self) {
        let (Some(root), Some(id)) = (self.root.clone(), self.title) else {
            self.error = Some("no game folder".into());
            return;
        };
        let Some(title) = title_for(id) else {
            return;
        };
        if self.jobs.iter().any(|job| job.title == id) {
            self.status = "already working on that game".into();
            return;
        }
        let key = id.key().to_string();
        let depth = self.depth;
        let expected = storage::load_cache(&key, depth.label())
            .map(|cache| cache.files)
            .unwrap_or_default();
        self.work.insert(
            id,
            Work {
                done: 0,
                total: 0,
                what: "checking the files".into(),
                depth,
                kind: scan::Kind::Mount,
                found: 0,
            },
        );
        self.jobs.push(scan::check(title, root, expected));
    }

    /// Stop the scan of the game that is open. Other games carry on.
    pub fn stop_scan(&mut self) {
        let Some(id) = self.title else { return };
        for job in self.jobs.iter().filter(|job| job.title == id) {
            job.stop();
        }
        self.drop_jobs(id);
        self.work.remove(&id);
        self.status = "scan stopped".into();
    }

    /// Stop every scan, whichever game it is for.
    pub fn stop_all(&mut self) {
        for job in &self.jobs {
            job.stop();
        }
        self.jobs.clear();
        self.work.clear();
    }

    fn drop_jobs(&mut self, id: TitleId) {
        self.jobs.retain(|job| {
            if job.title == id {
                job.stop();
                return false;
            }
            true
        });
    }

    pub fn scanning(&self) -> bool {
        !self.jobs.is_empty()
    }

    /// Whether the game on screen is being worked on right now, which is what
    /// the scan and stop buttons answer to.
    pub fn busy_here(&self) -> bool {
        let here = self.title;
        self.loading.is_some()
            || self.looking.is_some()
            || self
                .jobs
                .iter()
                .any(|job| Some(job.title) == here && job.kind == scan::Kind::Scan)
    }

    /// The progress of the game on screen, for the bar over the list.
    pub fn here_progress(&self) -> Option<(usize, usize)> {
        let id = self.title?;
        let work = self.work.get(&id)?;
        Some((work.done, work.total))
    }

    /// One line saying what harmony is doing, in the words the window uses.
    ///
    /// Several things can be going on at once now: two or three games being
    /// scanned, a cache being read, a folder being fingerprinted. The line says
    /// how many and which, rather than flashing whichever message landed last.
    pub fn activity(&self) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();

        let scans: Vec<&scan::Job> = self
            .jobs
            .iter()
            .filter(|job| job.kind == scan::Kind::Scan)
            .collect();
        if !scans.is_empty() {
            let names: Vec<&str> = scans.iter().map(|job| job.title.abbr()).collect();
            let what = scans
                .iter()
                .filter_map(|job| self.work.get(&job.title))
                .map(|work| work.what.clone())
                .find(|what| !what.is_empty())
                .unwrap_or_else(|| "reading".into());
            let counted: usize = scans
                .iter()
                .filter_map(|job| self.work.get(&job.title))
                .map(|work| work.found)
                .sum();
            let heading = match names.len() {
                1 => format!("scanning {}", names[0]),
                many => format!("scanning {many} games ({})", names.join(", ")),
            };
            let sound_count = match counted {
                0 => String::new(),
                found => format!(" Â· {} sounds", ui::widgets::tally(found)),
            };
            parts.push(format!("{heading} Â· {what}{sound_count}"));
        }

        if self.jobs.iter().any(|job| job.kind == scan::Kind::Mount) {
            parts.push("opening containers".into());
        }
        if let Some(looking) = self.looking.as_ref() {
            parts.push(format!(
                "searching {}",
                crate::hm::discord::clip(
                    &looking
                        .root
                        .file_name()
                        .map(|n| n.to_string_lossy().to_ascii_lowercase())
                        .unwrap_or_else(|| "that folder".into()),
                    28
                )
            ));
        }
        if let Some(loading) = self.loading.as_ref() {
            let total = loading
                .cache
                .as_ref()
                .map(|cache| cache.sounds.len())
                .unwrap_or(0);
            parts.push(match total {
                0 => format!("reading the cached scan of {}", loading.title.abbr()),
                total => format!(
                    "loading {} Â· {} of {}",
                    loading.title.abbr(),
                    ui::widgets::tally(loading.at),
                    ui::widgets::tally(total)
                ),
            });
        }
        {
            let queue = self.queue.progress.lock().unwrap();
            if queue.running {
                parts.push(format!(
                    "extracting {} of {}",
                    ui::widgets::tally(queue.done + queue.failed),
                    ui::widgets::tally(queue.total)
                ));
            }
        }
        if self.dragging_out.is_some() {
            parts.push("getting a carry ready".into());
        }

        match parts.is_empty() {
            true => None,
            false => Some(parts.join("  \u{b7}  ")),
        }
    }

    /// Name lists that finished loading on their worker.
    fn pump_names(&mut self) {
        let Some(rx) = self.names_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(names) => {
                self.names = std::sync::Arc::new(names);
                self.names_rx = None;
                // Anything already listed gets its name now, on a worker.
                self.apply_names();
                self.dirty = true;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(_) => self.names_rx = None,
        }
    }


    /// Take a reading of the disks harmony writes to, now and then.
    fn read_disks(&mut self) {
        if self.disk.checked.elapsed() < std::time::Duration::from_secs(5) {
            return;
        }
        let cache = storage::cache_dir();
        let scratch = crate::hm::window::dragout::scratch_dir();
        self.disk = Disk {
            checked: std::time::Instant::now(),
            cache_free: space::free(&cache),
            cache_held: storage::cache_bytes(),
            scratch_free: space::free(&scratch),
            export_free: self.output.as_ref().and_then(|path| space::free(path)),
        };
    }

    /// Drain every running job. Entries are only kept for the game on screen:

    /// a scan of another game is writing its own cache, and its catalogue comes
    /// back from that when its tab is opened.
    pub fn pump(&mut self) {
        self.pump_looking();
        self.pump_loading();
        self.pump_names();
        self.pump_naming();
        self.pump_sounding();
        self.pump_presence();
        self.read_disks();

        // The veil comes down once there is nothing left to wait for.
        if self.booting && self.looking.is_none() && self.loading.is_none() && self.names_rx.is_none()
        {
            self.booting = false;
        }

        let here = self.title;
        let mut finished: Vec<TitleId> = Vec::new();
        let mut reload = false;
        let mut rescan: Option<TitleId> = None;
        // Shared with the matching worker, so a clone of the handle is enough.
        let names = self.names.clone();

        for job in &self.jobs {
            let mine = Some(job.title) == here;
            let work = self.work.entry(job.title).or_insert_with(|| Work {
                done: 0,
                total: 0,
                what: String::new(),
                depth: job.depth,
                kind: job.kind,
                found: 0,
            });
            // Only the kapi walk counts through these; the others report by
            // message, and reading a zero here is what made the bar flick back
            // to nothing between frames.
            let counted = job.done.load(Ordering::Relaxed);
            let of = job.total.load(Ordering::Relaxed);
            if of > 0 {
                work.done = counted;
                work.total = of;
            }

            // A batch at a time, and never for longer than a frame's worth:
            // a scan can produce entries faster than the window can draw them,
            // and the rest are still there next frame.
            let until = std::time::Instant::now() + std::time::Duration::from_millis(8);
            while let Ok(message) = job.rx.try_recv() {
                match message {
                    scan::Msg::Progress { done, total, what } => {
                        if total > 0 {
                            work.done = done;
                            work.total = total;
                        }
                        work.what = what;
                    }
                    scan::Msg::Found(batch) => {
                        work.found += batch.len();
                        if !mine {
                            continue;
                        }
                        for mut entry in batch {
                            entry.id = SoundId(self.catalog.entries.len() as u32);
                            if let crate::hm::catalog::Source::Stream { key } = entry.source {
                                entry.favorite = self.favorites.contains(&key);
                                if let Some(tags) = self.tags.get(&key) {
                                    entry.tags = tags.clone();
                                }
                                if let Some(name) =
                                    names.lookup(key)
                                {
                                    let name = Name::Resolved(name.to_string());
                                    entry.category = scan::classify(
                                        &name,
                                        entry.language.as_ref(),
                                        entry.channels,
                                        entry.seconds(),
                                    );
                                    entry.rename(name);
                                }
                            }
                            self.catalog.push(entry);
                        }
                        self.dirty = true;
                    }
                    scan::Msg::Ready(mount) => {
                        if mine {
                            self.mounted.packages =
                                mount.store.info().iter().map(|i| i.name.clone()).collect();
                            self.mounted.keys = mount.store.keys();
                            self.mounted.container = mount.store.label().to_string();
                            self.mounted.build = mount.build.clone();
                            self.mount = Some(Arc::new(Mutex::new(*mount)));
                            self.catalog.scanned_at = Some(storage::stamp());
                            self.dirty = true;
                        }
                        work.what = match job.kind {
                            scan::Kind::Mount => String::new(),
                            scan::Kind::Scan => "caching".into(),
                        };
                        if job.kind == scan::Kind::Mount {
                            finished.push(job.title);
                        }
                    }
                    scan::Msg::Checked { changed, total } => {
                        work.what = match changed {
                            0 => "nothing changed".into(),
                            n => format!("{n} containers changed"),
                        };
                        if mine {
                            self.status = match changed {
                                0 => format!(
                                    "{} containers checked, nothing changed",
                                    ui::widgets::tally(total)
                                ),
                                n => format!(
                                    "{} of {} containers changed: rescanning",
                                    ui::widgets::tally(n),
                                    ui::widgets::tally(total)
                                ),
                            };
                        }
                        if changed > 0 {
                            rescan = Some(job.title);
                        }
                    }
                    scan::Msg::Cached { sounds } => {
                        work.found = sounds;
                        if mine {
                            self.status =
                                format!("{} sounds cached", ui::widgets::tally(sounds));
                            // Rows that arrived while this game's tab was not
                            // the one on screen were not kept. The cache has
                            // all of them, and it has just been written.
                            if self.catalog.len() < sounds {
                                reload = true;
                            }
                        }
                        finished.push(job.title);
                    }
                    scan::Msg::NoRoom(room) => {
                        self.blocked = Some(Blocked {
                            what: "cache this scan".into(),
                            room,
                            again: Again::Cache,
                        });
                        finished.push(job.title);
                    }
                    scan::Msg::CacheFailed(error) => {
                        self.error = Some(format!("the scan could not be cached: {error}"));
                        finished.push(job.title);
                    }
                    scan::Msg::Failed(error) => {
                        if mine {
                            self.error = Some(error);
                        }
                        finished.push(job.title);
                    }
                }
                if std::time::Instant::now() > until {
                    break;
                }
            }
        }

        for id in finished {
            self.jobs.retain(|job| job.title != id);
            if let Some(work) = self.work.remove(&id) {
                self.settings.scans.insert(
                    id.key().to_string(),
                    storage::Scan {
                        at: storage::stamp(),
                        sounds: work.found,
                        packages: self.mounted.packages.len(),
                    },
                );
            }
            storage::save(&self.settings);
        }
        if reload {
            self.load_cache();
        }
        // A refresh that found the install moved goes straight on to a scan.
        if let Some(id) = rescan {
            self.drop_jobs(id);
            if self.title == Some(id) {
                self.start_scan();
            }
        }
    }

    /// Rebuild the filtered list, but not on every frame of a scan.
    ///
    /// Filtering fifty thousand entries and regrouping them takes longer than a
    /// frame, and a running scan makes the catalogue dirty several times a
    /// frame. Four times a second is enough to look live.
    pub fn settle(&mut self) {
        if !self.dirty {
            return;
        }
        let quiet = self.jobs.is_empty() && self.loading.is_none();
        let waited = self.filtered_at.elapsed() >= std::time::Duration::from_millis(250);
        if quiet || waited {
            self.refilter();
        }
    }

    pub fn refilter(&mut self) {
        self.dirty = false;
        self.filtered_at = std::time::Instant::now();
        self.query = parse::parse(&self.query_text);
        let game = self
            .title
            .map(|t| t.key().to_string())
            .unwrap_or_else(|| "unknown".into());
        let context = eval::Context {
            game: &game,
            packages: &self.mounted.packages,
        };
        self.filtered = self
            .catalog
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                if self.view == View::Favorites && !entry.favorite {
                    return false;
                }
                self.query.is_empty() || eval::matches(entry, &self.query, &context)
            })
            .map(|(index, _)| index)
            .collect();

        if let Some(name) = self.collection_pick.clone() {
            let set = self.collections.get(&name).cloned().unwrap_or_default();
            self.filtered
                .retain(|index| set.contains(&self.catalog.entries[*index].key()));
        }

        if let Some(pick) = self.tree_pick.clone() {
            let kept: Vec<usize> = self
                .filtered
                .iter()
                .copied()
                .filter(|index| {
                    let entry = &self.catalog.entries[*index];
                    self.branch_label(entry) == pick
                })
                .collect();
            self.filtered = kept;
        }

        self.tree = match self.view {
            View::TreePlus => {
                group::build_plus(&self.catalog, &self.filtered, &self.mounted.packages)
            }
            _ => group::build(
                &self.catalog,
                &self.filtered,
                self.group_by,
                &self.mounted.packages,
            ),
        };
    }

    pub fn branch_label(&self, entry: &Entry) -> String {
        match self.group_by {
            GroupBy::Category => entry.category.label().to_string(),
            GroupBy::Mode => {
                let package = self
                    .mounted
                    .packages
                    .get(entry.package.0 as usize)
                    .map(|name| name.as_str())
                    .unwrap_or("");
                crate::hm::catalog::mode::of(&entry.display(), package)
                    .label()
                    .to_string()
            }
            GroupBy::Package => self
                .mounted
                .packages
                .get(entry.package.0 as usize)
                .cloned()
                .unwrap_or_else(|| "unknown".into()),
            GroupBy::Language => entry.language.clone().unwrap_or_else(|| "shared".into()),
            GroupBy::Codec => entry.codec.label().to_string(),
            GroupBy::Path => group::path_of(entry)
                .first()
                .cloned()
                .unwrap_or_else(|| "unnamed".into()),
        }
    }

    pub fn raw_of(&self, index: usize) -> Option<Vec<u8>> {
        let entry = self.catalog.entries.get(index)?;
        let mount = self.mount.as_ref()?;
        let mut lock = mount.lock().ok()?;
        let Mount { store, oodle, .. } = &mut *lock;
        store
            .read(entry.package, entry.index as usize, oodle.as_ref())
            .ok()
    }

    /// Rank everything shown against the selected sound once, when the
    /// selection changes: doing it per frame over a catalogue this size is
    /// pure waste.
    fn rank_similar(&mut self, index: usize) {
        self.similar.clear();
        let Some(source) = self.catalog.entries.get(index) else {
            return;
        };
        let name = source.lower.clone();
        let left = crate::hm::analysis::similarity::Print {
            seconds: source.seconds(),
            channels: source.channels,
            ..Default::default()
        };
        let mut ranked: Vec<(f32, usize)> = self
            .filtered
            .iter()
            .copied()
            .filter(|other| *other != index)
            .map(|other| {
                let entry = &self.catalog.entries[other];
                let right = crate::hm::analysis::similarity::Print {
                    seconds: entry.seconds(),
                    channels: entry.channels,
                    ..Default::default()
                };
                (
                    crate::hm::analysis::similarity::distance(
                        &left,
                        &right,
                        &name,
                        entry.text(),
                    ),
                    other,
                )
            })
            .collect();
        ranked.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        ranked.truncate(8);
        self.similar = ranked;
    }

    pub fn select(&mut self, index: usize, play: bool) {
        self.cursor = Some(index);
        self.rank_similar(index);
        let Some(entry) = self.catalog.entries.get(index).cloned() else {
            self.loaded = None;
            return;
        };
        // The old sound goes now, so the panes do not show one sound's waveform
        // under another's name.
        self.loaded = None;
        if let Some(transport) = self.transport.as_mut() {
            transport.stop();
        }
        let id = entry.id;
        let name = entry.display();
        self.recent.retain(|other| *other != id);
        self.recent.insert(0, id);
        self.recent.truncate(200);
        self.start_sounding(index, entry, play, false);
        self.status = format!("reading {name}");
        self.push_presence();
    }

    /// Read, decode and measure a sound on a worker.
    ///
    /// `against` puts the result in the b slot of the compare pane instead of
    /// the player.
    fn start_sounding(&mut self, index: usize, entry: Entry, play: bool, against: bool) {
        let Some(mount) = self.mount.clone() else {
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let id = entry.id;
        let name = entry.display();
        let area = self.area_of(&entry);
        std::thread::spawn(move || {
            let _ = tx.send(measure(&mount, &entry).map(Box::new));
        });
        self.sounding = Some(Sounding {
            index,
            id,
            name,
            area,
            play,
            against,
            rx,
        });
    }

    /// Where a sound is being read from, as a person would say it.
    ///
    /// While a game is still being opened that is whichever container the
    /// mount is on, because that is what the read is waiting behind. Once it
    /// is open it is simply the container the sound lives in.
    fn area_of(&self, entry: &Entry) -> String {
        let working = self
            .title
            .and_then(|id| self.work.get(&id))
            .map(|work| work.what.clone())
            .filter(|what| !what.is_empty());
        match working {
            Some(what) => what,
            None => self.package_name(entry.package.0).to_string(),
        }
    }

    /// A sound that finished being read and measured.
    fn pump_sounding(&mut self) {
        let Some(sounding) = self.sounding.as_ref() else {
            return;
        };
        let Ok(result) = sounding.rx.try_recv() else {
            return;
        };
        let Some(sounding) = self.sounding.take() else {
            return;
        };
        // The selection may have moved on while this was being read.
        if !sounding.against && self.cursor != Some(sounding.index) {
            return;
        }
        let Some(loaded) = result else {
            self.status = format!("{} has no audio harmony can decode", sounding.name);
            return;
        };
        if sounding.against {
            self.against = Some(*loaded);
            return;
        }
        self.loaded = Some(*loaded);
        self.status = sounding.name.clone();
        if sounding.play
            && let (Some(transport), Some(loaded)) =
                (self.transport.as_mut(), self.loaded.as_ref())
        {
            transport.play(&loaded.samples);
        }
        self.push_presence();
    }

    /// Collections are named sets of sounds the user put together by hand.
    /// They live in the settings file keyed by stream key, so they outlast a
    /// rescan and even a game patch.
    pub fn collect(&mut self, name: &str, indices: &[usize]) {
        if name.trim().is_empty() {
            return;
        }
        let keys: Vec<u64> = indices
            .iter()
            .filter_map(|index| self.catalog.entries.get(*index))
            .map(|entry| entry.key())
            .collect();
        if keys.is_empty() {
            return;
        }
        let set = self.collections.entry(name.to_string()).or_default();
        for key in keys {
            set.insert(key);
        }
        self.save_collections();
        self.status = format!("{} holds {}", name, ui::widgets::tally(self.collections[name].len()));
    }

    pub fn drop_collection(&mut self, name: &str) {
        self.collections.remove(name);
        if self.collection_pick.as_deref() == Some(name) {
            self.collection_pick = None;
        }
        self.save_collections();
        self.refilter();
    }

    fn save_collections(&mut self) {
        self.settings.collections = self
            .collections
            .iter()
            .map(|(name, keys)| {
                (
                    name.clone(),
                    keys.iter().map(|key| format!("{key:016x}")).collect(),
                )
            })
            .collect();
        storage::save(&self.settings);
    }

    /// Put a tag on a sound. Tags live with the key, so they survive a rescan.
    pub fn tag(&mut self, index: usize, tag: &str) {
        let Some(entry) = self.catalog.entries.get_mut(index) else {
            return;
        };
        if entry.tags.iter().any(|other| other == tag) {
            entry.tags.retain(|other| other != tag);
        } else {
            entry.tags.push(tag.to_string());
        }
        let key = entry.key();
        {
            let tags = entry.tags.clone();
            if tags.is_empty() {
                self.tags.remove(&key);
                self.settings.tags.remove(&format!("{key:016x}"));
            } else {
                self.tags.insert(key, tags.clone());
                self.settings.tags.insert(format!("{key:016x}"), tags);
            }
            storage::save(&self.settings);
        }
    }

    /// Keep what is loaded as the "b" side, so the next selection can be held
    /// against it.
    pub fn hold_against(&mut self) {
        // The sound is already measured: holding it costs nothing but a move.
        if let Some(loaded) = self.loaded.take() {
            self.against = Some(loaded);
            self.status = "held for comparison".into();
            if let Some(index) = self.cursor
                && let Some(entry) = self.catalog.entries.get(index).cloned()
            {
                // And the selected sound is read again, so the a side is
                // still there to play.
                self.start_sounding(index, entry, false, false);
            }
        }
    }

    pub fn toggle_favorite(&mut self, index: usize) {
        let Some(entry) = self.catalog.entries.get_mut(index) else {
            return;
        };
        entry.favorite = !entry.favorite;
        let key = entry.key();
        {
            if entry.favorite {
                self.favorites.insert(key);
            } else {
                self.favorites.remove(&key);
            }
            self.settings.favorites = self.favorites.iter().map(|k| format!("{k:016x}")).collect();
            storage::save(&self.settings);
        }
    }

    /// Start carrying sounds out of the window.
    ///
    /// The files do not exist yet: they are written into a scratch folder while
    /// the card stands over the window, and the drag itself begins once they
    /// land. `row` is the row the drag started on, which joins the selection if
    /// it was not already part of it.
    pub fn drag_out(&mut self, row: Option<usize>) {
        if self.dragging_out.is_some() {
            return;
        }
        let Some(mount) = self.mount.clone() else {
            self.error = Some("nothing mounted".into());
            return;
        };

        let mut wanted: Vec<usize> = match row {
            Some(index) if self.selection.contains(&index) => {
                self.selection.iter().copied().collect()
            }
            Some(index) => vec![index],
            None if !self.selection.is_empty() => self.selection.iter().copied().collect(),
            None => self.cursor.into_iter().collect(),
        };
        wanted.sort_unstable();
        wanted.dedup();

        let chosen: Vec<Entry> = wanted
            .iter()
            .filter_map(|index| self.catalog.entries.get(*index).cloned())
            .collect();
        if chosen.is_empty() {
            self.status = "nothing to drag out".into();
            return;
        }

        let name = match chosen.len() {
            1 => chosen[0].display(),
            many => format!("{many} sounds"),
        };
        let into = crate::hm::window::dragout::scratch_dir();
        let want = crate::hm::export::estimate(&chosen, self.options.format);
        if let Some(room) = space::shortfall(&into, want) {
            self.blocked = Some(Blocked {
                what: format!("carry {name} out"),
                room,
                again: Again::DragOut,
            });
            self.status = "not enough room in the scratch folder".into();
            return;
        }
        let making = crate::hm::export::drag::Making::start(
            mount,
            chosen,
            self.mounted.packages.clone(),
            self.options.clone(),
            into,
        );
        self.status = format!("getting {name} ready");
        self.dragging_out = Some(DragOut {
            paths: Vec::new(),
            name,
            temp: true,
            painted: false,
            making: Some(making),
        });
    }

    /// The second half, run at the top of the frame after the card reached the
    /// screen. It blocks here, on the ui thread, until the pointer comes up:
    /// ole reads the button state off the calling thread, so a drag begun
    /// anywhere else ends the instant it starts.
    pub fn finish_drag_out(&mut self) {
        use crate::hm::window::dragout::{self, DragOutcome};

        if !self
            .dragging_out
            .as_ref()
            .is_some_and(|carried| carried.painted)
        {
            return;
        }
        if let Some(carried) = self.dragging_out.as_mut()
            && let Some(making) = carried.making.as_ref()
        {
            match making.poll() {
                None => return,
                Some(Ok(paths)) => {
                    carried.paths = paths;
                    carried.making = None;
                    // The card said "getting ready" a frame ago; let it say
                    // "carrying" before the thread is handed to ole.
                    carried.painted = false;
                    return;
                }
                Some(Err(error)) => {
                    let name = carried.name.clone();
                    self.dragging_out = None;
                    self.status = format!("{name} could not be written: {error}");
                    self.error = Some(error);
                    return;
                }
            }
        }

        let Some(carried) = self.dragging_out.take() else {
            return;
        };
        let DragOut {
            paths, name, temp, ..
        } = carried;
        let outcome = dragout::drag_files(&paths);
        // A drop that went nowhere leaves files in the scratch folder nobody
        // asked for. One that landed is the target's business now.
        if temp && outcome != DragOutcome::Dropped {
            for path in &paths {
                let _ = std::fs::remove_file(path);
            }
        }
        self.status = match outcome {
            DragOutcome::Dropped => format!("dropped {name}"),
            DragOutcome::Cancelled => "drag cancelled, nothing left harmony".into(),
            DragOutcome::Unsupported => "this window cannot start a drag".into(),
        };
    }

    pub fn extract(&mut self, all: bool) {
        let Some(mount) = self.mount.clone() else {
            self.error = Some("nothing mounted".into());
            return;
        };
        let root = match self.output.clone() {
            Some(root) => root,
            None => match rfd::FileDialog::new().pick_folder() {
                Some(folder) => {
                    self.output = Some(folder.clone());
                    self.settings.output = Some(folder.clone());
                    storage::save(&self.settings);
                    folder
                }
                None => return,
            },
        };
        let chosen: Vec<Entry> = if all || self.selection.is_empty() {
            self.filtered
                .iter()
                .map(|index| self.catalog.entries[*index].clone())
                .collect()
        } else {
            self.selection
                .iter()
                .map(|index| self.catalog.entries[*index].clone())
                .collect()
        };
        if chosen.is_empty() {
            self.error = Some("nothing selected".into());
            return;
        }
        // What this will take on disk, measured before a single file is
        // written, so a run that cannot finish never starts.
        let want = crate::hm::export::estimate(&chosen, self.options.format);
        if let Some(room) = space::shortfall(&root, want) {
            self.blocked = Some(Blocked {
                what: format!("extract {} sounds", ui::widgets::tally(chosen.len())),
                room,
                again: Again::Extract { all },
            });
            self.status = "not enough room to extract".into();
            return;
        }
        self.blocked = None;
        let game = self
            .title
            .map(|t| t.key().to_string())
            .unwrap_or_else(|| "unknown".into());
        self.show_queue = true;
        self.export_since = Some(crate::hm::discord::now_ms());
        self.queue.start(
            mount,
            chosen,
            self.mounted.packages.clone(),
            game,
            root,
            self.options.clone(),
        );
    }

    /// What a whole library is written into: `[t7] black ops iii - 1.0.0.2`.
    ///
    /// The id first, because that is what harmony keys everything else on and
    /// what sorts a folder of them into something readable; the name for the
    /// people who do not think in engine names; the build last, so two versions
    /// of the same game pulled a year apart do not land on top of each other.
    /// A game whose build harmony could not read is written without one rather
    /// than with a guess.
    pub fn library_folder(&self, build: Option<&str>) -> String {
        let id = self.title.map(|t| t.key()).unwrap_or("unknown");
        let name = self.title.map(title_label).unwrap_or("unknown");
        let stem = match build {
            Some(build) if !build.trim().is_empty() => {
                format!("[{id}] {name} - {}", build.trim())
            }
            _ => format!("[{id}] {name}"),
        };
        crate::hm::export::layout::safe(&stem)
    }

    /// Extract everything this game has, filter or no filter.
    ///
    /// Not the same button as `extract shown` with nothing typed in the search
    /// box: this one ignores the view entirely, makes a folder of its own named
    /// after the game and the build it came from, and runs wide — every worker
    /// the machine can spare — because a library is a hundred thousand sounds
    /// and an afternoon, not a handful.
    pub fn extract_library(&mut self) {
        let Some(mount) = self.mount.clone() else {
            self.error = Some("nothing mounted".into());
            return;
        };
        if self.catalog.is_empty() {
            self.error = Some("nothing to extract".into());
            return;
        }
        let into = match self.output.clone() {
            Some(root) => root,
            None => match rfd::FileDialog::new().pick_folder() {
                Some(folder) => {
                    self.output = Some(folder.clone());
                    self.settings.output = Some(folder.clone());
                    storage::save(&self.settings);
                    folder
                }
                None => return,
            },
        };
        let build = mount.lock().ok().and_then(|lock| lock.build.clone());
        let root = into.join(self.library_folder(build.as_deref()));

        let chosen: Vec<Entry> = self.catalog.entries.clone();
        let want = crate::hm::export::estimate(&chosen, self.options.format);
        if let Some(room) = space::shortfall(&into, want) {
            self.blocked = Some(Blocked {
                what: format!("extract {} sounds", ui::widgets::tally(chosen.len())),
                room,
                again: Again::Library,
            });
            self.status = "not enough room to extract".into();
            return;
        }
        self.blocked = None;
        if let Err(error) = std::fs::create_dir_all(&root) {
            self.error = Some(error.to_string());
            return;
        }
        let game = self
            .title
            .map(|t| t.key().to_string())
            .unwrap_or_else(|| "unknown".into());
        self.show_queue = true;
        self.export_since = Some(crate::hm::discord::now_ms());
        self.status = format!(
            "extracting {} sounds into {}",
            ui::widgets::tally(chosen.len()),
            self.library_folder(build.as_deref())
        );
        self.queue.start(
            mount,
            chosen,
            self.mounted.packages.clone(),
            game,
            root,
            self.options.clone(),
        );
    }

    /// What the presence line says while an export is running, or nothing when
    /// none is.
    ///
    /// Read off the queue under its lock and let go of straight away: the
    /// workers are writing into the same struct.
    pub fn exporting(&self) -> Option<Running> {
        let progress = self.queue.progress.lock().ok()?;
        if !progress.running || progress.total == 0 {
            return None;
        }
        let through = progress.done + progress.failed;
        let percent = (through as f32 / progress.total as f32 * 100.0).round() as u32;
        Some(Running {
            details: format!(
                "extracting {} of {}",
                crate::hm::discord::compact(through),
                crate::hm::discord::compact(progress.total)
            ),
            what: match self.queue.paused.load(std::sync::atomic::Ordering::Relaxed) {
                true => format!("export paused at {percent}%"),
                false => format!("exporting {percent}%"),
            },
        })
    }

    /// Keep the presence line in step with a running export.
    ///
    /// The line only moves in whole percents, and discord will not take more
    /// than one update every few seconds anyway, so this asks no more often
    /// than the answer can change.
    fn pump_presence(&mut self) {
        let running = self
            .queue
            .progress
            .lock()
            .map(|progress| progress.running)
            .unwrap_or(false);
        if !running && !self.was_exporting {
            return;
        }
        let now = crate::hm::discord::now_ms();
        if running && now - self.presence_at < 2000 {
            return;
        }
        self.presence_at = now;
        // The run that just ended puts the ordinary line back.
        self.was_exporting = running;
        self.push_presence();
    }

    pub fn push_presence(&mut self) {
        if !self.settings.discord {
            self.rpc.clear();
            return;
        }
        let game = self.title.map(presence_name).unwrap_or("no game").to_string();
        // An export is the loudest thing harmony can be doing, and the one
        // worth saying out loud: it runs for hours and the person watching
        // wants to know how far along it is without opening the window.
        if let Some(run) = self.exporting() {
            self.rpc.set(Presence {
                details: crate::hm::discord::clip(&run.details, 60),
                state: crate::hm::discord::clip(&format!("{game} \u{b7} {}", run.what), 60),
                large_text: "harmony".into(),
                since: self.export_since.or(Some(self.started)),
            });
            return;
        }
        // What is being listened to, or how much there is if nothing is.
        let details = match self.cursor.and_then(|i| self.catalog.entries.get(i)) {
            // A hash alone says nothing to anyone reading it, so the container
            // it came out of goes in front: `zmb_tomb.all/_f6a6b431ac13033b`.
            Some(entry) => {
                let shown = match entry.name.resolved() {
                    true => entry.display(),
                    false => format!(
                        "{}/{}",
                        self.package_name(entry.package.0),
                        entry.display()
                    ),
                };
                crate::hm::discord::clip(&shown, 60)
            }
            None if self.catalog.is_empty() => "nothing open".to_string(),
            None => format!("{} sounds", crate::hm::discord::compact(self.catalog.len())),
        };
        // Under it, the game and what harmony is doing to it. Counts are
        // rounded here and nowhere else: a presence line has about forty
        // characters before discord cuts it off.
        let count = crate::hm::discord::compact(self.catalog.len());
        let state = match (self.catalog.is_empty(), self.scanning()) {
            (_, true) => format!("{game} \u{b7} scanning"),
            (false, false) => format!("{game} \u{b7} {count} sounds"),
            (true, false) => game,
        };
        self.rpc.set(Presence {
            details,
            state: crate::hm::discord::clip(&state, 60),
            large_text: "harmony".into(),
            since: Some(self.started),
        });
    }

    /// Save the current extraction options under a name.
    pub fn save_preset(&mut self, name: &str) {
        if name.trim().is_empty() {
            self.error = Some("give the preset a name".into());
            return;
        }
        self.settings.presets.insert(
            name.to_string(),
            storage::Preset {
                format: self.options.format.label().to_string(),
                layout: self.options.layout.label().to_string(),
                preserve_paths: self.options.layout.keeps_paths(),
                normalise_names: self.options.normalise_names,
                skip_duplicates: self.options.skip_duplicates,
                write_manifest: self.options.write_manifest,
            },
        );
        storage::save(&self.settings);
        self.status = format!("preset {name} saved");
    }

    pub fn apply_preset(&mut self, name: &str) {
        let Some(preset) = self.settings.presets.get(name).cloned() else {
            return;
        };
        self.settings.format = preset.format;
        self.settings.layout = preset.layout;
        self.options.format = self.settings.format();
        self.options.layout = self.settings.layout();
        self.options.normalise_names = preset.normalise_names;
        self.options.skip_duplicates = preset.skip_duplicates;
        self.options.write_manifest = preset.write_manifest;
        self.save_options();
        self.status = format!("preset {name} applied");
    }

    pub fn save_options(&mut self) {
        self.settings.format = self.options.format.label().to_string();
        self.settings.layout = self.options.layout.label().to_string();
        self.settings.preserve_paths = self.options.layout.keeps_paths();
        self.settings.normalise_names = self.options.normalise_names;
        self.settings.skip_duplicates = self.options.skip_duplicates;
        self.settings.write_manifest = self.options.write_manifest;
        storage::save(&self.settings);
    }
}

/// The full name of a title, straight from the registry, so a game only has to
/// be named in one place.
pub fn title_label(id: TitleId) -> &'static str {
    title_for(id).map(|title| title.label()).unwrap_or("unknown")
}

/// What a title is called in rich presence.
///
/// The full name where it fits, and the tab's own short name where it does
/// not: `modern warfare remastered` leaves no room for the count beside it,
/// `mwr` does. Twenty characters is what the longest name harmony spells out
/// in full needs, and everything above it reads better as the tab's name.
pub fn presence_name(id: TitleId) -> &'static str {
    const ROOM: usize = 20;
    let label = title_label(id);
    match label.len() <= ROOM {
        true => label,
        false => id.abbr(),
    }
}

pub fn support_of(id: TitleId) -> Support {
    title_for(id).map(|t| t.status()).unwrap_or(Support::None)
}

/// Read name lists into a database, and say how many names went in.
///
/// Two file shapes: `hash,name` pairs, and a plain wordlist, which is hashed
/// every way the engines have used because nothing in the file says which.
/// Both can be enormous, which is why this is a free function: the startup
/// worker runs it on its own thread, with no window in reach.
/// Everything a sound needs before it can be shown: the blob out of its
/// container, decoded, measured, and reduced to a waveform and a spectrogram.
///
/// A free function, because it runs on a worker with no window in reach.
pub fn measure(mount: &Arc<Mutex<Mount>>, entry: &Entry) -> Option<Loaded> {
    let raw = {
        let mut lock = mount.lock().ok()?;
        let Mount { store, oodle, .. } = &mut *lock;
        store
            .read(entry.package, entry.index as usize, oodle.as_ref())
            .ok()?
    };
    let about = decode::sniff(&raw)?;
    let samples = decode::decode(&raw, about).ok()?;
    let stats = analysis::stats(&samples);
    let peaks = crate::hm::analysis::peaks::build(&samples, 600);
    let columns = crate::hm::analysis::peaks::spectrogram(&samples, 240, 48);
    Some(Loaded {
        id: entry.id,
        samples,
        peaks,
        stats,
        columns,
    })
}

/// Matching a catalogue against a name database, off the window's thread.
///
/// A hundred thousand keys against half a million names is not a frame's work,
/// and it is pure lookup: nothing about it needs the window. The keys go out
/// to a worker, the names come back in batches, and the window applies each
/// batch as it arrives so the list fills in rather than freezing and jumping.
pub struct Naming {
    pub rx: std::sync::mpsc::Receiver<Vec<(u32, String)>>,
    pub asked: usize,
    pub found: usize,
    pub done: bool,
}

/// The worker: every key that has no name yet, looked up once.
fn match_names(
    names: std::sync::Arc<NameDb>,
    keys: Vec<(u32, u64)>,
    tx: std::sync::mpsc::Sender<Vec<(u32, String)>>,
) {
    const BATCH: usize = 4096;
    let mut batch: Vec<(u32, String)> = Vec::with_capacity(BATCH);
    for (index, key) in keys {
        if let Some(found) = names.lookup(key) {
            batch.push((index, found.to_string()));
            if batch.len() >= BATCH && tx.send(std::mem::take(&mut batch)).is_err() {
                return;
            }
        }
    }
    let _ = tx.send(batch);
}

pub fn read_names(names: &mut NameDb, paths: Vec<PathBuf>) -> usize {

    let mut total = 0usize;
    for path in paths {
        let pairs = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| {
                text.lines()
                    .find(|line| !line.trim().is_empty() && !line.starts_with('#'))
                    .map(|line| line.contains(',') || line.contains('\t'))
            })
            .unwrap_or(false);
        if pairs {
            if let Ok(added) = names.load_pairs(&path) {
                total += added;
            }
            continue;
        }
        for how in HashFn::all() {
            if let Ok(added) = names.load_wordlist(&path, how) {
                total = total.max(added);
            }
        }
    }
    total
}

pub fn categories(catalog: &Catalog) -> Vec<(Category, usize)> {

    catalog.by_category().into_iter().collect()
}

pub struct Harmony {
    pub state: State,
}

impl Harmony {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Harmony {
        theme::apply(&cc.egui_ctx);
        let mut state = State::new();
        state.icon = load_icon(&cc.egui_ctx);
        state.push_presence();
        Harmony { state }
    }
}

fn load_icon(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let bytes = include_bytes!("../../images/chinchou.png");
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let colour = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Some(ctx.load_texture("chinchou", colour, egui::TextureOptions::LINEAR))
}

impl eframe::App for Harmony {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        // Before anything else: the card went up last frame and is on screen, so
        // this hands the thread to ole until the pointer comes up.
        self.state.finish_drag_out();
        self.state.pump();
        // Filtering and regrouping happen here, once, rather than every time a
        // batch of a running scan lands.
        self.state.settle();
        keys(&ctx, &mut self.state);
        dropped(&ctx, &mut self.state);

        egui::Panel::top("top")
            .frame(top_frame())
            .show(root, |ui| ui::games::strip(ui, &mut self.state));

        egui::Panel::bottom("status")
            .frame(top_frame())
            .show(root, |ui| status(ui, &mut self.state));

        egui::Panel::bottom("transport")
            .frame(top_frame())
            .show(root, |ui| ui::player::bar(ui, &mut self.state));

        egui::Panel::left("left")
            .default_size(260.0)
            // Dragged to any width the window allows. Nothing inside a panel
            // gets to set how wide it is: long paths truncate, and the labels
            // keep their column.
            .resizable(true)
            .min_size(150.0)
            .max_size(560.0)
            .frame(side_frame())
            .show(root, |ui| ui::games::card(ui, &mut self.state));

        egui::Panel::right("right")
            .default_size(300.0)
            .resizable(true)
            .min_size(180.0)
            .max_size(640.0)
            .frame(side_frame())
            .show(root, |ui| ui::detail::panel(ui, &mut self.state));

        egui::CentralPanel::default()
            .frame(side_frame())
            .show(root, |ui| ui::browser::show(ui, &mut self.state));

        if self.state.show_queue {
            ui::queue::window(&ctx, &mut self.state);
        }

        // Anything stopped for want of disk room says so over everything else.
        ui::space::window(&ctx, &mut self.state);

        ui::dragout::card(&ctx, &mut self.state);

        // Over everything, while the last folder and its cache are still being
        // read on their threads.
        ui::splash::veil(&ctx, &self.state);

        // The window has no system border, so its edges are harmony's to
        // handle. Not while a carry is up: that gesture owns the pointer.
        crate::hm::window::chrome::edges(&ctx, self.state.dragging_out.is_some());
        self.state.remember_size(&ctx);

        let working = self.state.scanning()
            || self.state.loading.is_some()
            || self.state.looking.is_some()
            || self.state.queue.progress.lock().unwrap().running;
        if working {
            // Often enough to look alive, rarely enough to leave the disk and
            // the worker threads the machine they are busy with.
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        } else if self.state.transport.as_ref().is_some_and(|t| t.playing) {
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
    }
}

fn top_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::PANEL)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .stroke(egui::Stroke::new(1.0, theme::HAIRLINE))
}

fn side_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::PANEL)
        .inner_margin(egui::Margin::symmetric(8, 6))
}

/// The line along the bottom: what harmony is doing, then what is on screen.
///
/// It used to flash whichever message landed last, which with several games
/// being read at once was mostly noise. Now the work says itself in one phrase
/// and the counts sit on the right.
fn status(ui: &mut egui::Ui, state: &mut State) {
    ui.horizontal(|ui| {
        if let Some(error) = state.error.clone() {
            ui.colored_label(theme::UNLIT, error.to_ascii_lowercase());
            if ui.small_button("ok").clicked() {
                state.error = None;
            }
            return;
        }
        match state.activity() {
            Some(doing) => {
                ui.colored_label(theme::WAVE, doing.to_ascii_lowercase());
                if state.scanning() && ui.small_button("stop all").clicked() {
                    state.stop_all();
                }
            }
            None => {
                ui.colored_label(theme::DIM, state.status.to_ascii_lowercase());
            }
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {

            if state.rpc.is_linked() {
                ui.colored_label(theme::GOOD, "discord");
            }
            ui.colored_label(
                theme::DIM,
                format!("{} shown", ui::widgets::tally(state.filtered.len())),
            );
            if !state.selection.is_empty() {
                ui.colored_label(
                    theme::WAVE,
                    format!("{} picked", ui::widgets::tally(state.selection.len())),
                );
            }
        });
    });
}

fn keys(ctx: &egui::Context, state: &mut State) {
    let wants = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::Space),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::Escape),
            i.key_pressed(egui::Key::F),
        )
    });
    if ctx.memory(|m| m.focused().is_some()) {
        return;
    }
    let (space, down, up, escape, favorite) = wants;
    if space && let Some(loaded) = state.loaded.as_ref() {
        let samples = &loaded.samples;
        if let Some(transport) = state.transport.as_mut() {
            if transport.finished() {
                transport.play(samples);
            } else {
                transport.toggle();
            }
        }
    }
    let step = if down {
        1i32
    } else if up {
        -1
    } else {
        0
    };
    if step != 0 && !state.filtered.is_empty() {
        let position = state
            .cursor
            .and_then(|index| state.filtered.iter().position(|i| *i == index))
            .map(|p| p as i32)
            .unwrap_or(-1);
        let next = (position + step).clamp(0, state.filtered.len() as i32 - 1) as usize;
        let index = state.filtered[next];
        state.select(index, true);
    }
    // Escape during a carry calls it off. Once the drag itself has begun ole
    // has the thread and handles escape on its own; this covers the wait while
    // the files are still being written.
    if escape && state.dragging_out.is_some() {
        state.dragging_out = None;
        state.status = "drag cancelled".into();
        return;
    }
    if escape && let Some(transport) = state.transport.as_mut() {
        transport.stop();
    }
    if favorite && let Some(index) = state.cursor {
        state.toggle_favorite(index);
    }
}

/// A folder dropped on the window is taken as a game folder; a dropped file is
/// taken as a name list.
fn dropped(ctx: &egui::Context, state: &mut State) {
    let files: Vec<PathBuf> = ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .filter_map(|file| file.path.clone())
            .collect()
    });
    if files.is_empty() {
        return;
    }
    let mut lists: Vec<PathBuf> = Vec::new();
    for path in files {
        if path.is_dir() {
            state.look(path);
        } else {
            lists.push(path);
        }
    }
    if !lists.is_empty() {
        state.load_names(lists);
    }
}
