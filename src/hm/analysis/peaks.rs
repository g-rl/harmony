use crate::hm::sound::Samples;

#[derive(Clone, Debug, Default)]
pub struct Peaks {
    pub bins: Vec<(f32, f32)>,
}

pub fn build(samples: &Samples, width: usize) -> Peaks {
    let width = width.max(1);
    let channels = samples.channels.max(1) as usize;
    let frames = samples.pcm.len() / channels;
    if frames == 0 {
        return Peaks::default();
    }
    let per = (frames as f32 / width as f32).max(1.0);
    let mut bins = Vec::with_capacity(width);
    for bin in 0..width {
        let start = (bin as f32 * per) as usize;
        let end = (((bin + 1) as f32 * per) as usize).min(frames);
        if start >= end {
            bins.push((0.0, 0.0));
            continue;
        }
        let mut low = 0.0f32;
        let mut high = 0.0f32;
        for frame in start..end {
            for channel in 0..channels {
                let value = samples.pcm[frame * channels + channel] as f32 / 32768.0;
                low = low.min(value);
                high = high.max(value);
            }
        }
        bins.push((low, high));
    }
    Peaks { bins }
}

pub fn spectrogram(samples: &Samples, columns: usize, bands: usize) -> Vec<Vec<f32>> {
    let channels = samples.channels.max(1) as usize;
    let frames = samples.pcm.len() / channels;
    if frames == 0 || columns == 0 || bands == 0 {
        return Vec::new();
    }
    let window = (frames / columns).max(64);
    let mut out = Vec::with_capacity(columns);
    for column in 0..columns {
        let start = column * window;
        let end = (start + window).min(frames);
        let mut energy = vec![0.0f32; bands];
        if start >= end {
            out.push(energy);
            continue;
        }
        let mut previous = 0.0f32;
        let mut band_rate = vec![0usize; bands];
        for frame in start..end {
            let value = samples.pcm[frame * channels] as f32 / 32768.0;
            let delta = (value - previous).abs();
            previous = value;
            let band = ((delta * bands as f32 * 4.0) as usize).min(bands - 1);
            energy[band] += value.abs();
            band_rate[band] += 1;
        }
        let count = (end - start) as f32;
        for band in 0..bands {
            energy[band] = (energy[band] / count * 8.0).min(1.0);
        }
        out.push(energy);
    }
    out
}
