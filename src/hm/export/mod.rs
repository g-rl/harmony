pub mod drag;
pub mod layout;
pub mod manifest;
pub mod ogg;
pub mod queue;
pub mod wav;

use crate::hm::catalog::Entry;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Wav,
    Ogg,
    Raw,
}

pub const FORMATS: &[Format] = &[Format::Wav, Format::Ogg, Format::Raw];

impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Format::Wav => "wav",
            Format::Ogg => "ogg",
            Format::Raw => "raw",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Wav => "wav",
            Format::Ogg => "ogg",
            Format::Raw => "bin",
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
            Format::Ogg => entry.bytes + entry.bytes / 16 + 4096,
            Format::Raw => entry.bytes,
        })
        .sum()
}

#[derive(Clone, Debug)]
pub struct Options {
    pub format: Format,
    pub layout: layout::Layout,
    pub preserve_paths: bool,
    pub normalise_names: bool,
    pub skip_duplicates: bool,
    pub write_manifest: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            format: Format::Wav,
            layout: layout::Layout::CategoryPackage,
            preserve_paths: true,
            normalise_names: true,
            skip_duplicates: true,
            write_manifest: false,
        }
    }
}
