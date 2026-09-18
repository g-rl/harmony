use crate::hm::analysis::Stats;
use crate::hm::catalog::Entry;

#[derive(Clone, Debug, Default)]
pub struct Print {
    pub seconds: f32,
    pub rms: f32,
    pub peak: f32,
    pub centroid: f32,
    pub channels: u8,
}

pub fn print_of(entry: &Entry, stats: &Stats) -> Print {
    Print {
        seconds: entry.seconds(),
        rms: stats.rms,
        peak: stats.peak,
        centroid: stats.centroid,
        channels: entry.channels,
    }
}

fn shared_prefix(a: &str, b: &str) -> f32 {
    let count = a
        .chars()
        .zip(b.chars())
        .take_while(|(x, y)| x == y)
        .count() as f32;
    count / a.len().max(b.len()).max(1) as f32
}

pub fn distance(left: &Print, right: &Print, left_name: &str, right_name: &str) -> f32 {
    let length = ((left.seconds - right.seconds).abs() / left.seconds.max(0.05)).min(4.0);
    let loud = (left.rms - right.rms).abs() * 6.0;
    let bright = ((left.centroid - right.centroid).abs() / left.centroid.max(50.0)).min(4.0);
    let channels = if left.channels == right.channels {
        0.0
    } else {
        0.5
    };
    let name = 1.0 - shared_prefix(left_name, right_name);
    length * 0.9 + loud * 1.2 + bright * 0.9 + channels + name * 1.4
}
