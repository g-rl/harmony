use anyhow::{Result, anyhow};
use opus_decoder::OpusDecoder;

pub const FRAME: usize = 960;
pub const RATE: u32 = 48_000;

#[derive(Clone, Copy, Debug)]
pub struct Stream {
    pub seek_table: usize,
    pub packets: usize,
    pub channels: u8,
    pub frames: u64,
}

pub fn walk(raw: &[u8], from: usize) -> Option<usize> {
    let mut at = from;
    let mut packets = 0usize;
    while at + 2 <= raw.len() {
        let len = u16::from_le_bytes(raw[at..at + 2].try_into().unwrap()) as usize;
        if len == 0 || len > 8192 {
            return None;
        }
        at += 2 + len;
        packets += 1;
        if at <= raw.len() && at + 16 >= raw.len() {
            return if packets >= 2 { Some(packets) } else { None };
        }
        if at > raw.len() {
            return None;
        }
    }
    None
}

/// Number of ascending `u32` offsets making up a seek table at `base`.
/// The first offset is always zero and no offset can run past the blob.
fn table_len(raw: &[u8], base: usize) -> Option<usize> {
    let word = |at: usize| -> Option<u32> {
        raw.get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    if word(base)? != 0 {
        return None;
    }
    let mut count = 1usize;
    let mut last = 0u32;
    while let Some(value) = word(base + count * 4) {
        if value <= last || value as usize > raw.len() {
            break;
        }
        last = value;
        count += 1;
    }
    (count >= 2).then_some(count)
}

/// Both shapes a modern kapi sound blob comes in: a 32 byte header then the
/// seek table (packed entries), or the seek table on its own (stored entries).
pub const BASES: [usize; 2] = [32, 0];

pub fn probe(raw: &[u8]) -> Option<Stream> {
    if raw.len() < 0x30 {
        return None;
    }
    for base in BASES {
        let Some(count) = table_len(raw, base) else {
            continue;
        };
        // The table may have picked up a word of packet data; try both lengths.
        for packets in [count, count.saturating_sub(1)] {
            if packets < 2 {
                continue;
            }
            let start = base + packets * 4;
            if start + 3 > raw.len() {
                continue;
            }
            if walk(raw, start) != Some(packets) {
                continue;
            }
            let toc = raw[start + 2];
            return Some(Stream {
                seek_table: start,
                packets,
                channels: if (toc >> 2) & 1 == 1 { 2 } else { 1 },
                frames: (packets * FRAME) as u64,
            });
        }
    }
    None
}
pub fn packets(raw: &[u8], from: usize) -> Packets<'_> {
    Packets { raw, at: from }
}

pub struct Packets<'a> {
    raw: &'a [u8],
    at: usize,
}

impl<'a> Iterator for Packets<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        if self.at + 2 > self.raw.len() {
            return None;
        }
        let len = u16::from_le_bytes(self.raw[self.at..self.at + 2].try_into().unwrap()) as usize;
        self.at += 2;
        if len == 0 || self.at + len > self.raw.len() {
            return None;
        }
        let packet = &self.raw[self.at..self.at + len];
        self.at += len;
        Some(packet)
    }
}

pub fn decode(raw: &[u8], stream: Stream) -> Result<Vec<i16>> {
    let channels = stream.channels as usize;
    let mut decoder =
        OpusDecoder::new(RATE, channels).map_err(|e| anyhow!("opus decoder: {e:?}"))?;
    let mut scratch = vec![0i16; FRAME * 6 * channels];
    let mut pcm: Vec<i16> = Vec::with_capacity(stream.packets * FRAME * channels);

    for packet in packets(raw, stream.seek_table) {
        match decoder.decode(packet, &mut scratch, false) {
            Ok(n) => pcm.extend_from_slice(&scratch[..n * channels]),
            Err(_) => break,
        }
    }
    if pcm.is_empty() {
        return Err(anyhow!("opus produced nothing"));
    }
    Ok(pcm)
}

/// Read a stream's shape from the front of a blob alone.
///
/// The seek table and the first packets sit at the start, so a few kilobytes
/// are enough to tell what a sound is without decompressing all of it. Returns
/// `None` when the table runs past what was handed over, in which case the
/// caller has to read the blob whole and use [`probe`].
pub fn probe_head(head: &[u8]) -> Option<Stream> {
    let word = |at: usize| -> Option<u32> {
        head.get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    for base in BASES {
        let Some(count) = table_len(head, base) else {
            continue;
        };
        // The run of rising words stopped because the head stopped: no verdict.
        if base + (count + 1) * 4 > head.len() {
            return None;
        }
        for packets in [count, count.saturating_sub(1)] {
            if packets < 2 {
                continue;
            }
            let start = base + packets * 4;
            // Each table step is a packet's length plus its two length bytes.
            let agrees = (0..packets.min(3)).all(|i| {
                let (Some(here), Some(next)) = (word(base + i * 4), word(base + (i + 1) * 4))
                else {
                    return false;
                };
                let at = start + here as usize;
                let Some(raw) = head.get(at..at + 2) else {
                    return false;
                };
                u16::from_le_bytes(raw.try_into().unwrap()) as u32 + 2 == next - here
            });
            if !agrees {
                continue;
            }
            let toc = *head.get(start + 2)?;
            return Some(Stream {
                seek_table: start,
                packets,
                channels: if (toc >> 2) & 1 == 1 { 2 } else { 1 },
                frames: (packets * FRAME) as u64,
            });
        }
    }
    None
}
