//! Binaural and isochronic tones. Phases accumulate continuously and
//! frequency changes glide, so changing a setting never clicks.

use super::{ToneSpec, SAMPLE_RATE_F};
use crate::settings::BeatMode;
use std::f64::consts::TAU;

/// Peak amplitude of a tone before its volume.
pub const TONE_AMPLITUDE: f64 = 0.25;
/// Frequency changes glide over 80 ms, like volume changes: short enough
/// that a dragged slider is heard following the hand; the phase stays
/// continuous, so even an instant jump would not click.
const GLIDE_SAMPLES: u32 = 3_840;

#[derive(Clone, Copy)]
struct Glide {
    value: f64,
    target: f64,
    step: f64,
}

impl Glide {
    fn new(v: f64) -> Glide {
        Glide { value: v, target: v, step: 0.0 }
    }
    fn set(&mut self, target: f64) {
        self.target = target;
        self.step = (target - self.value) / GLIDE_SAMPLES as f64;
    }
    #[inline]
    fn next(&mut self) -> f64 {
        if self.value != self.target {
            self.value += self.step;
            if (self.step > 0.0 && self.value >= self.target) || (self.step < 0.0 && self.value <= self.target) {
                self.value = self.target;
            }
        }
        self.value
    }
}

pub struct Tone {
    pub mode: BeatMode,
    beat: Glide,
    carrier: Glide,
    // binaural: left/right oscillators; isochronic: carrier/beat
    phase_a: f64,
    phase_b: f64,
}

impl Tone {
    pub fn new(spec: ToneSpec) -> Tone {
        Tone { mode: spec.mode, beat: Glide::new(spec.beat_hz), carrier: Glide::new(spec.carrier_hz), phase_a: 0.0, phase_b: 0.0 }
    }

    /// Same mode, new frequencies: glide to them.
    pub fn retarget(&mut self, spec: ToneSpec) {
        self.beat.set(spec.beat_hz);
        self.carrier.set(spec.carrier_hz);
    }

    #[inline]
    pub fn sample(&mut self) -> (f32, f32) {
        let beat = self.beat.next();
        let carrier = self.carrier.next();
        match self.mode {
            BeatMode::Binaural => {
                // Each ear gets its own sine; the brainstem hears their difference.
                self.phase_a = (self.phase_a + TAU * (carrier - beat / 2.0) / SAMPLE_RATE_F) % TAU;
                self.phase_b = (self.phase_b + TAU * (carrier + beat / 2.0) / SAMPLE_RATE_F) % TAU;
                ((TONE_AMPLITUDE * self.phase_a.sin()) as f32, (TONE_AMPLITUDE * self.phase_b.sin()) as f32)
            }
            BeatMode::Isochronic => {
                // The carrier pulsed by a raised cosine, so no clicks.
                self.phase_a = (self.phase_a + TAU * carrier / SAMPLE_RATE_F) % TAU;
                self.phase_b = (self.phase_b + TAU * beat / SAMPLE_RATE_F) % TAU;
                let v = TONE_AMPLITUDE * self.phase_a.sin() * (0.5 - 0.5 * self.phase_b.cos());
                (v as f32, v as f32)
            }
            BeatMode::Off => (0.0, 0.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crossings(samples: &[f32]) -> usize {
        samples.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count()
    }

    #[test]
    fn binaural_puts_carrier_minus_and_plus_half_beat_in_each_ear() {
        let mut t = Tone::new(ToneSpec { mode: BeatMode::Binaural, beat_hz: 16.0, carrier_hz: 250.0 });
        let (l, r): (Vec<f32>, Vec<f32>) = (0..48_000 * 4).map(|_| t.sample()).unzip();
        let (fl, fr) = (crossings(&l) as f64 / 4.0, crossings(&r) as f64 / 4.0);
        assert!((fl - 242.0).abs() <= 1.0, "left {fl}");
        assert!((fr - 258.0).abs() <= 1.0, "right {fr}");
    }

    #[test]
    fn frequency_change_does_not_click() {
        let mut t = Tone::new(ToneSpec { mode: BeatMode::Isochronic, beat_hz: 10.0, carrier_hz: 200.0 });
        let mut prev = t.sample().0;
        let mut max_step = 0f32;
        for i in 0..96_000 {
            if i == 24_000 {
                t.retarget(ToneSpec { mode: BeatMode::Isochronic, beat_hz: 40.0, carrier_hz: 500.0 });
            }
            let v = t.sample().0;
            max_step = max_step.max((v - prev).abs());
            prev = v;
        }
        // A 500 Hz sine at 0.25 moves at most 2π·500/48000·0.25 ≈ 0.016 per sample.
        assert!(max_step < 0.02, "max step {max_step}");
    }
}
