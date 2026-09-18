pub mod peaks;
pub mod similarity;
pub mod transient;

use crate::hm::sound::Samples;

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub peak: f32,
    pub rms: f32,
    pub silence: bool,
    pub zero_crossings: usize,
    pub centroid: f32,
}

pub fn stats(samples: &Samples) -> Stats {
    if samples.pcm.is_empty() {
        return Stats {
            silence: true,
            ..Stats::default()
        };
    }
    let mut peak = 0.0f32;
    let mut square = 0.0f64;
    let mut crossings = 0usize;
    let mut previous = 0i16;
    for (i, sample) in samples.pcm.iter().copied().enumerate() {
        let value = sample as f32 / 32768.0;
        peak = peak.max(value.abs());
        square += (value * value) as f64;
        if i > 0 && ((previous < 0) != (sample < 0)) {
            crossings += 1;
        }
        previous = sample;
    }
    let rms = (square / samples.pcm.len() as f64).sqrt() as f32;
    Stats {
        peak,
        rms,
        silence: peak < 0.0005,
        zero_crossings: crossings,
        centroid: crossings as f32 / samples.seconds().max(0.001) / 2.0,
    }
}

pub fn db(value: f32) -> f32 {
    if value <= 0.0 {
        -120.0
    } else {
        20.0 * value.log10()
    }
}
