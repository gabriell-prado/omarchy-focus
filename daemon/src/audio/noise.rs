//! White, pink and brown noise, and a custom noise shaped band by band.
//! Each ear gets its own generator, which sounds wider than one copied to
//! both and keeps the noise from correlating with a binaural tone. Every
//! color is scaled to the same loudness.

use super::{ms_to_samples, Ramp, SAMPLE_RATE_F};
use crate::settings::{NoiseColor, NOISE_BANDS};
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
            NoiseColor::Pink | NoiseColor::Custom => {
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
        NoiseColor::Pink | NoiseColor::Custom => -15.6,
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
        NoiseColor::Pink | NoiseColor::Custom => table[2],
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
    /// Only for `NoiseColor::Custom`.
    eq: Option<Equalizer>,
}

impl Noise {
    /// `bands` (0–100 per octave) only matter for `NoiseColor::Custom`.
    pub fn new(color: NoiseColor, seed: u64, bands: &[f32; NOISE_BANDS]) -> Noise {
        Noise {
            color,
            left: Channel::new(color, seed.wrapping_mul(0x9E37_79B9).wrapping_add(11)),
            right: Channel::new(color, seed.wrapping_mul(0x85EB_CA6B).wrapping_add(29)),
            scale: scale_for(color),
            eq: (color == NoiseColor::Custom).then(|| Equalizer::new(bands, seed)),
        }
    }

    pub fn set_bands(&mut self, bands: &[f32; NOISE_BANDS]) {
        if let Some(eq) = self.eq.as_mut() {
            eq.set_bands(bands);
        }
    }

    #[inline]
    pub fn sample(&mut self) -> (f32, f32) {
        match self.eq.as_mut() {
            Some(eq) => eq.sample(self.scale),
            None => (self.left.next() * self.scale, self.right.next() * self.scale),
        }
    }
}

// ------------------------------------------------------------------ equalizer
//
// The custom noise is ten octave bands of pink noise, each with its own
// generator, mixed at the levels of the ten sliders. Pink noise has the same
// energy in every octave, so equal sliders are pink, a rising shape is
// brighter (towards white) and a falling one darker (towards brown).
//
// Each band is cut out by 4th-order Butterworth edges halfway between the
// band centres. Butterworth low- and high-pass pairs are power-complementary,
// and independent bands add in power, so all bands together are flat to
// 0.02 dB; a band at zero is silent and takes nothing from its neighbours.
// The filters never change: a slider only moves a band's gain, which glides
// like every other change in the mixer. Only the shape matters: the result is
// scaled back to the calibrated loudness of plain pink.

/// Octave band centres (ISO 266), Hz.
pub const BAND_HZ: [f64; NOISE_BANDS] = [31.5, 63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];
/// Q of the two sections of a 4th-order Butterworth filter.
const BUTTERWORTH4_Q: [f64; 2] = [0.541_196_100_146_197, 1.306_562_964_876_377];
/// The lowest band stops here: below is felt, not heard, and only eats headroom.
const SUBSONIC_HZ: f64 = 20.0;
/// Never boost a thin shape by more than this (+24 dB).
const MAX_MAKEUP: f32 = 16.0;
const GLIDE_MS: u32 = 80;
const MAX_SECTIONS: usize = 5;

/// A slider level (0–100) as an amplitude: square law, like the volume sliders.
fn band_gain(level: f32) -> f32 {
    let v = level.clamp(0.0, 100.0) / 100.0;
    v * v
}

/// The slider shape (0–100 per band) that sounds like a named color.
pub fn shape_for(color: NoiseColor) -> Option<[u32; NOISE_BANDS]> {
    let tilt_db = match color {
        NoiseColor::Pink => 0.0,
        NoiseColor::White => 3.0,  // +3 dB per octave over pink
        NoiseColor::Brown => -3.0, // −3 dB per octave under pink
        _ => return None,
    };
    let db: Vec<f64> = (0..NOISE_BANDS).map(|i| tilt_db * i as f64).collect();
    let max = db.iter().cloned().fold(f64::MIN, f64::max);
    let mut out = [0u32; NOISE_BANDS];
    for (o, d) in out.iter_mut().zip(&db) {
        *o = (100.0 * 10f64.powf((d - max) / 40.0)).round() as u32;
    }
    Some(out)
}

#[derive(Clone, Copy, Default)]
struct Coef {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

/// RBJ cookbook low- or high-pass section.
fn pass(high: bool, fc: f64, q: f64) -> Coef {
    let w0 = std::f64::consts::TAU * fc / SAMPLE_RATE_F;
    let (sin, cos) = w0.sin_cos();
    let alpha = sin / (2.0 * q);
    let a0 = 1.0 + alpha;
    let (b0, b1) = if high { ((1.0 + cos) / 2.0, -(1.0 + cos)) } else { ((1.0 - cos) / 2.0, 1.0 - cos) };
    Coef { b0: b0 / a0, b1: b1 / a0, b2: b0 / a0, a1: -2.0 * cos / a0, a2: (1.0 - alpha) / a0 }
}

/// |H(e^jw)|² of a section, given cos/sin of w and 2w.
fn magnitude2(c: &Coef, (cw, sw, c2w, s2w): (f64, f64, f64, f64)) -> f64 {
    let (nr, ni) = (c.b0 + c.b1 * cw + c.b2 * c2w, -(c.b1 * sw + c.b2 * s2w));
    let (dr, di) = (1.0 + c.a1 * cw + c.a2 * c2w, -(c.a1 * sw + c.a2 * s2w));
    (nr * nr + ni * ni) / (dr * dr + di * di)
}

/// ITU-R BS.1770 K-weighting at 48 kHz (high shelf, then RLB high-pass):
/// how loudness meters, and roughly ears, weigh frequencies.
const K_WEIGHTING: [Coef; 2] = [
    Coef { b0: 1.535_124_859_586_97, b1: -2.691_696_189_406_38, b2: 1.198_392_810_852_85, a1: -1.690_659_293_182_41, a2: 0.732_480_774_215_85 },
    Coef { b0: 1.0, b1: -2.0, b2: 1.0, a1: -1.990_047_454_833_98, a2: 0.990_072_250_366_21 },
];

/// Each band's filter sections, and the share of pink noise's loudness
/// (K-weighted power) that lands in it.
struct Bank {
    sections: [([Coef; MAX_SECTIONS], usize); NOISE_BANDS],
    share: [f64; NOISE_BANDS],
}

fn bank() -> &'static Bank {
    static BANK: OnceLock<Bank> = OnceLock::new();
    BANK.get_or_init(|| {
        let mut sections = [([Coef::default(); MAX_SECTIONS], 0usize); NOISE_BANDS];
        for (b, (coefs, n)) in sections.iter_mut().enumerate() {
            let mut add = |c: Coef| {
                coefs[*n] = c;
                *n += 1;
            };
            if b == 0 {
                add(pass(true, SUBSONIC_HZ, std::f64::consts::FRAC_1_SQRT_2));
            } else {
                let lower = (BAND_HZ[b - 1] * BAND_HZ[b]).sqrt();
                BUTTERWORTH4_Q.iter().for_each(|q| add(pass(true, lower, *q)));
            }
            if b + 1 < NOISE_BANDS {
                let upper = (BAND_HZ[b] * BAND_HZ[b + 1]).sqrt();
                BUTTERWORTH4_Q.iter().for_each(|q| add(pass(false, upper, *q)));
            }
        }
        // Pink noise has equal power per octave, so a band's share is its
        // power response averaged evenly over log-frequency, across the range
        // the pink generator covers (about 9 Hz up to Nyquist), weighted the
        // way loudness is measured, so every shape is as loud as pink.
        const POINTS: usize = 400;
        let (lo, hi) = (9f64.ln(), 23_900f64.ln());
        let mut share = [0f64; NOISE_BANDS];
        for k in 0..POINTS {
            let f = (lo + (hi - lo) * (k as f64 + 0.5) / POINTS as f64).exp();
            let w = std::f64::consts::TAU * f / SAMPLE_RATE_F;
            let p = (w.cos(), w.sin(), (2.0 * w).cos(), (2.0 * w).sin());
            let weight: f64 = K_WEIGHTING.iter().map(|c| magnitude2(c, p)).product();
            for (s, (coefs, n)) in share.iter_mut().zip(&sections) {
                *s += weight * coefs[..*n].iter().map(|c| magnitude2(c, p)).product::<f64>() / POINTS as f64;
            }
        }
        Bank { sections, share }
    })
}

/// Transposed direct form II state.
#[derive(Clone, Copy, Default)]
struct State {
    z1: f64,
    z2: f64,
}

impl State {
    #[inline]
    fn run(&mut self, c: &Coef, x: f64) -> f64 {
        let y = c.b0 * x + self.z1;
        self.z1 = c.b1 * x - c.a1 * y + self.z2;
        self.z2 = c.b2 * x - c.a2 * y;
        y
    }
}

struct Band {
    left: Channel,
    right: Channel,
    left_state: [State; MAX_SECTIONS],
    right_state: [State; MAX_SECTIONS],
    gain: Ramp,
}

pub struct Equalizer {
    bands: Vec<Band>,
    makeup: Ramp,
}

/// Gains for a slider shape, relative to its loudest band, and the makeup
/// that keeps the mix as loud (K-weighted) as all bands at once, which is
/// pink minus the inaudible rumble below 20 Hz: equal sliders get exactly 1.
fn gains_for(levels: &[f32; NOISE_BANDS]) -> ([f32; NOISE_BANDS], f32) {
    let max = levels.iter().cloned().fold(0.0f32, f32::max);
    if max <= 0.0 {
        return ([0.0; NOISE_BANDS], 0.0);
    }
    let gains = levels.map(|l| band_gain(l / max * 100.0));
    let share = &bank().share;
    let full: f64 = share.iter().sum();
    let power: f64 = gains.iter().zip(share).map(|(g, s)| (*g as f64).powi(2) * s).sum::<f64>() / full;
    (gains, ((1.0 / power.max(1e-9).sqrt()) as f32).min(MAX_MAKEUP))
}

impl Equalizer {
    pub fn new(levels: &[f32; NOISE_BANDS], seed: u64) -> Equalizer {
        let (gains, makeup) = gains_for(levels);
        let bands = (0..NOISE_BANDS)
            .map(|b| {
                let s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(b as u64 * 7919);
                Band {
                    left: Channel::new(NoiseColor::Pink, s ^ 0xA5A5),
                    right: Channel::new(NoiseColor::Pink, s ^ 0x5A5A_0000),
                    left_state: [State::default(); MAX_SECTIONS],
                    right_state: [State::default(); MAX_SECTIONS],
                    gain: Ramp::new(gains[b]),
                }
            })
            .collect();
        Equalizer { bands, makeup: Ramp::new(makeup) }
    }

    pub fn set_bands(&mut self, levels: &[f32; NOISE_BANDS]) {
        let (gains, makeup) = gains_for(levels);
        let samples = ms_to_samples(GLIDE_MS);
        for (band, g) in self.bands.iter_mut().zip(gains) {
            band.gain.set(g, samples);
        }
        self.makeup.set(makeup, samples);
    }

    /// One stereo sample; `scale` calibrates the pink sources.
    #[inline]
    pub fn sample(&mut self, scale: f32) -> (f32, f32) {
        let bank = bank();
        let (mut l, mut r) = (0f64, 0f64);
        for (band, (coefs, n)) in self.bands.iter_mut().zip(&bank.sections) {
            let g = band.gain.advance();
            if g == 0.0 && band.gain.settled() {
                continue; // a silent band costs nothing
            }
            let (mut x, mut y) = (band.left.next() as f64, band.right.next() as f64);
            for (i, c) in coefs[..*n].iter().enumerate() {
                x = band.left_state[i].run(c, x);
                y = band.right_state[i].run(c, y);
            }
            l += x * g as f64;
            r += y * g as f64;
        }
        let m = (self.makeup.advance() * scale) as f64;
        ((l * m) as f32, (r * m) as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLAT: [f32; NOISE_BANDS] = [100.0; NOISE_BANDS];

    /// Loudness (K-weighted level in dB, as BS.1770 meters measure) and zero
    /// crossings per second.
    fn measure(n: &mut Noise) -> (f32, f64) {
        let mut k = [State::default(); 2];
        let mut weigh = |v: f64| K_WEIGHTING.iter().zip(k.iter_mut()).fold(v, |x, (c, st)| st.run(c, x));
        for _ in 0..48_000 {
            weigh(n.sample().0 as f64);
        }
        let count = 48_000 * 10;
        let mut sum = 0f64;
        let mut crossings = 0usize;
        let mut prev = 0f32;
        for _ in 0..count {
            let v = n.sample().0;
            sum += weigh(v as f64).powi(2);
            if prev <= 0.0 && v > 0.0 {
                crossings += 1;
            }
            prev = v;
        }
        ((20.0 * (sum / count as f64).sqrt().log10()) as f32, crossings as f64 / 10.0)
    }

    /// Raw RMS (dBFS), what the color calibration targets.
    fn rms_db(color: NoiseColor) -> f32 {
        let mut n = Noise::new(color, 7, &FLAT);
        for _ in 0..48_000 {
            n.sample();
        }
        let count = 48_000 * 10;
        let sum: f64 = (0..count).map(|_| (n.sample().0 as f64).powi(2)).sum();
        (20.0 * (sum / count as f64).sqrt().log10()) as f32
    }

    fn shaped(bands: [u32; NOISE_BANDS]) -> (f32, f64) {
        measure(&mut Noise::new(NoiseColor::Custom, 7, &bands.map(|b| b as f32)))
    }

    #[test]
    fn flat_custom_is_pink() {
        let (custom, _) = shaped([100; NOISE_BANDS]);
        let pink = measure(&mut Noise::new(NoiseColor::Pink, 7, &FLAT)).0;
        assert!((custom - pink).abs() < 0.3, "custom {custom} dB vs pink {pink} dB");
    }

    #[test]
    fn shaping_changes_the_color_not_the_loudness() {
        let target = measure(&mut Noise::new(NoiseColor::Pink, 7, &FLAT)).0;
        let (_, pink_rate) = shaped([100; NOISE_BANDS]);
        let shapes = [
            shape_for(NoiseColor::White).unwrap(),
            shape_for(NoiseColor::Brown).unwrap(),
            [100, 100, 100, 100, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 100, 100, 100, 100],
            [0, 0, 0, 0, 0, 100, 0, 0, 0, 0],
        ];
        let mut rates = Vec::new();
        for bands in shapes {
            let (rms, rate) = shaped(bands);
            assert!((rms - target).abs() < 0.5, "{bands:?}: {rms} dB vs {target}");
            rates.push(rate);
        }
        // Zero crossings per second track brightness.
        assert!(rates[0] > pink_rate * 1.5, "white shape is brighter: {} vs {pink_rate}", rates[0]);
        assert!(rates[1] < pink_rate / 1.5, "brown shape is darker: {} vs {pink_rate}", rates[1]);
        assert!(rates[2] < rates[3] / 5.0, "lows only vs highs only: {} vs {}", rates[2], rates[3]);
        assert!((rates[4] - 1000.0).abs() < 250.0, "a lone 1 kHz band sits near 1 kHz: {}", rates[4]);
    }

    #[test]
    fn color_shapes() {
        assert_eq!(shape_for(NoiseColor::Pink), Some([100; NOISE_BANDS]));
        let white = shape_for(NoiseColor::White).unwrap();
        assert_eq!((white[0], white[9]), (21, 100));
        let brown = shape_for(NoiseColor::Brown).unwrap();
        assert_eq!((brown[0], brown[9]), (100, 21));
        assert_eq!(shape_for(NoiseColor::Off), None);
    }

    #[test]
    fn bands_glide_without_clicks() {
        // A click is a jump bigger than the noise makes on its own: compare
        // the largest sample step while the shape changes with the largest in
        // steady sound before and after.
        let mut n = Noise::new(NoiseColor::Custom, 3, &FLAT);
        let steps: Vec<f32> = {
            let mut prev = n.sample().0;
            (0..144_000)
                .map(|i| {
                    if i == 48_000 {
                        n.set_bands(&[0.0, 0.0, 0.0, 0.0, 0.0, 100.0, 100.0, 100.0, 100.0, 100.0]);
                    }
                    let v = n.sample().0;
                    let step = (v - prev).abs();
                    prev = v;
                    step
                })
                .collect()
        };
        let worst = |r: std::ops::Range<usize>| steps[r].iter().cloned().fold(0f32, f32::max);
        let steady = worst(0..48_000).max(worst(60_000..144_000));
        let change = worst(48_000..60_000);
        assert!(change <= steady * 1.1, "change {change} vs steady {steady}");
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
        let mut n = Noise::new(NoiseColor::Pink, 3, &FLAT);
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

