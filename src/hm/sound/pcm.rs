pub fn from_s16le(raw: &[u8]) -> Vec<i16> {
    raw.chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect()
}

pub fn from_s24le(raw: &[u8]) -> Vec<i16> {
    raw.chunks_exact(3)
        .map(|c| i16::from_le_bytes([c[1], c[2]]))
        .collect()
}

pub fn to_f32(pcm: &[i16]) -> Vec<f32> {
    pcm.iter().map(|s| *s as f32 / 32768.0).collect()
}
