use crate::hm::sound::Samples;

#[derive(Clone, Copy, Debug)]
pub struct Marks {
    pub attack: usize,
    pub tail: usize,
    pub silence_head: usize,
    pub silence_tail: usize,
}

pub fn detect(samples: &Samples) -> Option<Marks> {
    let channels = samples.channels.max(1) as usize;
    let frames = samples.pcm.len() / channels;
    if frames == 0 {
        return None;
    }
    let level = |frame: usize| -> f32 {
        (0..channels)
            .map(|c| (samples.pcm[frame * channels + c] as f32 / 32768.0).abs())
            .fold(0.0f32, f32::max)
    };

    let threshold = 0.004f32;
    let mut head = 0usize;
    while head < frames && level(head) < threshold {
        head += 1;
    }
    let mut tail = frames;
    while tail > head && level(tail - 1) < threshold {
        tail -= 1;
    }

    let mut attack = head;
    let mut loudest = 0.0f32;
    let window = (samples.rate as usize / 200).max(8);
    let mut frame = head;
    while frame + window < tail {
        let energy: f32 = (frame..frame + window).map(level).sum();
        if energy > loudest {
            loudest = energy;
            attack = frame;
        }
        frame += window;
    }

    Some(Marks {
        attack,
        tail: tail.saturating_sub(window),
        silence_head: head,
        silence_tail: frames - tail,
    })
}
