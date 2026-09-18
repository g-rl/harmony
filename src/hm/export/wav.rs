use std::path::Path;

use anyhow::Result;

use crate::hm::sound::Samples;

pub fn write(path: &Path, samples: &Samples) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let spec = hound::WavSpec {
        channels: samples.channels.max(1) as u16,
        sample_rate: samples.rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for sample in &samples.pcm {
        writer.write_sample(*sample)?;
    }
    writer.finalize()?;
    Ok(())
}
