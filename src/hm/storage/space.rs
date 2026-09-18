//! How much room is left where harmony is about to write.
//!
//! Three things harmony writes can be large: the scan cache, an extraction and
//! the scratch folder a drag out of the window uses. All three go where the
//! user pointed them, which may be a small disk, so the room is measured first
//! and the work waits rather than failing halfway through.

use std::path::{Path, PathBuf};

/// Never write a disk down to its last byte. Windows itself misbehaves with no
/// slack left, and a truncated cache is worse than no cache, so everything is
/// measured against this much on top of what the work actually needs.
pub const HEADROOM: u64 = 256 * 1024 * 1024;

/// What one catalogue entry costs in a cache file. Measured rather than
/// guessed: modern warfare 2's 18 549 sounds come to 4.3 MB of json, which is
/// 234 bytes each, and the named titles are the expensive ones.
pub const PER_ENTRY: u64 = 288;

/// A place harmony wants to write, what it needs there and what it has.
#[derive(Clone, Debug)]
pub struct Room {
    pub path: PathBuf,
    pub free: u64,
    pub want: u64,
}

impl Room {
    pub fn needed(&self) -> u64 {
        self.want.saturating_add(HEADROOM)
    }

    pub fn short(&self) -> bool {
        self.free < self.needed()
    }

    /// How much more room would have to appear for the work to go ahead.
    pub fn missing(&self) -> u64 {
        self.needed().saturating_sub(self.free)
    }
}

/// The room at `path`, or `None` when the volume cannot be measured — an
/// unknown answer is not a shortfall, so the work goes ahead.
pub fn room(path: &Path, want: u64) -> Option<Room> {
    Some(Room {
        path: path.to_path_buf(),
        free: free(path)?,
        want,
    })
}

/// The same measurement, reported only when there is not enough.
pub fn shortfall(path: &Path, want: u64) -> Option<Room> {
    room(path, want).filter(Room::short)
}

pub fn cache_size(entries: usize) -> u64 {
    entries as u64 * PER_ENTRY
}

/// The folder a write will land in may not exist yet; the volume it lands on
/// does, so the measurement walks up until something is there.
fn existing(path: &Path) -> Option<PathBuf> {
    let mut here = Some(path);
    while let Some(dir) = here {
        if dir.exists() {
            return Some(dir.to_path_buf());
        }
        here = dir.parent();
    }
    None
}

#[cfg(windows)]
pub fn free(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::core::PCWSTR;

    let dir = existing(path)?;
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut caller = 0u64;
    // What the caller may use, not what the volume holds: a quota can make
    // those different, and the smaller number is the true one.
    unsafe { GetDiskFreeSpaceExW(PCWSTR(wide.as_ptr()), Some(&mut caller), None, None) }.ok()?;
    Some(caller)
}

#[cfg(not(windows))]
pub fn free(path: &Path) -> Option<u64> {
    let _ = existing(path)?;
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shortfall_counts_the_headroom_not_just_the_work() {
        let room = Room {
            path: PathBuf::from("."),
            free: HEADROOM,
            want: 1024,
        };
        assert!(room.short());
        assert_eq!(room.missing(), 1024);
    }

    #[test]
    fn room_to_spare_is_not_short() {
        let room = Room {
            path: PathBuf::from("."),
            free: HEADROOM * 4,
            want: HEADROOM,
        };
        assert!(!room.short());
        assert_eq!(room.missing(), 0);
    }

    #[test]
    fn the_volume_is_measured_through_folders_that_do_not_exist_yet() {
        let deep = std::env::temp_dir().join("harmony-not-here/nor-here");
        assert!(existing(&deep).is_some());
    }

    #[test]
    fn an_impossible_ask_is_short_on_any_real_volume() {
        let here = std::env::temp_dir();
        // Measured against the disk this runs on, so it also proves the
        // measurement itself came back with a number.
        assert!(shortfall(&here, u64::MAX / 2).is_some());
    }

    #[test]
    fn what_fits_is_not_reported() {
        assert!(shortfall(&std::env::temp_dir(), 0).is_none() || free(&std::env::temp_dir()).is_none());
    }
}
