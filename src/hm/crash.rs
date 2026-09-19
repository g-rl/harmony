//! What harmony leaves behind when it falls over.
//!
//! The games themselves keep a `minidumps` folder with one `crash-<when>` in
//! it per fall, and anybody who has had to work out why a title would not
//! start already knows to look there. Harmony keeps the same shape, in a
//! folder of its own, and puts rather more in it: a report of what was being
//! done, the console as it stood, and the exports that were running, so that a
//! crash half way through a hundred thousand sound library run is something
//! that can be read afterwards rather than guessed at.
//!
//! A report is a folder rather than a file because three things want writing
//! and they are read at different times: `report.txt` first, `console.log`
//! when the report is not enough, `state.json` when somebody wants to pick the
//! work up by hand.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::hm::console::{self, Channel, Level};

/// How many crash folders are kept before the oldest goes.
pub const KEEP: usize = 20;

/// How much of the console goes in the report itself. The whole of it goes in
/// the file beside it.
const IN_REPORT: usize = 60;
const IN_FILE: usize = 2000;

/// What harmony was doing, kept up to date by the window so that a crash has
/// something to say beyond a line number.
///
/// Small, flat and cheap to replace: it is written every second or so from the
/// draw thread, and read once, by a process on its way out.
#[derive(Clone, Default)]
pub struct Doing {
    pub game: String,
    pub build: String,
    pub root: String,
    pub sounds: usize,
    pub shown: usize,
    pub picked: usize,
    pub names: usize,
    pub output: String,
    pub scanning: bool,
    /// One line per export lane: what it is, and how far it has got.
    pub lanes: Vec<String>,
    /// Exports that have been written down and not yet picked up.
    pub notes: Vec<String>,
}

static DOING: Mutex<Option<Doing>> = Mutex::new(None);

/// Tell the crash reporter what is going on. Called from the window.
pub fn doing(now: Doing) {
    if let Ok(mut lock) = DOING.lock() {
        *lock = Some(now);
    }
}

pub fn folder() -> PathBuf {
    crate::hm::storage::config_dir().join("crashes")
}

/// Put the panic hook in place.
///
/// The hook never panics itself: every step of it is allowed to fail quietly,
/// because the one thing worse than a crash without a report is a crash inside
/// the code that writes the report.
pub fn install() {
    let earlier = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        let thread = std::thread::current();
        let who = thread.name().unwrap_or("unnamed").to_string();
        let what = message_of(panic);
        let at = panic
            .location()
            .map(|place| format!("{}:{}:{}", place.file(), place.line(), place.column()))
            .unwrap_or_else(|| "somewhere with no line number".into());

        // Said on the console first: a panic on a worker does not take the
        // window with it, and the line is what the user sees.
        console::deep(
            Channel::App,
            Level::Error,
            format!("panic on {who}: {what}"),
            vec![format!("at {at}")],
        );

        // Taken here rather than later: the stack is still the one that fell
        // over. Forced, because nobody sets RUST_BACKTRACE before the crash
        // they did not know was coming.
        let trace = std::backtrace::Backtrace::force_capture().to_string();
        let wrote = write_report(&Reason::Panic {
            who,
            what,
            at,
            trace,
        });
        if let Some(path) = wrote.as_ref() {
            console::note(
                Channel::App,
                Level::Warn,
                "a crash report was written",
                path.to_string_lossy().to_ascii_lowercase(),
            );
        }
        earlier(panic);
    }));
}

/// Write a report now, without anything having gone wrong. The console's
/// `crash.dump` runs this, and so does anybody who wants the state of a long
/// run kept before they try something.
pub fn dump(why: &str) -> Option<PathBuf> {
    write_report(&Reason::Asked {
        why: why.to_string(),
    })
}

enum Reason {
    Panic {
        who: String,
        what: String,
        at: String,
        trace: String,
    },
    Asked {
        why: String,
    },
}

/// How many frames of the stack go in the report. Enough to see how the call
/// got there, not so many that the interesting part is off the bottom.
const FRAMES: usize = 40;

fn write_report(reason: &Reason) -> Option<PathBuf> {
    let stamp = crate::hm::storage::file_stamp();
    let into = folder().join(format!("crash-{stamp}"));
    std::fs::create_dir_all(&into).ok()?;

    let doing = DOING.lock().ok().and_then(|lock| lock.clone()).unwrap_or_default();
    let mut out = String::new();
    let mut line = |text: &str| {
        out.push_str(text);
        out.push('\n');
    };

    line("harmony crash report");
    line("");
    line(&format!("when      {} ({stamp})", crate::hm::storage::stamp()));
    line(&format!("version   {}", env!("CARGO_PKG_VERSION")));
    line(&format!(
        "build     {}",
        match cfg!(debug_assertions) {
            true => "debug",
            false => "release",
        }
    ));
    line(&format!("up for    {}", spell(console::uptime())));
    match reason {
        Reason::Panic {
            who,
            what,
            at,
            trace,
        } => {
            line("");
            line("what happened");
            line(&format!("  panic     {what}"));
            line(&format!("  thread    {who}"));
            line(&format!("  at        {at}"));
            if !trace.is_empty() && !trace.starts_with("disabled") {
                line("");
                line("how it got there");
                for row in trace.lines().take(FRAMES) {
                    line(&format!("  {}", row.trim_end()));
                }
            }
        }
        Reason::Asked { why } => {
            line("");
            line("what happened");
            line(&format!("  asked for {why}"));
            line("  nothing went wrong: this report was written on purpose");
        }
    }

    line("");
    line("machine");
    line(&format!("  os        {}", os()));
    line(&format!(
        "  threads   {}",
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0)
    ));
    if let Some((total, free)) = memory() {
        line(&format!(
            "  memory    {} of {} free",
            crate::hm::ui::widgets::bytes(free),
            crate::hm::ui::widgets::bytes(total)
        ));
    }
    line(&format!(
        "  exe       {}",
        std::env::current_exe()
            .map(|path| path.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_else(|_| "unknown".into())
    ));

    line("");
    line("what harmony was doing");
    line(&format!("  game      {}", blank(&doing.game)));
    line(&format!("  build     {}", blank(&doing.build)));
    line(&format!("  from      {}", blank(&doing.root)));
    line(&format!("  sounds    {} ({} shown, {} picked)", doing.sounds, doing.shown, doing.picked));
    line(&format!("  names     {}", doing.names));
    line(&format!("  output    {}", blank(&doing.output)));
    line(&format!("  scanning  {}", doing.scanning));
    match doing.lanes.is_empty() {
        true => line("  exports   none running"),
        false => {
            line("  exports");
            for lane in &doing.lanes {
                line(&format!("    {lane}"));
            }
        }
    }
    if !doing.notes.is_empty() {
        line("  put down");
        for note in &doing.notes {
            line(&format!("    {note}"));
        }
    }

    // The exports that were going are on disk already, as resume notes. Saying
    // where they are is the difference between a report and a way back.
    line("");
    line("picking the work back up");
    line(&format!("  resume notes  {}", crate::hm::storage::resume_dir().to_string_lossy().to_ascii_lowercase()));
    line("  open harmony again, choose the same game, and the put-down exports");
    line("  are listed under the export panel. they carry on into the same");
    line("  folder and step over every file already written.");

    let tail = console::tail(IN_REPORT);
    if !tail.is_empty() {
        line("");
        line(&format!("the last {} console lines", tail.len()));
        for row in &tail {
            line(&format!("  {row}"));
        }
        line("");
        line("  the rest is in console.log beside this file");
    }

    let _ = std::fs::write(into.join("report.txt"), out);
    let _ = std::fs::write(into.join("console.log"), console::tail(IN_FILE).join("\n"));
    let _ = std::fs::write(into.join("state.json"), state_json(&doing));
    prune(KEEP);
    Some(into)
}

fn state_json(doing: &Doing) -> String {
    // Written by hand rather than through serde: a crash report is written
    // while the process is already on its way out, and this way it cannot
    // fail on anything.
    let lanes: Vec<String> = doing.lanes.iter().map(|lane| quote(lane)).collect();
    let notes: Vec<String> = doing.notes.iter().map(|note| quote(note)).collect();
    format!(
        "{{\n  \"when\": {},\n  \"version\": {},\n  \"game\": {},\n  \"build\": {},\n  \"root\": {},\n  \"sounds\": {},\n  \"output\": {},\n  \"scanning\": {},\n  \"lanes\": [{}],\n  \"notes\": [{}]\n}}\n",
        quote(&crate::hm::storage::stamp()),
        quote(env!("CARGO_PKG_VERSION")),
        quote(&doing.game),
        quote(&doing.build),
        quote(&doing.root),
        doing.sounds,
        quote(&doing.output),
        doing.scanning,
        lanes.join(", "),
        notes.join(", ")
    )
}

fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for glyph in text.chars() {
        match glyph {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' | '\r' | '\t' => out.push(' '),
            other if (other as u32) < 0x20 => out.push(' '),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn blank(text: &str) -> &str {
    match text.trim().is_empty() {
        true => "none",
        false => text,
    }
}

/// Every crash report there is, newest first.
pub fn reports() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder()) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("crash-"))
        })
        .collect();
    // The name carries the time it happened, down to the second, so sorting
    // the names sorts the reports — no file dates to be lost on a copy.
    found.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    found
}

/// Keep the newest `keep` reports and let the rest go.
pub fn prune(keep: usize) {
    for old in reports().into_iter().skip(keep) {
        let _ = std::fs::remove_dir_all(old);
    }
}

fn message_of(panic: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = panic.payload();
    if let Some(text) = payload.downcast_ref::<&str>() {
        return (*text).to_string();
    }
    if let Some(text) = payload.downcast_ref::<String>() {
        return text.clone();
    }
    "a panic with nothing to say for itself".into()
}

/// What windows actually is, rather than what it tells an application it is.
///
/// `GetVersion` has answered 6.2 to everything since windows 8 unless the
/// binary carries a manifest saying otherwise, which makes it useless in a
/// report. `RtlGetVersion` is the one the kernel answers honestly, so that is
/// the one asked - and if it cannot be reached, the lie is better than
/// nothing.
#[cfg(windows)]
#[repr(C)]
struct OsVersion {
    size: u32,
    major: u32,
    minor: u32,
    build: u32,
    platform: u32,
    service_pack: [u16; 128],
}

fn os() -> String {
    #[cfg(windows)]
    {
        if let Some(said) = real_windows() {
            return said;
        }
        use windows::Win32::System::SystemInformation::GetVersion;
        let packed = unsafe { GetVersion() };
        let major = packed & 0xff;
        let minor = (packed >> 8) & 0xff;
        return format!("windows {major}.{minor} ({})", std::env::consts::ARCH);
    }
    #[cfg(not(windows))]
    format!("{} ({})", std::env::consts::OS, std::env::consts::ARCH)
}

#[cfg(windows)]
fn real_windows() -> Option<String> {
    unsafe {
        let ntdll = libloading::Library::new("ntdll.dll").ok()?;
        let ask: libloading::Symbol<unsafe extern "system" fn(*mut OsVersion) -> i32> =
            ntdll.get(b"RtlGetVersion").ok()?;
        let mut about = OsVersion {
            size: std::mem::size_of::<OsVersion>() as u32,
            major: 0,
            minor: 0,
            build: 0,
            platform: 0,
            service_pack: [0; 128],
        };
        if ask(&mut about) != 0 {
            return None;
        }
        Some(format!(
            "windows {}.{}.{} ({})",
            about.major,
            about.minor,
            about.build,
            std::env::consts::ARCH
        ))
    }
}

fn memory() -> Option<(u64, u64)> {
    #[cfg(windows)]
    {
        use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut about = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        unsafe { GlobalMemoryStatusEx(&mut about) }.ok()?;
        return Some((about.ullTotalPhys, about.ullAvailPhys));
    }
    #[cfg(not(windows))]
    None
}

/// Seconds as something a person reads.
fn spell(seconds: f64) -> String {
    let whole = seconds as u64;
    let (hours, minutes, rest) = (whole / 3600, (whole % 3600) / 60, whole % 60);
    match hours {
        0 => format!("{minutes}m {rest:02}s"),
        _ => format!("{hours}h {minutes:02}m {rest:02}s"),
    }
}

/// Where the reports live, said the way the console says it.
pub fn where_they_are() -> String {
    folder().to_string_lossy().to_ascii_lowercase()
}

pub fn is_report(path: &Path) -> bool {
    path.join("report.txt").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_is_named_after_the_moment_it_happened() {
        let stamp = crate::hm::storage::file_stamp();
        assert_eq!(stamp.len(), "2026-09-16-15-36-16".len());
        assert_eq!(stamp.matches('-').count(), 5);
    }

    #[test]
    fn a_line_of_state_survives_being_written_as_json() {
        let quoted = quote("c:\\games\\call of duty \"iii\"\nnext");
        assert!(quoted.starts_with('"') && quoted.ends_with('"'));
        assert!(!quoted.contains('\n'));
        assert!(quoted.contains("\\\\"));
    }

    #[test]
    fn nothing_known_reads_as_none() {
        assert_eq!(blank("  "), "none");
        assert_eq!(blank("t7"), "t7");
    }
}
