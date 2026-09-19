use std::path::PathBuf;

/// The folders and files harmony remembers between runs, and whether they are
/// still where they were.
///
/// A remembered path is a convenience, not a fact: drives get unplugged, games
/// get uninstalled, an export folder gets tidied away. When one of them stops
/// being there the right thing is to quietly forget it and go back to the
/// unselected state — a tab with no folder behind it, an export folder that is
/// asked for again — rather than to keep it and say so every time it is
/// touched.
#[derive(Clone, Debug, Default)]
pub struct Missing {
    /// Remembered game folders that are gone, by title key.
    pub roots: Vec<String>,
    /// The folder harmony writes extractions into.
    pub output: bool,
    /// The folders scans and scratch files are kept in, when they were moved
    /// somewhere that no longer exists. Forgetting one means going back to the
    /// default beside the settings, which harmony can always make.
    pub cache_dir: bool,
    pub temp_dir: bool,
    /// The folder that was open when harmony was last closed.
    pub last_root: bool,
    /// Name lists that have been deleted or moved since they were loaded.
    pub name_files: Vec<PathBuf>,
    /// Put-down exports whose folders are no longer where they were written,
    /// by the name of the file each is kept in.
    pub resumes: Vec<String>,
    /// The folder on screen right now.
    pub open: bool,
}

impl Missing {
    pub fn anything(&self) -> bool {
        !self.roots.is_empty()
            || self.output
            || self.cache_dir
            || self.temp_dir
            || self.last_root
            || !self.name_files.is_empty()
            || !self.resumes.is_empty()
            || self.open
    }
}

/// What to look at, gathered on the draw thread and checked on a worker.
///
/// Nothing here is read from disk while it is being collected: these are the
/// paths only, and every `stat` happens on the thread that runs [`look`].
#[derive(Clone, Debug, Default)]
pub struct Ask {
    pub roots: Vec<(String, PathBuf)>,
    pub output: Option<PathBuf>,
    pub cache_dir: Option<PathBuf>,
    pub temp_dir: Option<PathBuf>,
    pub last_root: Option<PathBuf>,
    pub name_files: Vec<PathBuf>,
    /// Each put-down export: the file it is kept in, and the folder it was
    /// writing into.
    pub resumes: Vec<(String, PathBuf)>,
    pub open: Option<PathBuf>,
}

/// Which of them are gone.
///
/// A folder counts as gone only when the answer is a plain no. A path that
/// cannot be read for some other reason — a permission, a drive that is busy —
/// is left alone, because forgetting it would lose something the user still
/// has.
pub fn look(ask: &Ask) -> Missing {
    Missing {
        roots: ask
            .roots
            .iter()
            .filter(|(_, path)| !there(path))
            .map(|(key, _)| key.clone())
            .collect(),
        output: ask.output.as_deref().is_some_and(|path| !there(path)),
        cache_dir: ask.cache_dir.as_deref().is_some_and(|path| !there(path)),
        temp_dir: ask.temp_dir.as_deref().is_some_and(|path| !there(path)),
        last_root: ask.last_root.as_deref().is_some_and(|path| !there(path)),
        name_files: ask
            .name_files
            .iter()
            .filter(|path| !is_file(path))
            .cloned()
            .collect(),
        resumes: ask
            .resumes
            .iter()
            .filter(|(_, path)| !there(path))
            .map(|(name, _)| name.clone())
            .collect(),
        open: ask.open.as_deref().is_some_and(|path| !there(path)),
    }
}

/// Is this folder still a folder?
fn there(path: &std::path::Path) -> bool {
    match std::fs::metadata(path) {
        Ok(about) => about.is_dir(),
        // `NotFound` is the only answer that means gone. Everything else —
        // a permission, a drive that is spinning up, a network share that is
        // reconnecting — is a maybe, and a maybe keeps the path.
        Err(error) => error.kind() != std::io::ErrorKind::NotFound,
    }
}

fn is_file(path: &std::path::Path) -> bool {
    match std::fs::metadata(path) {
        Ok(about) => about.is_file(),
        Err(error) => error.kind() != std::io::ErrorKind::NotFound,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A path that was never there is gone; the folder harmony is running in
    /// is not. Nothing else is guessed at.
    #[test]
    fn only_what_is_actually_gone() {
        let here = std::env::current_dir().expect("a working directory");
        let ask = Ask {
            roots: vec![
                ("t7".into(), here.clone()),
                ("iw3".into(), here.join("no such folder at all")),
            ],
            output: Some(here.join("nor this one")),
            last_root: Some(here.clone()),
            ..Ask::default()
        };
        let missing = look(&ask);
        assert_eq!(missing.roots, vec!["iw3".to_string()]);
        assert!(missing.output);
        assert!(!missing.last_root);
        assert!(missing.anything());
    }

    /// Nothing remembered, nothing to forget.
    #[test]
    fn an_empty_ask_finds_nothing() {
        assert!(!look(&Ask::default()).anything());
    }
}
