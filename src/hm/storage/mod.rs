pub mod space;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::hm::export::{Format, layout::Layout};

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub last_root: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub volume: f32,
    pub discord: bool,
    pub discord_app_id: String,
    pub format: String,
    pub layout: String,
    pub preserve_paths: bool,
    pub normalise_names: bool,
    pub skip_duplicates: bool,
    pub write_manifest: bool,
    /// Settings written before this was spelled the american way still say
    /// `favourites`, and a person's starred sounds are not worth losing over
    /// a vowel.
    #[serde(alias = "favourites")]
    pub favorites: Vec<String>,
    pub tags: HashMap<String, Vec<String>>,
    pub collections: HashMap<String, Vec<String>>,
    pub scans: HashMap<String, Scan>,
    /// Name lists the user pointed harmony at. Harmony ships none of its own.
    #[serde(default)]
    pub name_files: Vec<PathBuf>,
    /// Named sets of extraction options.
    #[serde(default)]
    pub presets: HashMap<String, Preset>,
    /// Where each game was last found, keyed by title id, so a tab can switch
    /// straight to it instead of asking for the folder again.
    #[serde(default)]
    pub roots: HashMap<String, PathBuf>,
    /// The size the window was left at.
    #[serde(default)]
    pub window_size: Option<[f32; 2]>,
    /// Where scan caches are written. A catalogue of a big install runs to tens
    /// of megabytes, so it does not have to live on the system disk.
    #[serde(default)]
    pub cache_dir: Option<PathBuf>,
    /// Where the scratch files a drag out of the window needs are written.
    #[serde(default)]
    pub temp_dir: Option<PathBuf>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Preset {
    pub format: String,
    pub layout: String,
    pub preserve_paths: bool,
    pub normalise_names: bool,
    pub skip_duplicates: bool,
    pub write_manifest: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Scan {
    pub at: String,
    pub sounds: usize,
    pub packages: usize,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            last_root: None,
            output: None,
            volume: 0.8,
            discord: true,
            discord_app_id: crate::hm::discord::APP_ID.to_string(),
            format: "wav".into(),
            layout: "category/package".into(),
            preserve_paths: true,
            normalise_names: true,
            skip_duplicates: true,
            write_manifest: false,
            favorites: Vec::new(),
            tags: HashMap::new(),
            collections: HashMap::new(),
            scans: HashMap::new(),
            name_files: Vec::new(),
            presets: HashMap::new(),
            roots: HashMap::new(),
            window_size: None,
            cache_dir: None,
            temp_dir: None,
        }
    }
}

impl Settings {
    pub fn format(&self) -> Format {
        match self.format.as_str() {
            "ogg" => Format::Ogg,
            "raw" => Format::Raw,
            _ => Format::Wav,
        }
    }

    pub fn layout(&self) -> Layout {
        match self.layout.as_str() {
            "language/category" => Layout::LanguageCategory,
            "package" => Layout::Package,
            "flat" => Layout::Flat,
            _ => Layout::CategoryPackage,
        }
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("harmony")
}

// Where the caches and the scratch files go is a setting, but the code that
// writes them is spread across threads that have no settings to hand, so the
// two folders are held here and read wherever they are needed.
static CACHE_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);
static TEMP_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Point the cache and scratch folders wherever the settings say. Called once
/// at startup and again whenever the user moves one.
pub fn use_folders(settings: &Settings) {
    if let Ok(mut lock) = CACHE_DIR.write() {
        *lock = settings.cache_dir.clone();
    }
    if let Ok(mut lock) = TEMP_DIR.write() {
        *lock = settings.temp_dir.clone();
    }
}

/// Where scan caches are written: the user's folder, or beside the settings.
pub fn cache_dir() -> PathBuf {
    CACHE_DIR
        .read()
        .ok()
        .and_then(|lock| lock.clone())
        .unwrap_or_else(config_dir)
}

/// Where scratch files are written: the user's folder, or the system's temp.
pub fn temp_dir() -> PathBuf {
    TEMP_DIR
        .read()
        .ok()
        .and_then(|lock| lock.clone())
        .unwrap_or_else(std::env::temp_dir)
}

/// Carry the caches already written over to a folder the user just chose. A
/// name already taken at the far end is left alone: a cache is rebuildable,
/// somebody else's file is not.
pub fn move_caches(from: &Path, to: &Path) -> usize {
    if from == to {
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(from) else {
        return 0;
    };
    let _ = std::fs::create_dir_all(to);
    let mut moved = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !is_cache(name) {
            continue;
        }
        let target = to.join(name);
        if target.exists() {
            continue;
        }
        if std::fs::rename(&path, &target).is_ok()
            || (std::fs::copy(&path, &target).is_ok() && std::fs::remove_file(&path).is_ok())
        {
            moved += 1;
        }
    }
    moved
}

/// Is this one of harmony's caches? The scans, and the zone indexes beside
/// them.
fn is_cache(name: &str) -> bool {
    (name.starts_with("scan-") || name.starts_with("zones-")) && name.ends_with(".json")
}

/// What the caches on disk come to, for the folder card to show.
pub fn cache_bytes() -> u64 {
    let Ok(entries) = std::fs::read_dir(cache_dir()) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(is_cache)
        })
        .filter_map(|entry| entry.metadata().ok())
        .map(|meta| meta.len())
        .sum()
}

pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn load() -> Settings {
    let path = settings_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Settings::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save(settings: &Settings) {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(path, text);
    }
}

/// The time now, as the user's own clock reads it.
///
/// A scan stamped in utc reads as an hour or eight out to whoever is looking
/// at it, so the local clock is asked first and utc is only the fallback.
pub fn stamp() -> String {
    #[cfg(windows)]
    if let Some(now) = local_now() {
        return now;
    }
    use std::time::{SystemTime, UNIX_EPOCH};
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = seconds / 86_400;
    let rest = seconds % 86_400;
    let (year, month, day) = civil(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rest / 3600,
        (rest % 3600) / 60
    )
}

/// Windows keeps the local clock, timezone and daylight saving already
/// worked out, so harmony asks for it rather than doing that arithmetic.
#[cfg(windows)]
fn local_now() -> Option<String> {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let now = unsafe { GetLocalTime() };
    Some(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        now.wYear, now.wMonth, now.wDay, now.wHour, now.wMinute
    ))
}

fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// A scan is expensive on a big install, so the result is kept on disk and
/// reloaded instead of being redone. Only what the scanner found is stored:
/// harmony keeps no game data of its own.
#[derive(Clone, Serialize, Deserialize)]
pub struct Cached {
    pub game: String,
    pub depth: String,
    pub at: String,
    pub packages: Vec<String>,
    pub keys: usize,
    pub sounds: Vec<CachedSound>,
    /// The containers this scan read, as they were on disk at the time. A
    /// refresh stats them again: same size, same date, same file.
    #[serde(default)]
    pub files: Vec<CachedFile>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedFile {
    pub path: PathBuf,
    pub bytes: u64,
    /// Seconds since the epoch, as the filesystem reports them.
    pub at: u64,
}

/// What a container looks like on disk right now.
pub fn stamp_of(path: &Path) -> Option<CachedFile> {
    let meta = std::fs::metadata(path).ok()?;
    let at = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|since| since.as_secs())
        .unwrap_or(0);
    Some(CachedFile {
        path: path.to_path_buf(),
        bytes: meta.len(),
        at,
    })
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CachedSound {
    pub key: u64,
    pub package: u32,
    pub index: u32,
    pub channels: u8,
    pub frames: u64,
    pub bytes: u64,
    pub language: Option<String>,
    pub category: String,
    /// The name, where the container carried one. The kapi titles never do, so
    /// this is empty for them and the name list fills it in instead.
    #[serde(default)]
    pub name: Option<String>,
    /// Everything below is defaulted, so a cache written before these existed
    /// still loads: it simply comes back as the opus a kapi scan produced.
    #[serde(default)]
    pub rate: u32,
    #[serde(default)]
    pub codec: String,
    /// Whether this came out of a bank rather than a keyed stream.
    #[serde(default)]
    pub bank: bool,
    /// Whether the key is one harmony worked out rather than one the container
    /// carried. These are shown by where they sit instead.
    #[serde(default)]
    pub slot: bool,
}

/// What one fastfile turned out to hold, kept so it never has to be inflated
/// twice.
///
/// A zone is a couple of hundred megabytes of zlib and there are three hundred
/// of them in a modern warfare 2 install: two minutes of inflating to learn
/// something that fits in a few hundred kilobytes and does not change until
/// the game is patched. The file it came from is stamped alongside, so a
/// patched zone is read again and an untouched one is not.
#[derive(Clone, Serialize, Deserialize)]
pub struct CachedZone {
    pub file: CachedFile,
    pub sounds: Vec<CachedZoneSound>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CachedZoneSound {
    pub name: String,
    /// Where the samples start in the inflated zone.
    pub at: u64,
    pub bytes: u32,
    pub rate: u32,
    pub channels: u8,
    pub bits: u16,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct CachedZones {
    pub zones: Vec<CachedZone>,
}

pub fn zones_path(game: &str) -> PathBuf {
    cache_dir().join(format!("zones-{game}.json"))
}

pub fn load_zones(game: &str) -> CachedZones {
    std::fs::read_to_string(zones_path(game))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Keep what the zones held. A cache that cannot be written is not an error:
/// the next mount simply does the work again.
pub fn save_zones(game: &str, zones: &CachedZones) {
    let path = zones_path(game);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string(zones) {
        let _ = std::fs::write(&path, text);
    }
}

pub fn cache_path(game: &str, depth: &str) -> PathBuf {
    cache_dir().join(format!("scan-{game}-{depth}.json"))
}

/// Write a scan cache, saying so rather than failing quietly: a disk that
/// filled up between the check and the write is the case this reports.
pub fn save_cache(cache: &Cached) -> Result<PathBuf, String> {
    let path = cache_path(&cache.game, &cache.depth);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string(cache).map_err(|e| e.to_string())?;
    // Straight to the real name: a part file would double the room needed.
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn load_cache(game: &str, depth: &str) -> Option<Cached> {
    let text = std::fs::read_to_string(cache_path(game, depth)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn drop_cache(game: &str, depth: &str) {
    let _ = std::fs::remove_file(cache_path(game, depth));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caches_move_but_never_land_on_something_already_there() {
        let base = std::env::temp_dir().join(format!("harmony-move-{}", std::process::id()));
        let from = base.join("from");
        let to = base.join("to");
        std::fs::create_dir_all(&from).unwrap();
        std::fs::create_dir_all(&to).unwrap();
        std::fs::write(from.join("scan-t4-quick.json"), "{}").unwrap();
        std::fs::write(from.join("scan-t6-quick.json"), "{}").unwrap();
        std::fs::write(from.join("settings.json"), "{}").unwrap();
        std::fs::write(to.join("scan-t6-quick.json"), "keep me").unwrap();

        assert_eq!(move_caches(&from, &to), 1);
        assert!(to.join("scan-t4-quick.json").exists());
        // The one already at the far end is left alone, and so is its source.
        assert_eq!(std::fs::read_to_string(to.join("scan-t6-quick.json")).unwrap(), "keep me");
        assert!(from.join("scan-t6-quick.json").exists());
        // Only caches move: the settings stay where harmony looks for them.
        assert!(from.join("settings.json").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn moving_a_folder_onto_itself_does_nothing() {
        let here = std::env::temp_dir();
        assert_eq!(move_caches(&here, &here), 0);
    }
}
