use std::io::Write;
use std::path::{Path, PathBuf};

/// The log a library run leaves behind it, beside the sounds.
///
/// A run of a hundred thousand files takes hours and is not watched all the
/// way through. What it wrote is in `manifest.json` when that is asked for;
/// this is the other half — what it was told to do, what it left out and why,
/// what went wrong, and how long it all took. It is written even when the run
/// is cancelled, because a run that stopped half way is exactly the one
/// somebody comes back to read.
pub const NAME: &str = "liblog.txt";

#[derive(Clone)]
pub struct Log {
    path: PathBuf,
    lines: Vec<String>,
}

impl Log {
    pub fn new(root: &Path) -> Log {
        Log {
            path: root.join(NAME),
            lines: Vec::new(),
        }
    }

    pub fn say(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
    }

    pub fn blank(&mut self) {
        self.lines.push(String::new());
    }

    /// Write what has been said so far, replacing whatever was there.
    ///
    /// Called at the end of a run and at every checkpoint along it, so a run
    /// that is killed outright still leaves a readable log behind.
    pub fn flush(&self) {
        let Ok(mut file) = std::fs::File::create(&self.path) else {
            return;
        };
        for line in &self.lines {
            let _ = writeln!(file, "{line}");
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// `1h 12m 04s`, for a run that is measured in hours rather than frames.
pub fn spell(seconds: f32) -> String {
    let whole = seconds.max(0.0) as u64;
    let (hours, minutes, rest) = (whole / 3600, (whole % 3600) / 60, whole % 60);
    match hours {
        0 => format!("{minutes}m {rest:02}s"),
        _ => format!("{hours}h {minutes:02}m {rest:02}s"),
    }
}
