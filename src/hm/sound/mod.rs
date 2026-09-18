pub mod adpcm;
pub mod decode;
pub mod t5;
pub mod opus;
pub mod pcm;

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Codec {
    Opus,
    Pcm16,
    Pcm24,
    Flac,
    Mp3,
    Xma,
    Adpcm,
    Unknown,
}

impl Codec {
    pub fn label(self) -> &'static str {
        match self {
            Codec::Opus => "opus",
            Codec::Pcm16 => "pcm16",
            Codec::Pcm24 => "pcm24",
            Codec::Flac => "flac",
            Codec::Mp3 => "mp3",
            Codec::Xma => "xma",
            Codec::Adpcm => "adpcm",
            Codec::Unknown => "unknown",
        }
    }
}

/// A codec back from its label, for reading a cached scan.
pub fn codec_of(label: &str) -> Codec {
    [
        Codec::Opus,
        Codec::Pcm16,
        Codec::Pcm24,
        Codec::Flac,
        Codec::Mp3,
        Codec::Xma,
        Codec::Adpcm,
    ]
    .into_iter()
    .find(|codec| codec.label() == label)
    .unwrap_or(Codec::Unknown)
}

impl fmt::Display for Codec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

pub struct Samples {
    pub pcm: Vec<i16>,
    pub rate: u32,
    pub channels: u8,
}

impl Samples {
    pub fn frames(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.pcm.len() / self.channels as usize
        }
    }

    pub fn seconds(&self) -> f32 {
        if self.rate == 0 {
            0.0
        } else {
            self.frames() as f32 / self.rate as f32
        }
    }
}
