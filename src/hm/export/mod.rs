pub mod drag;
pub mod flac;
pub mod layout;
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
