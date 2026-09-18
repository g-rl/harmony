use anyhow::{Result, anyhow};

use crate::hm::sound::{Codec, Samples, adpcm, opus, pcm, t5};

#[derive(Clone, Copy, Debug)]
pub struct Description {
    pub codec: Codec,
    pub rate: u32,
    pub channels: u8,
    pub data_at: usize,
}

impl Default for Description {
    fn default() -> Description {
        Description {
            codec: Codec::Opus,
            rate: opus::RATE,
            channels: 1,
            data_at: 0,
        }
    }
}

/// A wav header, as far as it can be read from the front of a file: the format
/// chunk and where the samples start. Nothing here reads the samples.
#[derive(Clone, Copy, Debug)]
pub struct Riff {
    pub rate: u32,
    pub channels: u8,
    pub bits: u16,
    pub format: u16,
    pub block_align: u16,
    pub data_at: usize,
    pub data_len: u64,
}

impl Riff {
    pub fn frames(&self) -> u64 {
        match self.format {
            2 | 0x11 => {
                let per_block = adpcm::frames_per_block(self.format, self.channels, self.block_align);
                if self.block_align == 0 {
                    return 0;
                }
                self.data_len / self.block_align as u64 * per_block
            }
            _ => {
                let width = (self.bits as u64 / 8).max(1) * self.channels.max(1) as u64;
                self.data_len / width
            }
        }
    }

    /// Which decoder these samples need.
    pub fn codec(&self) -> Codec {
        match (self.format, self.bits) {
            (2, _) | (0x11, _) => Codec::Adpcm,
            (0x55, _) => Codec::Mp3,
            (_, 24) => Codec::Pcm24,
            _ => Codec::Pcm16,
        }
    }
}

/// Walk a wav's chunks for `fmt ` and `data`. Works on a head of the file: the
/// data chunk's own length is taken from its header, not from what is present.
pub fn riff(raw: &[u8]) -> Option<Riff> {
    if raw.len() < 12 || &raw[0..4] != b"RIFF" || &raw[8..12] != b"WAVE" {
        return None;
    }
    let word = |at: usize| -> Option<u32> { Some(u32::from_le_bytes(raw.get(at..at + 4)?.try_into().ok()?)) };
    let half = |at: usize| -> Option<u16> { Some(u16::from_le_bytes(raw.get(at..at + 2)?.try_into().ok()?)) };

    let mut at = 12usize;
    let mut found: Option<Riff> = None;
    while at + 8 <= raw.len() {
        let id = &raw[at..at + 4];
        let size = word(at + 4)? as usize;
        let body = at + 8;
        if id == b"fmt " && body + 16 <= raw.len() {
            found = Some(Riff {
                format: half(body)?,
                channels: (half(body + 2)? as u8).max(1),
                rate: word(body + 4)?,
                block_align: half(body + 12)?,
                bits: half(body + 14)?,
                data_at: 0,
                data_len: 0,
            });
        }
        if id == b"data" {
            let mut head = found?;
            head.data_at = body;
            head.data_len = size as u64;
            return Some(head);
        }
        at = body + size + (size & 1);
    }
    // A head short of the data chunk still describes the audio, which is all a
    // catalogue row needs; the length stays unknown until the whole file is in.
    found
}

pub fn sniff(raw: &[u8]) -> Option<Description> {
    if raw.len() > 4 && &raw[0..4] == b"fLaC" {
        let (rate, channels) = flac_shape(raw).unwrap_or((44_100, 1));
        return Some(Description {
            codec: Codec::Flac,
            rate,
            channels,
            data_at: 0,
        });
    }
    if let Some(head) = riff(raw) {
        return Some(Description {
            codec: head.codec(),
            rate: head.rate,
            channels: head.channels,
            data_at: head.data_at,
        });
    }
    // Black ops keeps adpcm behind a header of its own, with no magic to it,
    // so it is asked after the formats that do have one.
    if let Some(head) = t5::head(raw) {
        return Some(Description {
            codec: Codec::Adpcm,
            rate: head.rate,
            channels: head.channels,
            data_at: head.data_at,
        });
    }
    if mp3_head(raw) {
        return Some(Description {
            codec: Codec::Mp3,
            rate: 0,
            channels: 0,
            data_at: 0,
        });
    }
    let stream = opus::probe(raw)?;
    Some(Description {
        codec: Codec::Opus,
        rate: opus::RATE,
        channels: stream.channels,
        data_at: stream.seek_table,
    })
}

/// An id3 tag or an mpeg frame sync at the front is enough to call it an mp3.
fn mp3_head(raw: &[u8]) -> bool {
    if raw.len() > 3 && &raw[0..3] == b"ID3" {
        return true;
    }
    raw.len() > 2 && raw[0] == 0xFF && (raw[1] & 0xE0) == 0xE0
}

/// A flac stream info block built from what a bank says about an entry.
///
/// The sab banks store flac with its header stripped: the bytes begin at a
/// frame. Everything a decoder needs is in the bank's own table, so the header
/// is put back from there and the frames follow it unchanged.
pub fn flac_header(rate: u32, channels: u8, frames: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(0x2A);
    out.extend_from_slice(b"fLaC");
    out.push(0x80); // last metadata block, type stream info
    out.extend_from_slice(&[0, 0, 34]);
    out.extend_from_slice(&0x400u16.to_be_bytes()); // smallest block
    out.extend_from_slice(&0x400u16.to_be_bytes()); // largest block
    out.extend_from_slice(&[0, 0, 0]); // smallest frame, unknown
    out.extend_from_slice(&[0, 0, 0]); // largest frame, unknown
    let packed = ((rate as u64) << 44)
        | ((channels.max(1) as u64 - 1) << 41)
        | (15u64 << 36)
        | (frames & 0x000F_FFFF_FFFF);
    out.extend_from_slice(&packed.to_be_bytes());
    out.extend_from_slice(&[0u8; 16]); // md5, unchecked
    out
}

/// Where the first flac frame starts in a headerless stream: its sync word.
pub fn first_flac_frame(raw: &[u8]) -> usize {
    raw.windows(2)
        .position(|pair| pair == [0xFF, 0xF8])
        .unwrap_or(0)
}

/// A wav header for samples a container holds bare.
pub fn wav_header(rate: u32, channels: u8, bits: u16, data_len: u32) -> Vec<u8> {
    let channels = channels.max(1) as u16;
    let block_align = channels * (bits / 8).max(1);
    let mut out = Vec::with_capacity(44);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // pcm
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * block_align as u32).to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out
}

/// Rate, channel count and bit rate out of an mp3's first frame header, so a
/// catalogue row can be filled in without decoding the file.
pub fn mp3_shape(raw: &[u8]) -> Option<(u32, u8, u32)> {
    const BITRATE_V1: [u32; 16] = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
    ];
    const BITRATE_V2: [u32; 16] = [
        0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0,
    ];
    const RATES: [u32; 3] = [44_100, 48_000, 32_000];

    let mut at = 0usize;
    if raw.len() > 10 && &raw[0..3] == b"ID3" {
        let size = ((raw[6] as usize & 0x7F) << 21)
            | ((raw[7] as usize & 0x7F) << 14)
            | ((raw[8] as usize & 0x7F) << 7)
            | (raw[9] as usize & 0x7F);
        at = 10 + size;
    }
    while at + 4 <= raw.len().min(at + 0x4000) {
        if raw[at] == 0xFF && (raw[at + 1] & 0xE0) == 0xE0 {
            let version = (raw[at + 1] >> 3) & 0x03;
            let bitrate_index = (raw[at + 2] >> 4) as usize;
            let rate_index = ((raw[at + 2] >> 2) & 0x03) as usize;
            let mode = (raw[at + 3] >> 6) & 0x03;
            if rate_index < 3 && bitrate_index > 0 && bitrate_index < 15 {
                let base = RATES[rate_index];
                let rate = match version {
                    3 => base,
                    2 => base / 2,
                    _ => base / 4,
                };
                let bitrate = if version == 3 {
                    BITRATE_V1[bitrate_index]
                } else {
                    BITRATE_V2[bitrate_index]
                };
                let channels = if mode == 3 { 1 } else { 2 };
                return Some((rate, channels, bitrate * 1000));
            }
        }
        at += 1;
    }
    None
}

/// Rate and channel count out of a flac stream info block, without decoding.
pub fn flac_shape(raw: &[u8]) -> Option<(u32, u8)> {
    if raw.len() < 26 || &raw[0..4] != b"fLaC" {
        return None;
    }
    let info = &raw[8..];
    let packed = u32::from_be_bytes(info.get(10..14)?.try_into().ok()?);
    let rate = packed >> 12;
    let channels = ((packed >> 9) & 0x7) as u8 + 1;
    Some((rate, channels))
}

/// The total sample count a flac stream info block declares: thirty six bits,
/// starting in the low nibble of the byte the bit depth ends in.
pub fn flac_frames(raw: &[u8]) -> u64 {
    if raw.len() < 8 + 18 || &raw[0..4] != b"fLaC" {
        return 0;
    }
    let info = &raw[8..];
    ((info[13] as u64 & 0x0F) << 32)
        | (info[14] as u64) << 24
        | (info[15] as u64) << 16
        | (info[16] as u64) << 8
        | info[17] as u64
}

pub fn decode(raw: &[u8], about: Description) -> Result<Samples> {
    match about.codec {
        Codec::Opus => {
            let stream = opus::probe(raw).ok_or_else(|| anyhow!("no opus stream in this blob"))?;
            let pcm = opus::decode(raw, stream)?;
            Ok(Samples {
                pcm,
                rate: opus::RATE,
                channels: stream.channels,
            })
        }
        Codec::Pcm16 => {
            let end = riff(raw)
                .map(|head| (head.data_at as u64 + head.data_len).min(raw.len() as u64) as usize)
                .unwrap_or(raw.len());
            let at = about.data_at.min(end);
            Ok(Samples {
                pcm: pcm::from_s16le(&raw[at..end]),
                rate: about.rate,
                channels: about.channels.max(1),
            })
        }
        Codec::Pcm24 => {
            let end = riff(raw)
                .map(|head| (head.data_at as u64 + head.data_len).min(raw.len() as u64) as usize)
                .unwrap_or(raw.len());
            let at = about.data_at.min(end);
            Ok(Samples {
                pcm: pcm::from_s24le(&raw[at..end]),
                rate: about.rate,
                channels: about.channels.max(1),
            })
        }
        Codec::Adpcm => adpcm_wav(raw),
        Codec::Flac => flac(raw),
        Codec::Mp3 => mp3(raw),
        other => Err(anyhow!("{other} is not decoded yet")),
    }
}

/// An adpcm wav, unpacked according to its own format chunk.
fn adpcm_wav(raw: &[u8]) -> Result<Samples> {
    if let Some(head) = t5::head(raw) {
        return t5_adpcm(raw, head);
    }
    let head = riff(raw).ok_or_else(|| anyhow!("adpcm: no wav header"))?;
    let end = (head.data_at as u64 + head.data_len).min(raw.len() as u64) as usize;
    let body = &raw[head.data_at.min(end)..end];
    let pcm = match head.format {
        2 => adpcm::ms(body, head.channels, head.block_align),
        0x11 => adpcm::ima(body, head.channels, head.block_align),
        other => return Err(anyhow!("adpcm: format {other} is not known")),
    };
    Ok(Samples {
        pcm,
        rate: head.rate,
        channels: head.channels,
    })
}

/// Black ops adpcm: the same microsoft decoder, told where the blocks start
/// and how wide they are by the game's own header rather than by a format
/// chunk. The tail is trimmed because the last block is padded out.
fn t5_adpcm(raw: &[u8], head: t5::Head) -> Result<Samples> {
    let at = head.data_at.min(raw.len());
    let end = (at as u64 + head.data_len()).min(raw.len() as u64) as usize;
    let mut pcm = adpcm::ms(&raw[at..end], head.channels, head.block_align());
    let wanted = head.frames as usize * head.channels.max(1) as usize;
    if pcm.len() > wanted {
        pcm.truncate(wanted);
    }
    if pcm.is_empty() {
        return Err(anyhow!("adpcm: no samples in this sound"));
    }
    Ok(Samples {
        pcm,
        rate: head.rate,
        channels: head.channels,
    })
}

/// Every frame of a flac stream, interleaved. The older titles stream flac, so
/// this is the whole of their playback path.
fn flac(raw: &[u8]) -> Result<Samples> {
    let mut reader = claxon::FlacReader::new(std::io::Cursor::new(raw))
        .map_err(|e| anyhow!("flac: {e}"))?;
    let info = reader.streaminfo();
    let shift = info.bits_per_sample as i32 - 16;
    let mut pcm: Vec<i16> = Vec::new();
    for sample in reader.samples() {
        // A stream carved out of a pak can end mid frame, and the frames read
        // so far are still the sound. Only a stream that gave up nothing at all
        // counts as a failure.
        let Ok(value) = sample else {
            if pcm.is_empty() {
                return Err(anyhow!("flac: no frames could be read"));
            }
            break;
        };
        let scaled = if shift > 0 {
            value >> shift
        } else {
            value << (-shift)
        };
        pcm.push(scaled.clamp(i16::MIN as i32, i16::MAX as i32) as i16);
    }
    Ok(Samples {
        pcm,
        rate: info.sample_rate,
        channels: info.channels.min(255) as u8,
    })
}

/// Mp3, which is what a few of the older titles keep their music in.
fn mp3(raw: &[u8]) -> Result<Samples> {
    use symphonia::core::audio::Signal;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let source = MediaSourceStream::new(Box::new(std::io::Cursor::new(raw.to_vec())), <_>::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3");
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| anyhow!("mp3: {e}"))?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| anyhow!("mp3: no track"))?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| anyhow!("mp3: {e}"))?;

    let mut pcm: Vec<i16> = Vec::new();
    let mut rate = 0u32;
    let mut channels = 0u8;
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        let Ok(decoded) = decoder.decode(&packet) else {
            continue;
        };
        let spec = *decoded.spec();
        rate = spec.rate;
        channels = spec.channels.count().min(255) as u8;
        let mut buffer = decoded.make_equivalent::<i16>();
        decoded.convert(&mut buffer);
        let frames = buffer.frames();
        for frame in 0..frames {
            for channel in 0..channels as usize {
                pcm.push(buffer.chan(channel)[frame]);
            }
        }
    }
    if pcm.is_empty() {
        return Err(anyhow!("mp3: nothing decoded"));
    }
    Ok(Samples {
        pcm,
        rate,
        channels: channels.max(1),
    })
}
