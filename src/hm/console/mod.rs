//! Harmony's console: one log, split into channels, and the commands typed at
//! it.
//!
//! Every part of harmony writes here — the scanner, the mounts, the export
//! pool, the player, the watcher on the remembered paths — and none of them
//! know anything about the window. A line is a channel, a level, a sentence,
//! and optionally a note beside it and a few rows underneath it. The window
//! reads the ring; the writer thread puts the same lines down on disk; a crash
//! report takes the tail of them. One log, three readers.
//!
//! Nothing here draws, so nothing here needs egui. A channel says what colour
//! it wants as three bytes and the window works out the rest.

pub mod cmd;

use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// How many lines the window can scroll back through. Past this the oldest go,
/// and the count of what went is kept so the console can say so rather than
/// quietly losing them.
pub const KEEP: usize = 4000;

/// What the log is called on disk, beside the settings.
pub const FILE: &str = "console.log";

/// How big that file gets before the last one is rolled aside.
pub const ROLL: u64 = 4 * 1024 * 1024;

/// Which part of harmony a line came from.
///
/// A channel is not a severity and not a module path: it is the thing a person
/// would say they were watching. Somebody waiting on a library run turns
/// everything off but `export`; somebody working out why a game will not open
/// wants `game` and `pack` and nothing else.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Channel {
    #[default]
    App,
    Game,
    Scan,
    Pack,
    Export,
    Audio,
    Names,
    Disk,
    Discord,
    Debug,
}

impl Channel {
    pub const ALL: [Channel; 10] = [
        Channel::App,
        Channel::Game,
        Channel::Scan,
        Channel::Pack,
        Channel::Export,
        Channel::Audio,
        Channel::Names,
        Channel::Disk,
        Channel::Discord,
        Channel::Debug,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Channel::App => "app",
            Channel::Game => "game",
            Channel::Scan => "scan",
            Channel::Pack => "pack",
            Channel::Export => "export",
            Channel::Audio => "audio",
            Channel::Names => "names",
            Channel::Disk => "disk",
            Channel::Discord => "discord",
            Channel::Debug => "debug",
        }
    }

    /// What this channel is for, in the console's own help.
    pub fn about(self) -> &'static str {
        match self {
            Channel::App => "the window itself: settings, tabs, what was clicked",
            Channel::Game => "detecting, opening and closing installs",
            Channel::Scan => "catalogue scans and the caches they write",
            Channel::Pack => "containers: fast files, sabs, casc, oodle",
            Channel::Export => "the export pool and the queue lanes",
            Channel::Audio => "decoding and playing sounds",
            Channel::Names => "name lists and hash matching",
            Channel::Disk => "free space, folders, paths that came and went",
            Channel::Discord => "rich presence",
            Channel::Debug => "detail nobody needs until they do",
        }
    }

    /// The colour this channel is spelled in. Fixed rather than taken off the
    /// theme: a channel that changed colour with the window is a channel
    /// nobody learns.
    pub fn tint(self) -> [u8; 3] {
        match self {
            Channel::App => [0xa4, 0xa4, 0xb8],
            Channel::Game => [0x7a, 0xb8, 0xff],
            Channel::Scan => [0x8c, 0xd0, 0x6a],
            Channel::Pack => [0xd9, 0xa0, 0x5c],
            Channel::Export => [0x39, 0xd0, 0xd8],
            Channel::Audio => [0x4c, 0xd6, 0xb4],
            Channel::Names => [0xa8, 0x8c, 0xf0],
            Channel::Disk => [0xc9, 0x8c, 0x6a],
            Channel::Discord => [0x88, 0x8c, 0xe8],
            Channel::Debug => [0x5a, 0x5a, 0x6e],
        }
    }

    pub fn from_label(name: &str) -> Option<Channel> {
        Channel::ALL
            .into_iter()
            .find(|channel| channel.label() == name)
    }

    /// The number key that turns this channel on and off.
    pub fn number(self) -> usize {
        Channel::ALL
            .into_iter()
            .position(|other| other == self)
            .unwrap_or(0)
            + 1
    }
}

/// How much a line matters.
///
/// `Typed` and `Reply` are not severities at all — they are the two halves of a
/// command, kept in the same stream so that what was asked for and what came
/// back read in order with everything that happened in between.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Level {
    Trace,
    #[default]
    Info,
    Good,
    Warn,
    Error,
    Typed,
    Reply,
}

impl Level {
    pub const ALL: [Level; 7] = [
        Level::Trace,
        Level::Info,
        Level::Good,
        Level::Warn,
        Level::Error,
        Level::Typed,
        Level::Reply,
    ];

    /// The tag in the left lane. Five columns, always, so the sentences line
    /// up down the window and a burst of failures is a shape rather than a
    /// thing to read.
    pub fn tag(self) -> &'static str {
        match self {
            Level::Trace => "trace",
            Level::Info => "info ",
            Level::Good => "done ",
            Level::Warn => "warn ",
            Level::Error => "fail ",
            Level::Typed => "you  ",
            Level::Reply => "says ",
        }
    }

    pub fn label(self) -> &'static str {
        self.tag().trim_end()
    }

    pub fn tint(self) -> [u8; 3] {
        match self {
            Level::Trace => [0x4a, 0x4a, 0x5c],
            Level::Info => [0xa4, 0xa4, 0xb8],
            Level::Good => [0x5a, 0xd8, 0x9a],
            Level::Warn => [0xe0, 0xa8, 0x4c],
            Level::Error => [0xff, 0x4d, 0x6a],
            Level::Typed => [0xff, 0x2e, 0x88],
            Level::Reply => [0xe8, 0xe8, 0xf0],
        }
    }

    pub fn from_label(name: &str) -> Option<Level> {
        Level::ALL.into_iter().find(|level| level.label() == name)
    }

    /// Is this line worth waking somebody for? Used by the strip along the
    /// bottom, which counts what has gone wrong since the console was last
    /// looked at.
    pub fn loud(self) -> bool {
        matches!(self, Level::Warn | Level::Error)
    }
}

/// One line of the log.
#[derive(Clone, Debug)]
pub struct Line {
    pub seq: u64,
    /// Seconds since harmony started, which is what a log of a long run is
    /// actually read against.
    pub at: f64,
    /// The wall clock, for the report somebody sends on to somebody else.
    pub clock: String,
    pub channel: Channel,
    pub level: Level,
    pub text: String,
    /// A short something on the right of the line: a size, a count, a rate.
    pub note: Option<String>,
    /// The detail under the line, shown folded in the window and written out
    /// in full to the file. This is what makes a console log worth keeping:
    /// the sentence says what happened, the rows underneath say enough to act
    /// on it.
    pub under: Vec<String>,
}

impl Line {
    /// The line as the file and a crash report hold it.
    pub fn plain(&self) -> String {
        let mut out = format!(
            "[{:9.3}] {:<7} {} {}",
            self.at,
            self.channel.label(),
            self.level.tag(),
            self.text
        );
        if let Some(note) = &self.note {
            out.push_str("  (");
            out.push_str(note);
            out.push(')');
        }
        for row in &self.under {
            out.push('\n');
            out.push_str("             ");
            out.push_str(row);
        }
        out
    }
}

struct Board {
    lines: VecDeque<Line>,
    seq: u64,
    started: Instant,
    /// Lines that fell off the back of the ring.
    dropped: u64,
    /// Warnings and failures since the console was last read, for the badge on
    /// the status bar.
    unread: usize,
}

static BOARD: OnceLock<Mutex<Board>> = OnceLock::new();
static SPILL: OnceLock<Sender<String>> = OnceLock::new();

fn board() -> &'static Mutex<Board> {
    BOARD.get_or_init(|| {
        Mutex::new(Board {
            lines: VecDeque::with_capacity(512),
            seq: 0,
            started: Instant::now(),
            dropped: 0,
            unread: 0,
        })
    })
}

pub fn path() -> PathBuf {
    crate::hm::storage::config_dir().join(FILE)
}

/// Start the log, and the thread that writes it down.
///
/// Writing happens on a thread of its own because a line is written from
/// wherever it happened — a worker in the export pool, the scanner, the draw
/// thread — and none of those should wait on a disk to say something.
pub fn boot() {
    let _ = board();
    SPILL.get_or_init(|| {
        let (tx, rx) = channel::<String>();
        std::thread::Builder::new()
            .name("console".into())
            .spawn(move || {
                let path = path();
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                roll(&path);
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .ok();
                let mut owed = 0usize;
                loop {
                    match rx.recv_timeout(std::time::Duration::from_millis(500)) {
                        Ok(text) => {
                            if let Some(file) = file.as_mut() {
                                let _ = writeln!(file, "{text}");
                                owed += 1;
                                if owed >= 64 {
                                    let _ = file.flush();
                                    owed = 0;
                                }
                            }
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            if owed > 0 && let Some(file) = file.as_mut() {
                                let _ = file.flush();
                                owed = 0;
                            }
                        }
                        // Everything that could write has gone: so has harmony.
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
                if let Some(file) = file.as_mut() {
                    let _ = file.flush();
                }
            })
            .ok();
        tx
    });
}

/// Keep the log from growing without end: one file back is enough to cover the
/// session before this one, which is the one somebody comes asking about.
fn roll(path: &std::path::Path) {
    let big = std::fs::metadata(path).map(|about| about.len()).unwrap_or(0) > ROLL;
    if big {
        let _ = std::fs::rename(path, path.with_extension("log.1"));
    }
}

/// Put a line in the log.
pub fn say(channel: Channel, level: Level, text: impl Into<String>) -> u64 {
    write(channel, level, text.into(), None, Vec::new())
}

/// A line with a short something on the right of it.
pub fn note(
    channel: Channel,
    level: Level,
    text: impl Into<String>,
    note: impl Into<String>,
) -> u64 {
    write(channel, level, text.into(), Some(note.into()), Vec::new())
}

/// A line with the detail under it.
pub fn deep(channel: Channel, level: Level, text: impl Into<String>, under: Vec<String>) -> u64 {
    write(channel, level, text.into(), None, under)
}

pub fn trace(channel: Channel, text: impl Into<String>) -> u64 {
    say(channel, Level::Trace, text)
}

pub fn info(channel: Channel, text: impl Into<String>) -> u64 {
    say(channel, Level::Info, text)
}

pub fn good(channel: Channel, text: impl Into<String>) -> u64 {
    say(channel, Level::Good, text)
}

pub fn warn(channel: Channel, text: impl Into<String>) -> u64 {
    say(channel, Level::Warn, text)
}

pub fn fail(channel: Channel, text: impl Into<String>) -> u64 {
    say(channel, Level::Error, text)
}

fn write(
    channel: Channel,
    level: Level,
    text: String,
    note: Option<String>,
    under: Vec<String>,
) -> u64 {
    let Ok(mut board) = board().lock() else {
        return 0;
    };
    board.seq += 1;
    let line = Line {
        seq: board.seq,
        at: board.started.elapsed().as_secs_f64(),
        clock: crate::hm::storage::clock(),
        channel,
        level,
        text,
        note,
        under,
    };
    if level.loud() {
        board.unread += 1;
    }
    if let Some(spill) = SPILL.get() {
        let _ = spill.send(format!("{} {}", line.clock, line.plain()));
    }
    let seq = line.seq;
    board.lines.push_back(line);
    while board.lines.len() > KEEP {
        board.lines.pop_front();
        board.dropped += 1;
    }
    seq
}

/// Read the log without copying it.
pub fn read<R>(look: impl FnOnce(&VecDeque<Line>) -> R) -> R {
    match board().lock() {
        Ok(board) => look(&board.lines),
        Err(poisoned) => look(&poisoned.into_inner().lines),
    }
}

/// The last few lines, flattened, for a crash report.
pub fn tail(count: usize) -> Vec<String> {
    read(|lines| {
        lines
            .iter()
            .skip(lines.len().saturating_sub(count))
            .map(|line| format!("{} {}", line.clock, line.plain()))
            .collect()
    })
}

pub fn clear() {
    if let Ok(mut board) = board().lock() {
        board.lines.clear();
        board.unread = 0;
    }
}

/// How many warnings and failures have gone by unread, and forget them.
pub fn take_unread() -> usize {
    match board().lock() {
        Ok(mut board) => std::mem::take(&mut board.unread),
        Err(_) => 0,
    }
}

pub fn unread() -> usize {
    board().lock().map(|board| board.unread).unwrap_or(0)
}

/// How long harmony has been up.
pub fn uptime() -> f64 {
    board()
        .lock()
        .map(|board| board.started.elapsed().as_secs_f64())
        .unwrap_or(0.0)
}

pub fn dropped() -> u64 {
    board().lock().map(|board| board.dropped).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_channel_is_named_and_numbered() {
        for channel in Channel::ALL {
            assert_eq!(Channel::from_label(channel.label()), Some(channel));
            assert!(channel.number() >= 1 && channel.number() <= Channel::ALL.len());
        }
    }

    #[test]
    fn every_tag_is_the_same_width() {
        let width = Level::Info.tag().len();
        for level in Level::ALL {
            assert_eq!(level.tag().len(), width, "{:?}", level);
            assert_eq!(Level::from_label(level.label()), Some(level));
        }
    }

    #[test]
    fn a_line_writes_itself_out_with_its_detail() {
        let line = Line {
            seq: 4,
            at: 12.5,
            clock: "12:00:00".into(),
            channel: Channel::Export,
            level: Level::Error,
            text: "could not write weapon/ak.wav".into(),
            note: Some("2 of 40".into()),
            under: vec!["reason: not an opus stream".into()],
        };
        let plain = line.plain();
        assert!(plain.contains("export"));
        assert!(plain.contains("fail"));
        assert!(plain.contains("(2 of 40)"));
        assert!(plain.lines().count() == 2);
    }

    #[test]
    fn the_ring_keeps_the_newest_and_counts_the_rest() {
        // Uses the real board, which other tests also write to, so this works
        // in what it added rather than in absolute numbers.
        let before = read(|lines| lines.len());
        for index in 0..10 {
            say(Channel::Debug, Level::Trace, format!("line {index}"));
        }
        let after = read(|lines| lines.len());
        assert!(after >= before);
        assert!(after <= KEEP);
        let last = read(|lines| lines.back().map(|line| line.text.clone()));
        assert_eq!(last.as_deref(), Some("line 9"));
    }
}
