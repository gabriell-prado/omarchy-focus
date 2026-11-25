//! Treatments for the noise after it is generated: stereo width and
//! softness. The tone never goes through here: a binaural beat needs each
//! ear's tone untouched.
//!
//! Like the equalizer, every treatment changes the sound's character, not
//! its loudness: a makeup gain, computed from the playing color's spectrum
//! through the treatment's filter with the K-weighting loudness meters use,
//! brings it back to the untreated level. Everything glides over 80 ms, so
//! the sliders can be heard while they are dragged.

use super::noise::{custom_power, gains_for, magnitude2, pass, Coef, State, K_WEIGHTING};
use super::{ms_to_samples, Ramp, Treatment, SAMPLE_RATE_F};
use crate::settings::{NoiseColor, NOISE_BANDS};
use std::f64::consts::{FRAC_1_SQRT_2, TAU};
use std::sync::OnceLock;

const GLIDE_MS: u32 = 80;
/// The softness filter is recomputed every 10 ms while it glides.
const UPDATE_EVERY: u32 = 480;
/// Never boost a heavily filtered noise by more than this (+18 dB).
const MAX_MAKEUP: f32 = 8.0;

// ------------------------------------------------------------------ width
//
// The two ears start as independent noises. Width mixes them with an
// equal-power rotation: the correlation between the ears becomes 1 − width,
// and each ear keeps its power, so narrowing never changes the loudness.

pub fn width_mix(width: f32) -> (f32, f32) {
    let correlation = (1.0 - width.clamp(0.0, 1.0)) as f64;
    let theta = correlation.asin() / 2.0;
    (theta.cos() as f32, theta.sin() as f32)
}

// ------------------------------------------------------------------ softness
//
// A gentle (12 dB/octave) low-pass whose corner moves from 20 kHz down to
// 2 kHz on a log scale: the way air and distance take the hiss off rain or
// surf.

const SOFT_TOP_HZ: f64 = 20_000.0;
const SOFT_BOTTOM_HZ: f64 = 2_000.0;

pub fn softness_cutoff(softness: f32) -> f64 {
    SOFT_TOP_HZ * (SOFT_BOTTOM_HZ / SOFT_TOP_HZ).powf(softness.clamp(0.0, 1.0) as f64)
}

// ------------------------------------------------------------------ loudness

/// cos/sin of w and 2w at one frequency.
type Trig = (f64, f64, f64, f64);

/// Log-spaced frequencies with their trig terms and K-weighting.
struct Grid {
    points: Vec<(f64, Trig, f64)>,
}

fn grid() -> &'static Grid {
    static GRID: OnceLock<Grid> = OnceLock::new();
    GRID.get_or_init(|| {
        const POINTS: usize = 160;
        let (lo, hi) = (15f64.ln(), 23_500f64.ln());
        let points = (0..POINTS)
            .map(|k| {
                let f = (lo + (hi - lo) * (k as f64 + 0.5) / POINTS as f64).exp();
                let w = TAU * f / SAMPLE_RATE_F;
                let p = (w.cos(), w.sin(), (2.0 * w).cos(), (2.0 * w).sin());
                let k_weight: f64 = K_WEIGHTING.iter().map(|c| magnitude2(c, p)).product();
                (f, p, k_weight)
            })
            .collect();
        Grid { points }
    })
}

/// A color's power per unit of log-frequency (relative), at one grid point.
fn density(color: NoiseColor, gains: &[f32; NOISE_BANDS], f: f64, p: Trig) -> f64 {
    let pink = f / f.max(9.0);
    match color {
        NoiseColor::Off => 0.0,
        NoiseColor::White => f,
        NoiseColor::Pink => pink,
        // The brown generator's leaky integrator: y = (y + 0.02·x) / 1.02.
        NoiseColor::Brown => {
            let (cw, _, _, _) = p;
            let g = 0.02 / 1.02;
            let r = 1.0 / 1.02;
            f * g * g / (1.0 - 2.0 * r * cw + r * r)
        }
        NoiseColor::Custom => pink * custom_power(gains, p),
    }
}

/// The gain that keeps the noise softened at `cutoff_hz` as loud
/// (K-weighted) as untreated. It is applied to the softened signal only, so
/// while softness fades in or out the untouched part is never boosted.
fn makeup_for(cutoff_hz: f64, color: NoiseColor, gains: &[f32; NOISE_BANDS]) -> f32 {
    let soft = pass(false, cutoff_hz, FRAC_1_SQRT_2);
    let (mut before, mut after) = (0f64, 0f64);
    for &(f, p, k) in &grid().points {
        let w = density(color, gains, f, p) * k;
        before += w;
        after += w * magnitude2(&soft, p);
    }
    if after <= 0.0 {
        return 1.0;
    }
    ((before / after).sqrt() as f32).min(MAX_MAKEUP)
}

// ------------------------------------------------------------------ chain

pub struct Chain {
    target: Treatment,
    color: NoiseColor,
    gains: [f32; NOISE_BANDS],
    mix_a: Ramp,
    mix_b: Ramp,
    /// 0 = dry (bypassed), 1 = through the low-pass; fades in and out.
    soft_wet: Ramp,
    soft_hz: f64,
    soft_target_hz: f64,
    soft_coef: Coef,
    soft_state: [State; 2],
    makeup: Ramp,
    countdown: u32,
}

impl Chain {
    pub fn new(t: Treatment, color: NoiseColor, bands: &[f32; NOISE_BANDS]) -> Chain {
        let (a, b) = width_mix(t.width);
        let soft_hz = if t.softness > 0.0 { softness_cutoff(t.softness) } else { SOFT_TOP_HZ };
        let gains = gains_for(bands).0;
        Chain {
            target: t,
            color,
            gains,
            mix_a: Ramp::new(a),
            mix_b: Ramp::new(b),
            soft_wet: Ramp::new(if t.softness > 0.0 { 1.0 } else { 0.0 }),
            soft_hz,
            soft_target_hz: soft_hz,
            soft_coef: pass(false, soft_hz, FRAC_1_SQRT_2),
            soft_state: [State::default(); 2],
            makeup: Ramp::new(makeup_for(soft_hz, color, &gains)),
            countdown: UPDATE_EVERY,
        }
    }

    pub fn set(&mut self, t: Treatment) {
        if t == self.target {
            return;
        }
        let n = ms_to_samples(GLIDE_MS);
        self.target = t;
        let (a, b) = width_mix(t.width);
        self.mix_a.set(a, n);
        self.mix_b.set(b, n);

        if t.softness > 0.0 {
            let hz = softness_cutoff(t.softness);
            // Fading in from dry: start at the new corner directly, with its
            // makeup (the softened branch is still silent).
            if self.soft_wet.value == 0.0 && self.soft_wet.settled() {
                self.soft_hz = hz;
                self.soft_coef = pass(false, hz, FRAC_1_SQRT_2);
                self.soft_state = [State::default(); 2];
                self.makeup.set(makeup_for(hz, self.color, &self.gains), 0);
            }
            // Otherwise the corner glides, and the makeup follows it step by
            // step in glide().
            self.soft_target_hz = hz;
            self.soft_wet.set(1.0, n);
        } else {
            // Fade out at the current corner, with its makeup; no sweep.
            self.soft_target_hz = self.soft_hz;
            self.soft_wet.set(0.0, n);
        }
    }

    /// The source the loudness correction is computed for.
    pub fn set_source(&mut self, color: NoiseColor, bands: &[f32; NOISE_BANDS]) {
        self.color = color;
        self.gains = gains_for(bands).0;
        self.makeup.set(makeup_for(self.soft_target_hz, color, &self.gains), ms_to_samples(GLIDE_MS));
    }

    /// Every 10 ms: move the softness corner a step towards its target, and
    /// its makeup with it, so the level never runs ahead of the filter.
    fn glide(&mut self) {
        if self.soft_hz != self.soft_target_hz {
            let ratio = self.soft_target_hz / self.soft_hz;
            if ratio.ln().abs() <= 1.5f64.ln() {
                self.soft_hz = self.soft_target_hz;
            } else {
                self.soft_hz *= ratio.clamp(1.0 / 1.5, 1.5);
            }
            self.soft_coef = pass(false, self.soft_hz, FRAC_1_SQRT_2);
            self.makeup.set(makeup_for(self.soft_hz, self.color, &self.gains), UPDATE_EVERY);
        }
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        self.countdown -= 1;
        if self.countdown == 0 {
            self.countdown = UPDATE_EVERY;
            self.glide();
        }
        let (a, b) = (self.mix_a.advance(), self.mix_b.advance());
        let mut x = (a * l + b * r) as f64;
        let mut y = (b * l + a * r) as f64;
        let soft = self.soft_wet.advance() as f64;
        let m = self.makeup.advance() as f64;
        if soft > 0.0 {
            let sx = self.soft_state[0].run(&self.soft_coef, x) * m;
            let sy = self.soft_state[1].run(&self.soft_coef, y) * m;
            x += (sx - x) * soft;
            y += (sy - y) * soft;
        }
        (x as f32, y as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::noise::Noise;

    const FLAT: [f32; NOISE_BANDS] = [100.0; NOISE_BANDS];

    fn treated(color: NoiseColor, t: Treatment) -> Noise {
        Noise::new(color, 11, &FLAT, t)
    }

    /// K-weighted level (dB), zero crossings per second and L/R correlation.
    fn measure(n: &mut Noise) -> (f64, f64, f64) {
        let mut k = [[State::default(); 2]; 2];
        let mut weigh = |ch: usize, v: f64| K_WEIGHTING.iter().zip(k[ch].iter_mut()).fold(v, |x, (c, st)| st.run(c, x));
        for _ in 0..24_000 {
            let (l, r) = n.sample();
            weigh(0, l as f64);
            weigh(1, r as f64);
        }
        let count = 48_000 * 8;
        let (mut pk, mut ll, mut rr, mut lr, mut crossings, mut prev) = (0f64, 0f64, 0f64, 0f64, 0usize, 0f32);
        for _ in 0..count {
            let (l, r) = n.sample();
            let (kl, kr) = (weigh(0, l as f64), weigh(1, r as f64));
            pk += kl * kl + kr * kr;
            ll += (l as f64).powi(2);
            rr += (r as f64).powi(2);
            lr += l as f64 * r as f64;
            if prev <= 0.0 && l > 0.0 {
                crossings += 1;
            }
            prev = l;
        }
        (10.0 * (pk / count as f64).log10(), crossings as f64 / 8.0, lr / (ll * rr).sqrt())
    }

    #[test]
    fn defaults_are_bypassed_exactly() {
        let mut plain = Noise::new(NoiseColor::Pink, 5, &FLAT, Treatment::default());
        let mut chain = Chain::new(Treatment::default(), NoiseColor::Pink, &FLAT);
        let mut reference = Noise::new(NoiseColor::Pink, 5, &FLAT, Treatment::default());
        for _ in 0..10_000 {
            let (l, r) = reference.sample();
            assert_eq!(chain.process(l, r), (l, r));
            assert_eq!(plain.sample(), (l, r));
        }
    }

    #[test]
    fn width_sets_the_correlation_and_keeps_the_power() {
        for (w, expect) in [(1.0f32, 0.0f64), (0.5, 0.5), (0.0, 1.0)] {
            let (a, b) = width_mix(w);
            assert!(((a * a + b * b) as f64 - 1.0).abs() < 1e-6, "equal power");
            assert!((2.0 * (a * b) as f64 - expect).abs() < 1e-6, "correlation for width {w}");
        }
        let (_, _, half) = measure(&mut treated(NoiseColor::Pink, Treatment { width: 0.5, ..Treatment::default() }));
        assert!((half - 0.5).abs() < 0.05, "measured correlation at half width: {half}");
        let mut mono = treated(NoiseColor::Pink, Treatment { width: 0.0, ..Treatment::default() });
        for _ in 0..1000 {
            let (l, r) = mono.sample();
            assert!((l - r).abs() < 1e-6, "mono: both ears identical");
        }
    }

    #[test]
    fn softness_darkens_without_changing_loudness() {
        for color in [NoiseColor::White, NoiseColor::Pink, NoiseColor::Brown] {
            let (base, base_rate, _) = measure(&mut treated(color, Treatment::default()));
            let mut last_rate = base_rate;
            for softness in [0.3f32, 0.6, 1.0] {
                let (level, rate, _) = measure(&mut treated(color, Treatment { softness, ..Treatment::default() }));
                assert!((level - base).abs() < 0.5, "{color:?} softness {softness}: {level:.2} vs {base:.2} dB");
                assert!(rate < last_rate, "{color:?}: softer must be darker ({rate} vs {last_rate})");
                last_rate = rate;
            }
        }
    }

    #[test]
    fn changes_glide_without_clicks() {
        let mut n = treated(NoiseColor::White, Treatment::default());
        let steps: Vec<f32> = {
            let mut prev = n.sample().0;
            (0..192_000)
                .map(|i| {
                    match i {
                        48_000 => n.set_treatment(Treatment { softness: 0.9, width: 0.2 }),
                        96_000 => n.set_treatment(Treatment { softness: 0.4, width: 1.0 }),
                        144_000 => n.set_treatment(Treatment::default()),
                        _ => {}
                    }
                    let v = n.sample().0;
                    let step = (v - prev).abs();
                    prev = v;
                    step
                })
                .collect()
        };
        let worst = |r: std::ops::Range<usize>| steps[r].iter().cloned().fold(0f32, f32::max);
        // The untreated (brightest) sound sets the scale of natural jumps.
        let steady = worst(0..48_000).max(worst(160_000..192_000));
        for (start, name) in [(48_000, "on"), (96_000, "change"), (144_000, "off")] {
            let change = worst(start..start + 12_000);
            assert!(change <= steady * 1.1, "{name}: {change} vs steady {steady}");
        }
    }
}
