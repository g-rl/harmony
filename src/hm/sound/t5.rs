//! Black Ops' own wrapper around ms-adpcm.
//!
//! The `.wav` files inside `main\iw_NN.iwd` are not riff files at all. They
//! open with a 64-byte header of their own, and the samples after it are plain
//! microsoft adpcm at a block alignment of 262 bytes per channel — seven bytes
//! of block header and 255 of nibbles, which is 512 samples a block.
//!
//! Everything here was read off the retail install rather than a spec: the
//! sizes of every file in it agree with `frames / 512 * 262 * channels` to the
//! byte, and the first byte of every block is a coefficient index in 0..=6,
//! which is the whole of the microsoft table.

/// One adpcm block, for one channel.
pub const BLOCK: usize = 262;

/// Samples in one block: two from the block header, then a nibble each.
pub const FRAMES_PER_BLOCK: u64 = 512;

#[derive(Clone, Copy, Debug)]
pub struct Head {
    pub frames: u64,
    pub rate: u32,
    pub channels: u8,
    /// Where the adpcm blocks start, which the header states rather than
    /// implies: it is 2096 in every file seen, but it is a field.
    pub data_at: usize,
    /// Whether the sound is marked as looping, which is what the `_l` at the
    /// end of so many of these names means.
    pub looping: bool,
}

impl Head {
    pub fn block_align(&self) -> u16 {
        BLOCK as u16 * self.channels.max(1) as u16
    }

    /// How long the samples should be, going by the header alone.
    pub fn data_len(&self) -> u64 {
        self.frames.div_ceil(FRAMES_PER_BLOCK) * self.block_align() as u64
    }
}

fn word(raw: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(raw.get(at..at + 4)?.try_into().ok()?))
}

/// The header, if this really is one.
///
/// The checks are deliberately tight. These files have no magic of their own,
/// so the only thing standing between a wrong guess and a lap of noise is that
/// every field has to be sensible at once.
pub fn head(raw: &[u8]) -> Option<Head> {
    if raw.len() < 64 {
        return None;
    }
    // A riff file is not one of these, whatever the rest of the bytes say.
    if &raw[0..4] == b"RIFF" {
        return None;
    }
    if word(raw, 0)? != 1 {
        return None;
    }
    let frames = word(raw, 4)? as u64;
    let rate = word(raw, 8)?;
    let channels = word(raw, 12)?;
    let data_at = word(raw, 16)? as usize;
    // The channel mask: one speaker for mono, two for stereo.
    let mask = word(raw, 32)?;
    let looping = word(raw, 36)? != 0;

    if frames == 0 || frames > 1 << 31 {
        return None;
    }
    if !(4_000..=96_000).contains(&rate) {
        return None;
    }
    if channels != 1 && channels != 2 {
        return None;
    }
    if mask != 1 && mask != 3 {
        return None;
    }
    if !(64..=1 << 20).contains(&data_at) {
        return None;
    }
    Some(Head {
        frames,
        rate,
        channels: channels as u8,
        data_at,
        looping,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(frames: u32, rate: u32, channels: u32) -> Vec<u8> {
        let mut raw = vec![0u8; 64];
        raw[0..4].copy_from_slice(&1u32.to_le_bytes());
        raw[4..8].copy_from_slice(&frames.to_le_bytes());
        raw[8..12].copy_from_slice(&rate.to_le_bytes());
        raw[12..16].copy_from_slice(&channels.to_le_bytes());
        raw[16..20].copy_from_slice(&2096u32.to_le_bytes());
        let mask = if channels == 2 { 3u32 } else { 1u32 };
        raw[32..36].copy_from_slice(&mask.to_le_bytes());
        raw
    }

    #[test]
    fn reads_a_mono_header() {
        let head = head(&fake(61952, 48025, 1)).expect("header");
        assert_eq!(head.frames, 61952);
        assert_eq!(head.rate, 48025);
        assert_eq!(head.channels, 1);
        assert_eq!(head.data_at, 2096);
        assert_eq!(head.block_align(), 262);
        // The install's own file is 31702 bytes of samples.
        assert_eq!(head.data_len(), 31702);
    }

    #[test]
    fn stereo_blocks_are_twice_as_wide() {
        let head = head(&fake(208384, 48059, 2)).expect("header");
        assert_eq!(head.block_align(), 524);
        assert_eq!(head.data_len(), 213268);
    }

    #[test]
    fn a_riff_file_is_not_one_of_these() {
        let mut raw = fake(1024, 48000, 1);
        raw[0..4].copy_from_slice(b"RIFF");
        assert!(head(&raw).is_none());
    }

    #[test]
    fn nonsense_is_refused() {
        let mut raw = fake(1024, 48000, 1);
        raw[8..12].copy_from_slice(&1_000_000u32.to_le_bytes());
        assert!(head(&raw).is_none());
    }
}
