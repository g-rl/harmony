use std::path::Path;

use anyhow::{Result, anyhow};
use flacenc::component::BitRepr;
use flacenc::error::Verify;

use crate::hm::sound::Samples;

/// Write a flac file from decoded samples.
///
/// Most of what harmony reads is already flac, and that case never comes
/// through here: a stream that is flac already is copied out byte for byte by
/// `passthrough`, because re-encoding a lossless file is work that changes
/// nothing. This is for the rest — the pcm, the adpcm, the opus — where flac
/// is the one format out that keeps every sample the decoder produced and
/// still takes about half of what a wav does.
pub fn write(path: &Path, samples: &Samples) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let channels = samples.channels.max(1) as usize;
    if samples.pcm.is_empty() {
        return Err(anyhow!("nothing to encode"));
    }
    // flacenc takes its samples as i32, interleaved, the way the decoder
    // already lays them out.
    let wide: Vec<i32> = samples.pcm.iter().map(|sample| *sample as i32).collect();
    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|(_, error)| anyhow!("flac settings: {error}"))?;
    let source = flacenc::source::MemSource::from_samples(&wide, channels, 16, samples.rate as usize);
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|error| anyhow!("flac encode: {error}"))?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|error| anyhow!("flac write: {error}"))?;
    std::fs::write(path, sink.as_slice())?;
    Ok(())
}

/// The bytes of a stream that is already flac, or nothing.
///
/// The sab banks of black ops iii, infinite warfare and black ops 4, and the
/// sound paks of ghosts and advanced warfare, all hold flac as it stands. Those
/// come out untouched.
pub fn passthrough(raw: &[u8]) -> Option<&[u8]> {
    (raw.len() > 4 && &raw[0..4] == b"fLaC").then_some(raw)
}
