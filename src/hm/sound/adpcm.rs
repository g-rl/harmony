//! The two adpcm flavours the older titles store wavs in.
//!
//! World at War ships nearly all of its sound effects as Microsoft adpcm, and
//! ima adpcm turns up here and there, so both are decoded rather than shown as
//! a format harmony cannot play.

const COEFF_1: [i32; 7] = [256, 512, 0, 192, 240, 460, 392];
const COEFF_2: [i32; 7] = [0, -256, 0, 64, 0, -208, -232];
const ADAPT: [i32; 16] = [
    230, 230, 230, 230, 307, 409, 512, 614, 768, 614, 512, 409, 307, 230, 230, 230,
];

fn clamp(value: i32) -> i16 {
    value.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

fn i16at(raw: &[u8], at: usize) -> i32 {
    i16::from_le_bytes([raw[at], raw[at + 1]]) as i32
}

/// Microsoft adpcm, format tag 2. Each block opens with per channel predictors
/// and two primed samples, then four bits a sample the rest of the way.
pub fn ms(raw: &[u8], channels: u8, block_align: u16) -> Vec<i16> {
    let channels = channels.max(1) as usize;
    let block = block_align.max(1) as usize;
    let head = 7 * channels;
    let mut out: Vec<i16> = Vec::with_capacity(raw.len() * 2);

    for chunk in raw.chunks(block) {
        if chunk.len() < head {
            break;
        }
        let mut coefficient = [0usize; 2];
        let mut delta = [0i32; 2];
        let mut first = [0i32; 2];
        let mut second = [0i32; 2];

        for channel in 0..channels {
            coefficient[channel] = (chunk[channel] as usize).min(COEFF_1.len() - 1);
        }
        let mut at = channels;
        for channel in 0..channels {
            delta[channel] = i16at(chunk, at);
            at += 2;
        }
        for channel in 0..channels {
            first[channel] = i16at(chunk, at);
            at += 2;
        }
        for channel in 0..channels {
            second[channel] = i16at(chunk, at);
            at += 2;
        }

        for channel in 0..channels {
            out.push(clamp(second[channel]));
        }
        for channel in 0..channels {
            out.push(clamp(first[channel]));
        }

        let mut channel = 0usize;
        for byte in &chunk[at..] {
            for nibble in [byte >> 4, byte & 0x0F] {
                let signed = if nibble >= 8 {
                    nibble as i32 - 16
                } else {
                    nibble as i32
                };
                let coefficients = coefficient[channel];
                let predicted = (first[channel] * COEFF_1[coefficients]
                    + second[channel] * COEFF_2[coefficients])
                    / 256
                    + signed * delta[channel];
                let sample = clamp(predicted);
                second[channel] = first[channel];
                first[channel] = sample as i32;
                delta[channel] = (ADAPT[nibble as usize] * delta[channel]) / 256;
                if delta[channel] < 16 {
                    delta[channel] = 16;
                }
                out.push(sample);
                channel = (channel + 1) % channels;
            }
        }
    }
    out
}

const IMA_STEP: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];

const IMA_INDEX: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

/// Ima adpcm, format tag 0x11: one predictor and one step index per channel per
/// block, then eight sample words at a time per channel.
pub fn ima(raw: &[u8], channels: u8, block_align: u16) -> Vec<i16> {
    let channels = channels.max(1) as usize;
    let block = block_align.max(1) as usize;
    let mut out: Vec<i16> = Vec::with_capacity(raw.len() * 2);

    for chunk in raw.chunks(block) {
        if chunk.len() < 4 * channels {
            break;
        }
        let mut predictor = [0i32; 2];
        let mut index = [0i32; 2];
        for channel in 0..channels {
            predictor[channel] = i16at(chunk, channel * 4);
            index[channel] = (chunk[channel * 4 + 2] as i32).clamp(0, 88);
            out.push(clamp(predictor[channel]));
        }

        let body = &chunk[4 * channels..];
        // Samples come in runs of four bytes a channel, channels in turn.
        let run = 4;
        let mut at = 0usize;
        while at + run * channels <= body.len() {
            for channel in 0..channels {
                let group = &body[at + channel * run..at + channel * run + run];
                for byte in group {
                    for nibble in [byte & 0x0F, byte >> 4] {
                        let step = IMA_STEP[index[channel] as usize];
                        let mut difference = step >> 3;
                        if nibble & 1 != 0 {
                            difference += step >> 2;
                        }
                        if nibble & 2 != 0 {
                            difference += step >> 1;
                        }
                        if nibble & 4 != 0 {
                            difference += step;
                        }
                        if nibble & 8 != 0 {
                            predictor[channel] -= difference;
                        } else {
                            predictor[channel] += difference;
                        }
                        predictor[channel] = predictor[channel].clamp(-32768, 32767);
                        index[channel] =
                            (index[channel] + IMA_INDEX[nibble as usize]).clamp(0, 88);
                        out.push(clamp(predictor[channel]));
                    }
                }
            }
            at += run * channels;
        }
    }
    out
}

/// How many frames a block of this size holds, for the two flavours.
pub fn frames_per_block(format: u16, channels: u8, block_align: u16) -> u64 {
    let channels = channels.max(1) as u64;
    let block = block_align as u64;
    match format {
        2 if block > 7 * channels => (block - 7 * channels) * 2 / channels + 2,
        0x11 if block > 4 * channels => (block - 4 * channels) * 2 / channels + 1,
        _ => 0,
    }
}
