//! White, pink and brown noise. Each ear gets its own generator, which
//! sounds wider than one copied to both and keeps the noise from correlating
//! with a binaural tone. Every color is scaled to the same loudness.

use crate::settings::NoiseColor;
use std::sync::OnceLock;

/// xorshift64*: fast, good enough for audio noise.
#[derive(Clone, Copy)]
struct Rng(u64);

impl Rng {
    #[inline]
    fn next_f32(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        let r = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        // Top 24 bits → [-1, 1).
        ((r >> 40) as f32 / (1u64 << 23) as f32) - 1.0
    }
}

/// One channel of raw (uncalibrated) noise.
#[derive(Clone, Copy)]
struct Channel {
    rng: Rng,
    color: NoiseColor,
    pink: [f32; 7],
    brown: f32,
}

impl Channel {
    fn new(color: NoiseColor, seed: u64) -> Channel {
        Channel { rng: Rng(seed | 1), color, pink: [0.0; 7], brown: 0.0 }
    }

    #[inline]
    fn next(&mut self) -> f32 {
        let w = self.rng.next_f32();
        match self.color {
            NoiseColor::Off => 0.0,
            NoiseColor::White => w,
            NoiseColor::Pink => {
                // Paul Kellet's refined pink filter (−3 dB/octave within ±0.05 dB).
                let b = &mut self.pink;
                b[0] = 0.99886 * b[0] + w * 0.055_517_9;
                b[1] = 0.99332 * b[1] + w * 0.075_075_9;
                b[2] = 0.96900 * b[2] + w * 0.153_852;
                b[3] = 0.86650 * b[3] + w * 0.310_485_6;
                b[4] = 0.55000 * b[4] + w * 0.532_952_2;
                b[5] = -0.7616 * b[5] - w * 0.016_898;
                let out = b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + w * 0.5362;
                b[6] = w * 0.115_926;
                out
            }
            NoiseColor::Brown => {
                // Leaky integrator: −6 dB/octave, bounded.
                self.brown = (self.brown + 0.02 * w) / 1.02;
                self.brown
            }
        }
    }
}

/// RMS (dBFS) per color for equal loudness: −16 LUFS per channel, −13 LUFS
/// for the stereo pair (measured with ffmpeg's ebur128), the same level the
/// ffmpeg version played. K-weighting lifts highs, so bright white noise
/// needs a lower RMS than dark brown to sound as loud.
fn target_rms_db(color: NoiseColor) -> f32 {
    match color {
        NoiseColor::White => -19.2,
        NoiseColor::Pink => -15.6,
        NoiseColor::Brown => -14.4,
        NoiseColor::Off => -120.0,
    }
}

/// Raw RMS of a color, measured once from a fixed seed.
fn raw_rms(color: NoiseColor) -> f32 {
    static CACHE: OnceLock<[f32; 4]> = OnceLock::new();
    let table = CACHE.get_or_init(|| {
        let mut out = [1.0f32; 4];
        for (i, c) in [NoiseColor::Off, NoiseColor::White, NoiseColor::Pink, NoiseColor::Brown].into_iter().enumerate() {
            if c == NoiseColor::Off {
                continue;
            }
            let mut ch = Channel::new(c, 0x9E37_79B9_7F4A_7C15);
            for _ in 0..48_000 {
                ch.next(); // let the filters settle
            }
            let n = 1 << 19;
            let sum: f64 = (0..n).map(|_| (ch.next() as f64).powi(2)).sum();
            out[i] = (sum / n as f64).sqrt() as f32;
        }
        out
    });
    match color {
        NoiseColor::Off => 1.0,
        NoiseColor::White => table[1],
        NoiseColor::Pink => table[2],
        NoiseColor::Brown => table[3],
    }
}

pub fn scale_for(color: NoiseColor) -> f32 {
    10f32.powf(target_rms_db(color) / 20.0) / raw_rms(color)
}

/// Calibrated stereo noise.
pub struct Noise {
    pub color: NoiseColor,
    left: Channel,
    right: Channel,
    scale: f32,
}

impl Noise {
    pub fn new(color: NoiseColor, seed: u64) -> Noise {
        Noise {
            color,
            left: Channel::new(color, seed.wrapping_mul(0x9E37_79B9).wrapping_add(11)),
            right: Channel::new(color, seed.wrapping_mul(0x85EB_CA6B).wrapping_add(29)),
            scale: scale_for(color),
        }
    }

    #[inline]
    pub fn sample(&mut self) -> (f32, f32) {
        (self.left.next() * self.scale, self.right.next() * self.scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms_db(color: NoiseColor) -> f32 {
        let mut n = Noise::new(color, 7);
        for _ in 0..48_000 {
            n.sample();
        }
        let count = 48_000 * 10;
        let sum: f64 = (0..count).map(|_| (n.sample().0 as f64).powi(2)).sum();
        (20.0 * (sum / count as f64).sqrt().log10()) as f32
    }

    #[test]
    fn colors_hit_their_calibrated_rms() {
        for c in [NoiseColor::White, NoiseColor::Pink, NoiseColor::Brown] {
            let got = rms_db(c);
            assert!((got - target_rms_db(c)).abs() < 0.6, "{c:?}: {got} dB");
        }
    }

    #[test]
    fn channels_are_independent() {
        let mut n = Noise::new(NoiseColor::Pink, 3);
        let (mut lr, mut ll, mut rr) = (0f64, 0f64, 0f64);
        for _ in 0..200_000 {
            let (l, r) = n.sample();
            lr += (l * r) as f64;
            ll += (l * l) as f64;
            rr += (r * r) as f64;
        }
        let correlation = lr / (ll * rr).sqrt();
        assert!(correlation.abs() < 0.1, "correlation {correlation}");
    }
}
