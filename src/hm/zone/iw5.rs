//! Modern Warfare 3's fastfiles, and the sounds inside them.
//!
//! Modern Warfare 3 keeps no audio in its `.iwd` archives at all: everything
//! is inside the fastfiles under `zone\`. A fastfile is a short header and
//! then one zlib stream, and the stream is a zone: a linear dump of the
//! structs the engine loads, with pointers written out as markers saying
//! "the thing I point at follows here".
//!
//! Harmony does not rebuild those structs. It walks the zone for the shape a
//! loaded sound makes — a wave format block, then the marker pair, then the
//! name, then the samples — and takes the ones where every field agrees with
//! every other. That is enough to list a sound, name it and play it, and it
//! costs one pass over the zone instead of a full asset reader.

use std::path::Path;

use anyhow::{Result, anyhow};

/// `IWffu100`: a fastfile that is only compressed.
const PLAIN: &[u8; 8] = b"IWffu100";
/// `IWff0100`: the signed kind the dlc zones use, whose body is encrypted.
const SIGNED: &[u8; 8] = b"IWff0100";
/// The magic and the version, and then a few bytes whose number depends on
/// how old the game is: call of duty 4 and black ops start the stream right
/// after the version at twelve, modern warfare 2 and 3 put nine more bytes in
/// front of it. Rather than keep a table of versions, the stream is found by
/// looking for it.
const BODY_FROM: usize = 12;
const BODY_TO: usize = 64;
const HEADER_LEAST: usize = BODY_FROM;

/// Does a zlib stream start here?
///
/// A zlib header is two bytes: deflate in a 32k window, and a check value that
/// makes the pair divide by thirty-one. Two bytes of anything else passing
/// both tests is rare enough that inflating settles it.
fn zlib_at(raw: &[u8], at: usize) -> bool {
    let Some(pair) = raw.get(at..at + 2) else {
        return false;
    };
    pair[0] & 0x0F == 8 && (u16::from(pair[0]) * 256 + u16::from(pair[1])) % 31 == 0
}

/// One sound inside an inflated zone.
#[derive(Clone, Debug)]
pub struct Sound {
    pub name: String,
    /// Where the samples start in the inflated zone.
    pub at: usize,
    pub bytes: u32,
    pub rate: u32,
    pub channels: u8,
    pub bits: u16,
}

impl Sound {
    pub fn frames(&self) -> u64 {
        let width = (self.bits as u64 / 8).max(1) * self.channels.max(1) as u64;
        self.bytes as u64 / width.max(1)
    }
}

/// Inflate a fastfile into its zone.
pub fn inflate(path: &Path) -> Result<Vec<u8>> {
    let raw = std::fs::read(path)?;
    inflate_bytes(&raw)
}

pub fn inflate_bytes(raw: &[u8]) -> Result<Vec<u8>> {
    if raw.len() < HEADER_LEAST {
        return Err(anyhow!("this file is too short to be a fastfile"));
    }
    let magic = &raw[..8];
    if magic == SIGNED {
        return Err(anyhow!(
            "this fastfile is signed and its body is encrypted, and harmony ships no keys for it"
        ));
    }
    if magic != PLAIN {
        return Err(anyhow!("not a fastfile harmony knows"));
    }
    // The two offsets the games actually use come first, and the rest of the
    // header after them. A stream that inflates to the end is the stream; one
    // that stops part way is kept only if nothing else works, because two
    // bytes of the header can pass for a zlib header and give back a few
    // kilobytes of nonsense before failing.
    let mut best: Vec<u8> = Vec::new();
    for at in [21, 12]
        .into_iter()
        .chain(BODY_FROM..BODY_TO.min(raw.len()))
    {
        if at >= raw.len() || !zlib_at(raw, at) {
            continue;
        }
        let mut out = Vec::new();
        let mut reader = flate2::read::ZlibDecoder::new(&raw[at..]);
        // A zone that ends mid stream still has every sound before the cut in
        // it, so a read error after some output is not a failure.
        match std::io::Read::read_to_end(&mut reader, &mut out) {
            Ok(_) if !out.is_empty() => return Ok(out),
            _ if out.len() > best.len() => best = out,
            _ => {}
        }
    }
    match best.is_empty() {
        true => Err(anyhow!("no zone stream in this fastfile")),
        false => Ok(best),
    }
}

/// The name pointer: a zone writes a pointer it is about to follow with as
/// `-2`, which is how the name says "I am right here". On the sixty-four bit
/// games that is eight bytes; on the thirty-two bit ones it is four, and the
/// shorter one is the front half of the longer, so the long shape is tried
/// first at every marker.
const MARKER: [u8; 4] = [0xFE, 0xFF, 0xFF, 0xFF];
const MARKER_64: [u8; 8] = [0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
/// Wave format block, five words of nothing, then the marker.
const BLOCK_BACK: usize = 20 + 28;
/// The thirty-two bit block: eleven words, ending at the marker.
const BLOCK_BACK_32: usize = 44;

/// Every sound in an inflated zone.
pub fn sounds(zone: &[u8]) -> Vec<Sound> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while let Some(found) = find(zone, &MARKER, at) {
        at = found + 4;
        let wide = zone.get(found..found + 8) == Some(&MARKER_64[..]);
        let sound = match wide {
            true => read_one(zone, found).or_else(|| read_one_32(zone, found)),
            false => read_one_32(zone, found),
        };
        let Some(sound) = sound else {
            continue;
        };
        at = sound.at + sound.bytes as usize;
        out.push(sound);
    }
    out
}

/// The name a marker stands in front of, if it is one.
fn name_at(zone: &[u8], from: usize) -> Option<(String, usize)> {
    let end = zone.get(from..)?.iter().position(|b| *b == 0)? + from;
    if end == from || end - from > 255 {
        return None;
    }
    let name = std::str::from_utf8(zone.get(from..end)?).ok()?;
    if !name.to_ascii_lowercase().ends_with(".wav") {
        return None;
    }
    if name.bytes().any(|b| !(0x20..0x7F).contains(&b)) {
        return None;
    }
    Some((name.to_string(), end + 1))
}

/// One sound in a thirty-two bit zone: call of duty 4 writes eleven words in
/// front of the marker rather than a wave format block and padding.
///
/// `format, name, bytes, rate, bits, channels, samples, align, name` — the
/// name pointer written twice, and the sample count and the block alignment
/// both worked out from the other fields, which is what makes a run of bytes
/// that merely looks like this one easy to throw out.
fn read_one_32(zone: &[u8], marker_at: usize) -> Option<Sound> {
    let (name, at) = name_at(zone, marker_at + 4)?;
    let block_at = marker_at.checked_sub(BLOCK_BACK_32)?;
    let block = zone.get(block_at..marker_at)?;
    let word = |n: usize| -> Option<u32> {
        Some(u32::from_le_bytes(
            block.get(n * 4..n * 4 + 4)?.try_into().ok()?,
        ))
    };

    if word(2)? != 1 {
        return None;
    }
    let bytes = word(4)?;
    let rate = word(5)?;
    let bits = word(6)? as u16;
    let channels = word(7)? as u16;
    let samples = word(8)?;
    let align = word(9)?;

    if !(1..=2).contains(&channels) || bits != 16 {
        return None;
    }
    if !(4_000..=96_000).contains(&rate) {
        return None;
    }
    if align != channels as u32 * bits as u32 / 8 {
        return None;
    }
    // The sample count is the size in sixteen bit samples, both channels
    // together: two fields that have to agree, out of the same struct.
    if samples == 0 || samples * 2 != bytes {
        return None;
    }
    if bytes == 0 || bytes as usize > zone.len().saturating_sub(at) {
        return None;
    }
    // The two name pointers are the same pointer, written either side of the
    // block. Nothing that is not this struct has that by accident.
    if word(3)? != word(10)? {
        return None;
    }

    Some(Sound {
        name,
        at,
        bytes,
        rate,
        channels: channels as u8,
        bits,
    })
}

/// One sound, if the bytes around this marker really describe one.
fn read_one(zone: &[u8], marker_at: usize) -> Option<Sound> {
    let name_at = marker_at + 8;
    let end = zone[name_at..].iter().position(|b| *b == 0)? + name_at;
    if end == name_at || end - name_at > 255 {
        return None;
    }
    let name = std::str::from_utf8(&zone[name_at..end]).ok()?;
    if !name.is_ascii() || !name.to_ascii_lowercase().ends_with(".wav") {
        return None;
    }
    if name.bytes().any(|b| !(0x20..0x7F).contains(&b)) {
        return None;
    }

    let block_at = marker_at.checked_sub(BLOCK_BACK)?;
    let block = zone.get(block_at..block_at + 28)?;
    let half = |at: usize| u16::from_le_bytes([block[at], block[at + 1]]);
    let word = |at: usize| u32::from_le_bytes(block[at..at + 4].try_into().ok().unwrap_or_default());

    let format = half(0);
    let channels = half(2);
    let rate = word(4);
    let per_second = word(8);
    let align = half(12);
    let bits = half(14);
    let bytes = word(24);

    // Every field has to agree with the others. A wave format block is common
    // enough in a zone that this is the only thing separating a sound from a
    // run of bytes that looks like one.
    if format != 1 || !(1..=2).contains(&channels) || bits != 16 {
        return None;
    }
    if !(4_000..=96_000).contains(&rate) {
        return None;
    }
    if align as u32 != channels as u32 * bits as u32 / 8 {
        return None;
    }
    if per_second != rate * align as u32 {
        return None;
    }
    if bytes == 0 || bytes as usize > zone.len().saturating_sub(end + 1) {
        return None;
    }

    Some(Sound {
        name: name.to_string(),
        at: end + 1,
        bytes,
        rate,
        channels: channels as u8,
        bits,
    })
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|at| at + from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A zone with one sound in it, laid out the way the game writes one.
    fn zone_with(name: &str, samples: &[i16], rate: u32, channels: u16) -> Vec<u8> {
        let mut out = vec![0u8; 64];
        let bits = 16u16;
        let align = channels * bits / 8;
        out.extend_from_slice(&1u16.to_le_bytes()); // pcm
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * align as u32).to_le_bytes());
        out.extend_from_slice(&align.to_le_bytes());
        out.extend_from_slice(&bits.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&((samples.len() * 2) as u32).to_le_bytes());
        out.extend_from_slice(&[0u8; 20]);
        out.extend_from_slice(&MARKER_64);
        out.extend_from_slice(name.as_bytes());
        out.push(0);
        for sample in samples {
            out.extend_from_slice(&sample.to_le_bytes());
        }
        out
    }

    #[test]
    fn finds_a_sound() {
        let zone = zone_with("weapons/ak47_fire.wav", &[0, 1, -1, 300], 44_100, 1);
        let found = sounds(&zone);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "weapons/ak47_fire.wav");
        assert_eq!(found[0].rate, 44_100);
        assert_eq!(found[0].channels, 1);
        assert_eq!(found[0].bytes, 8);
        assert_eq!(found[0].frames(), 4);
        let at = found[0].at;
        assert_eq!(&zone[at..at + 2], &0i16.to_le_bytes());
    }

    #[test]
    fn a_block_that_does_not_agree_is_refused() {
        let mut zone = zone_with("weapons/ak47_fire.wav", &[0, 1], 44_100, 1);
        // Break the bytes-per-second field: nothing else changes.
        zone[64 + 8] ^= 0xFF;
        assert!(sounds(&zone).is_empty());
    }

    #[test]
    fn a_signed_fastfile_says_so() {
        let mut raw = SIGNED.to_vec();
        raw.extend_from_slice(&[0u8; 32]);
        let error = inflate_bytes(&raw).unwrap_err().to_string();
        assert!(error.contains("no keys"), "{error}");
    }
}
