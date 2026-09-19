pub mod drag;
pub mod flac;
pub mod layout;
pub mod liblog;
pub mod manifest;
pub mod ogg;
pub mod queue;
pub mod wav;

use crate::hm::catalog::Entry;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Wav,
    Flac,
    Ogg,
    Raw,
}

pub const FORMATS: &[Format] = &[
    Format::Wav,
    Format::Flac,
    Format::Ogg,
    Format::Raw,
];

impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Format::Wav => "wav",
            Format::Flac => "flac",
            Format::Ogg => "ogg",
            Format::Raw => "raw",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Wav => "wav",
            Format::Flac => "flac",
            Format::Ogg => "ogg",
            Format::Raw => "bin",
        }
    }

    /// What this format says for itself, under the chips.
    pub fn about(self) -> &'static str {
        match self {
            Format::Wav => "decoded pcm, every sample the decoder made",
            Format::Flac => "lossless, and a copy where the sound is flac already",
            Format::Ogg => "the original opus packets remuxed, opus titles only",
            Format::Raw => "the blob as the container holds it",
        }
    }
}

/// Roughly what these sounds will take on disk in this format. Wav is the
/// only one that can be worked out exactly — decoded pcm is frames by
/// channels by two bytes, plus a header. Ogg remuxes the packets already
/// counted, and raw writes them as they are, so both are about the packed
/// size; a little is added for the container either way.
pub fn estimate(entries: &[Entry], format: Format) -> u64 {
    entries
        .iter()
        .map(|entry| match format {
            Format::Wav => entry.frames * entry.channels.max(1) as u64 * 2 + 44,
            // Flac lands around half of the pcm it is made from, and is the
            // packed size itself where the sound was flac to begin with.
            Format::Flac => entry.frames * entry.channels.max(1) as u64 + 8192,
            Format::Ogg => entry.bytes + entry.bytes / 16 + 4096,
            Format::Raw => entry.bytes,
        })
        .sum()
}

/// Is this sound one of the ones being left out of a library run?
///
/// Two things are matched, because `voice` means both of them: the bucket
/// harmony filed the sound under, and the first folder of the sound's own
/// name. On black ops iii `voice` is a bucket worked out from the name; on the
/// older titles it is a folder the game itself wrote. Leaving out `voice`
/// should mean the same thing either way.
pub fn left_out(skip: &[String], bucket: &str, display: &str) -> bool {
    if skip.is_empty() {
        return false;
    }
    if skip.iter().any(|name| name == bucket) {
        return true;
    }
    let Some((head, _)) = display.split_once('/') else {
        return false;
    };
    let head = head.to_ascii_lowercase();
    skip.iter().any(|name| *name == head)
}

#[derive(Clone, Debug)]
pub struct Options {
    pub format: Format,
    pub layout: layout::Layout,
    pub normalise_names: bool,
    pub skip_duplicates: bool,
    pub write_manifest: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            format: Format::Wav,
            layout: layout::Layout::CategoryPackage,
            normalise_names: true,
            skip_duplicates: true,
            write_manifest: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `voice` is a bucket on the newer titles and a folder on the older ones.
    /// Leaving it out has to mean both, and nothing else.
    #[test]
    fn what_is_left_out() {
        let skip = vec!["voice".to_string()];
        assert!(left_out(&skip, "voice", "_f6a6b431ac13033b"));
        assert!(left_out(&skip, "misc", "voice/ru/hello.wav"));
        assert!(!left_out(&skip, "weapons", "sound/voiceover/x.wav"));
        assert!(!left_out(&[], "voice", "voice/ru/hello.wav"));
    }

    /// A run measured in hours reads as hours.
    #[test]
    fn a_long_run_spells_itself() {
        assert_eq!(liblog::spell(9.0), "0m 09s");
        assert_eq!(liblog::spell(125.0), "2m 05s");
        assert_eq!(liblog::spell(4325.0), "1h 12m 05s");
    }
}
