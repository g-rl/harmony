use std::time::Duration;

use anyhow::{Result, anyhow};
use rodio::{Player, Source, buffer::SamplesBuffer};

use crate::hm::sound::Samples;

pub struct Transport {
    _sink: rodio::MixerDeviceSink,
    player: Player,
    pub volume: f32,
    pub looping: bool,
    pub length: f32,
    pub playing: bool,
}

impl Transport {
    pub fn open() -> Result<Transport> {
        let mut sink = rodio::DeviceSinkBuilder::from_default_device()
            .map_err(|e| anyhow!("no output device: {e}"))?
            .open_sink_or_fallback()
            .map_err(|e| anyhow!("no output device: {e}"))?;
        sink.log_on_drop(false);
        let player = Player::connect_new(sink.mixer());
        Ok(Transport {
            _sink: sink,
            player,
            volume: 0.8,
            looping: false,
            length: 0.0,
            playing: false,
        })
    }

    pub fn play(&mut self, samples: &Samples) {
        self.player.clear();
        let channels = std::num::NonZeroU16::new(samples.channels.max(1) as u16)
            .unwrap_or(std::num::NonZeroU16::new(1).expect("1 is not zero"));
        let rate = std::num::NonZeroU32::new(samples.rate.max(8000))
            .unwrap_or(std::num::NonZeroU32::new(48_000).expect("48000 is not zero"));
        let buffer = SamplesBuffer::new(
            channels,
            rate,
            samples
                .pcm
                .iter()
                .map(|s| *s as f32 / 32768.0)
                .collect::<Vec<f32>>(),
        );
        self.length = samples.seconds();
        if self.looping {
            self.player.append(buffer.repeat_infinite());
        } else {
            self.player.append(buffer);
        }
        self.player.set_volume(self.volume);
        self.player.play();
        self.playing = true;
    }

    pub fn stop(&mut self) {
        self.player.clear();
        self.playing = false;
    }

    pub fn toggle(&mut self) {
        if self.player.is_paused() {
            self.player.play();
            self.playing = true;
        } else {
            self.player.pause();
            self.playing = false;
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.5);
        self.player.set_volume(self.volume);
    }

    pub fn position(&self) -> f32 {
        self.player.get_pos().as_secs_f32()
    }

    pub fn seek(&mut self, seconds: f32) {
        let _ = self
            .player
            .try_seek(Duration::from_secs_f32(seconds.max(0.0)));
    }

    pub fn finished(&self) -> bool {
        self.player.empty()
    }
}
