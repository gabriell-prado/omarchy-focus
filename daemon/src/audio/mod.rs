//! In-process sound: noise and tone synthesis, alerts, a mixer with smooth
//! gain ramps and crossfades, and the output stream.

pub mod alerts;
pub mod engine;
pub mod noise;
pub mod output;
pub mod tone;
pub mod treatment;
pub mod wav;

use crate::settings::{BeatMode, NoiseColor, Settings, TapeSpeed, NOISE_BANDS};

pub const SAMPLE_RATE: u32 = 48_000;
pub const SAMPLE_RATE_F: f64 = SAMPLE_RATE as f64;

/// Amplitude for a 0–100 slider: square law, so half the slider is about −12 dB.
pub fn linear_gain(percent: u32) -> f32 {
    let v = percent.min(100) as f32 / 100.0;
    v * v
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToneSpec {
    pub mode: BeatMode,
    pub beat_hz: f64,
    pub carrier_hz: f64,
}

/// How the noise is treated after it is generated (the tone never is).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Treatment {
    /// 0 = mono, 1 = independent ears.
    pub width: f32,
    /// 0 = untouched, 1 = highs rolled off from 2 kHz.
    pub softness: f32,
    pub tape: TapeSpeed,
}

impl Default for Treatment {
    fn default() -> Self {
        Treatment { width: 1.0, softness: 0.0, tape: TapeSpeed::Off }
    }
}

/// What the focus sound should be right now. `None` sources are silent.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Mix {
    pub noise: Option<NoiseColor>,
    pub noise_gain: f32,
    /// Octave band levels (0–100) for `NoiseColor::Custom`.
    pub noise_bands: [f32; NOISE_BANDS],
    pub treatment: Treatment,
    pub tone: Option<ToneSpec>,
    pub tone_gain: f32,
}

impl Mix {
    pub fn silent() -> Mix {
        Mix::default()
    }

    /// The mix the settings describe, assuming the sound should play.
    pub fn from_settings(s: &Settings) -> Mix {
        // A custom noise with every band down is silence, not a flat shape.
        let shaped_silent = s.noise_color == NoiseColor::Custom && s.noise_bands.iter().all(|b| *b == 0);
        let noise = (s.noise_color != NoiseColor::Off && s.noise_volume > 0 && !shaped_silent).then_some(s.noise_color);
        let tone = (s.beat_mode != BeatMode::Off && s.tone_volume > 0).then_some(ToneSpec {
            mode: s.beat_mode,
            beat_hz: s.beat_hz as f64,
            carrier_hz: s.carrier_hz as f64,
        });
        Mix {
            noise,
            noise_gain: if noise.is_some() { linear_gain(s.noise_volume) } else { 0.0 },
            noise_bands: s.noise_bands.map(|b| b as f32),
            treatment: Treatment {
                width: s.noise_width.min(100) as f32 / 100.0,
                softness: s.noise_softness.min(100) as f32 / 100.0,
                tape: s.tape_speed,
            },
            tone,
            tone_gain: if tone.is_some() { linear_gain(s.tone_volume) } else { 0.0 },
        }
    }

    pub fn is_silent(&self) -> bool {
        self.noise.is_none() && self.tone.is_none()
    }
}

/// A value that moves linearly to a target over a number of samples.
#[derive(Clone, Copy, Debug)]
pub struct Ramp {
    pub value: f32,
    pub target: f32,
    step: f32,
}

impl Ramp {
    pub fn new(value: f32) -> Ramp {
        Ramp { value, target: value, step: 0.0 }
    }

    pub fn set(&mut self, target: f32, samples: u32) {
        self.target = target;
        self.step = if samples == 0 { 0.0 } else { (target - self.value) / samples as f32 };
        if samples == 0 {
            self.value = target;
        }
    }

    #[inline]
    pub fn advance(&mut self) -> f32 {
        if self.value != self.target {
            self.value += self.step;
            if (self.step > 0.0 && self.value >= self.target) || (self.step < 0.0 && self.value <= self.target) || self.step == 0.0 {
                self.value = self.target;
            }
        }
        self.value
    }

    pub fn settled(&self) -> bool {
        self.value == self.target
    }
}

pub fn ms_to_samples(ms: u32) -> u32 {
    SAMPLE_RATE / 1000 * ms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramp_reaches_target_exactly() {
        let mut r = Ramp::new(0.0);
        r.set(1.0, 4);
        let v: Vec<f32> = (0..6).map(|_| r.advance()).collect();
        assert_eq!(v, [0.25, 0.5, 0.75, 1.0, 1.0, 1.0]);
        r.set(0.0, 2);
        assert_eq!((r.advance(), r.advance(), r.settled()), (0.5, 0.0, true));
    }

    #[test]
    fn mix_from_settings() {
        let s = Settings { noise_volume: 50, ..Settings::default() };
        let m = Mix::from_settings(&s);
        assert_eq!((m.noise, m.noise_gain, m.tone), (Some(NoiseColor::Pink), 0.25, None));
        let off = Settings { noise_color: NoiseColor::Off, ..Settings::default() };
        assert!(Mix::from_settings(&off).is_silent());
    }
}
