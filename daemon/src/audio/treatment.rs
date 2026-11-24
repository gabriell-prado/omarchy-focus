//! Treatments for the noise after it is generated: stereo width, softness
//! and tape character. The tone never goes through here: a binaural beat
//! needs each ear's tone untouched.
//!
//! Like the equalizer, every treatment changes the sound's character, not
//! its loudness: a makeup gain, computed from the playing color's spectrum
//! through the treatment's filters with the K-weighting loudness meters use,
//! brings it back to the untreated level. Everything glides over 80 ms, so
//! the sliders can be heard while they are dragged.

use super::noise::{custom_power, gains_for, magnitude2, pass, target_rms_db, Coef, State, K_WEIGHTING};
use super::{ms_to_samples, Ramp, Treatment, SAMPLE_RATE_F};
use crate::settings::{NoiseColor, TapeSpeed, NOISE_BANDS};
use std::f64::consts::{FRAC_1_SQRT_2, TAU};
use std::sync::OnceLock;

const GLIDE_MS: u32 = 80;
/// Filters are recomputed every 10 ms while they glide.
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

// ------------------------------------------------------------------ tape
//
// Modelled on how tape machines behave at each speed: below a "head bump"
// in the lows the response falls away, the treble rolls off earlier the
// slower the tape, and peaks are rounded by magnetic saturation. These are
// typical values, not a measurement of one machine.

#[derive(Clone, Copy, Debug, PartialEq)]
struct TapeParams {
    hp_hz: f64,
    bump_hz: f64,
    bump_db: f64,
    lp_hz: f64,
    drive: f64,
}

fn tape_params(speed: TapeSpeed) -> Option<TapeParams> {
    let (hp_hz, bump_hz, bump_db, lp_hz, drive) = match speed {
        TapeSpeed::Off => return None,
        TapeSpeed::Ips30 => (25.0, 100.0, 1.0, 20_000.0, 1.3),
        TapeSpeed::Ips15 => (20.0, 60.0, 1.5, 15_000.0, 1.6),
        TapeSpeed::Ips7_5 => (18.0, 40.0, 2.0, 10_000.0, 2.0),
        TapeSpeed::Ips3_75 => (15.0, 30.0, 2.5, 6_000.0, 2.6),
    };
    Some(TapeParams { hp_hz, bump_hz, bump_db, lp_hz, drive })
}

/// RBJ cookbook peaking section (the head bump).
fn peaking(f0: f64, q: f64, gain_db: f64) -> Coef {
    let a = 10f64.powf(gain_db / 40.0);
    let w0 = TAU * f0 / SAMPLE_RATE_F;
    let (sin, cos) = w0.sin_cos();
    let alpha = sin / (2.0 * q);
    let a0 = 1.0 + alpha / a;
    Coef {
        b0: (1.0 + alpha * a) / a0,
        b1: -2.0 * cos / a0,
        b2: (1.0 - alpha * a) / a0,
        a1: -2.0 * cos / a0,
        a2: (1.0 - alpha / a) / a0,
    }
}

/// High-pass, head bump, (saturation), treble roll-off.
fn tape_coefs(p: &TapeParams) -> [Coef; 3] {
    [pass(true, p.hp_hz, FRAC_1_SQRT_2), peaking(p.bump_hz, 1.0, p.bump_db), pass(false, p.lp_hz, FRAC_1_SQRT_2)]
}

/// Soft saturation with unity gain for small signals: only peaks are rounded.
#[inline]
fn saturate(x: f64, drive: f64) -> f64 {
    (drive * x).tanh() / drive
}

/// How much louder the saturated noise must be made to sound as loud as
/// before: rounding the peaks of a noise with RMS `sigma` (its samples are
/// close to normally distributed) takes a little power away.
fn saturation_makeup(sigma: f64, drive: f64) -> f64 {
    const STEPS: usize = 400;
    let (mut p_in, mut p_out) = (0f64, 0f64);
    for i in 0..STEPS {
        let z = -6.0 + 12.0 * (i as f64 + 0.5) / STEPS as f64;
        let w = (-0.5 * z * z).exp();
        let x = z * sigma;
        p_in += w * x * x;
        p_out += w * saturate(x, drive).powi(2);
    }
    (p_in / p_out).sqrt()
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
fn density(color: NoiseColor, gains: &[f32; NOISE_BANDS], f: f64, p: (f64, f64, f64, f64)) -> f64 {
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

/// The gain that keeps the treated noise as loud (K-weighted) as untreated.
fn makeup_for(t: &Treatment, color: NoiseColor, gains: &[f32; NOISE_BANDS]) -> f32 {
    let soft = (t.softness > 0.0).then(|| pass(false, softness_cutoff(t.softness), FRAC_1_SQRT_2));
    let tape = tape_params(t.tape).map(|p| tape_coefs(&p));
    if soft.is_none() && tape.is_none() {
        return 1.0;
    }
    let (mut before, mut after) = (0f64, 0f64);
    for &(f, p, k) in &grid().points {
        let w = density(color, gains, f, p) * k;
        let mut g = soft.as_ref().map(|c| magnitude2(c, p)).unwrap_or(1.0);
        if let Some(cs) = &tape {
            g *= cs.iter().map(|c| magnitude2(c, p)).product::<f64>();
        }
        before += w;
        after += w * g;
    }
    if after <= 0.0 {
        return 1.0;
    }
    let mut makeup = (before / after).sqrt();
    if let Some(p) = tape_params(t.tape) {
        let sigma = 10f64.powf(target_rms_db(color) as f64 / 20.0);
        makeup *= saturation_makeup(sigma, p.drive);
    }
    (makeup as f32).min(MAX_MAKEUP)
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
    tape: TapeParams,
    tape_target: TapeParams,
    tape_coef: [Coef; 3],
    tape_state: [[State; 3]; 2],
    tape_wet: Ramp,
    makeup: Ramp,
    countdown: u32,
}

impl Chain {
    pub fn new(t: Treatment, color: NoiseColor, bands: &[f32; NOISE_BANDS]) -> Chain {
        let (a, b) = width_mix(t.width);
        let soft_hz = if t.softness > 0.0 { softness_cutoff(t.softness) } else { SOFT_TOP_HZ };
        let tape = tape_params(t.tape).unwrap_or_else(|| tape_params(TapeSpeed::Ips15).expect("15 ips"));
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
            tape,
            tape_target: tape,
            tape_coef: tape_coefs(&tape),
            tape_state: [[State::default(); 3]; 2],
            tape_wet: Ramp::new(if t.tape == TapeSpeed::Off { 0.0 } else { 1.0 }),
            makeup: Ramp::new(makeup_for(&t, color, &gains)),
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
            // Fading in from dry: start at the new corner directly.
            if self.soft_wet.value == 0.0 && self.soft_wet.settled() {
                self.soft_hz = hz;
                self.soft_coef = pass(false, hz, FRAC_1_SQRT_2);
                self.soft_state = [State::default(); 2];
            }
            self.soft_target_hz = hz;
            self.soft_wet.set(1.0, n);
        } else {
            // Fade out at the current corner; no sweep.
            self.soft_target_hz = self.soft_hz;
            self.soft_wet.set(0.0, n);
        }

        match tape_params(t.tape) {
            Some(p) => {
                // Switching on from silence: start at the new speed directly.
                if self.tape_wet.value == 0.0 && self.tape_wet.settled() {
                    self.tape = p;
                    self.tape_coef = tape_coefs(&p);
                }
                self.tape_target = p;
                self.tape_wet.set(1.0, n);
            }
            None => self.tape_wet.set(0.0, n),
        }
        self.makeup.set(makeup_for(&t, self.color, &self.gains), n);
    }

    /// The source the loudness correction is computed for.
    pub fn set_source(&mut self, color: NoiseColor, bands: &[f32; NOISE_BANDS]) {
        self.color = color;
        self.gains = gains_for(bands).0;
        self.makeup.set(makeup_for(&self.target, color, &self.gains), ms_to_samples(GLIDE_MS));
    }

    /// Every 10 ms: move the softness corner and the tape parameters a step
    /// towards their targets.
    fn glide(&mut self) {
        if self.soft_hz != self.soft_target_hz {
            let ratio = self.soft_target_hz / self.soft_hz;
            if (ratio.ln()).abs() <= 1.5f64.ln() {
                self.soft_hz = self.soft_target_hz;
            } else {
                self.soft_hz *= ratio.clamp(1.0 / 1.5, 1.5);
            }
            self.soft_coef = pass(false, self.soft_hz, FRAC_1_SQRT_2);
        }
        if self.tape != self.tape_target {
            let t = &self.tape_target;
            let step = |cur: f64, to: f64| if (to - cur).abs() < 1e-3 * to.abs().max(1.0) { to } else { cur + (to - cur) * 0.35 };
            self.tape = TapeParams {
                hp_hz: step(self.tape.hp_hz, t.hp_hz),
                bump_hz: step(self.tape.bump_hz, t.bump_hz),
                bump_db: step(self.tape.bump_db, t.bump_db),
                lp_hz: step(self.tape.lp_hz, t.lp_hz),
                drive: step(self.tape.drive, t.drive),
            };
            self.tape_coef = tape_coefs(&self.tape);
        }
    }

    #[inline]
    fn tape_channel(&mut self, ch: usize, x: f64) -> f64 {
        let s = &mut self.tape_state[ch];
        let c = &self.tape_coef;
        let v = s[0].run(&c[0], x);
        let v = s[1].run(&c[1], v);
        s[2].run(&c[2], saturate(v, self.tape.drive))
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
        if soft > 0.0 {
            let sx = self.soft_state[0].run(&self.soft_coef, x);
            let sy = self.soft_state[1].run(&self.soft_coef, y);
            x += (sx - x) * soft;
            y += (sy - y) * soft;
        }
        let wet = self.tape_wet.advance() as f64;
        if wet > 0.0 {
            let tx = self.tape_channel(0, x);
            let ty = self.tape_channel(1, y);
            x += (tx - x) * wet;
            y += (ty - y) * wet;
        }
        let m = self.makeup.advance() as f64;
        ((x * m) as f32, (y * m) as f32)
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
    fn softness_and_tape_darken_without_changing_loudness() {
        for color in [NoiseColor::White, NoiseColor::Pink, NoiseColor::Brown] {
            let (base, base_rate, _) = measure(&mut treated(color, Treatment::default()));
            let mut last_rate = base_rate;
            for softness in [0.3f32, 0.6, 1.0] {
                let (level, rate, _) = measure(&mut treated(color, Treatment { softness, ..Treatment::default() }));
                assert!((level - base).abs() < 0.5, "{color:?} softness {softness}: {level:.2} vs {base:.2} dB");
                assert!(rate < last_rate, "{color:?}: softer must be darker ({rate} vs {last_rate})");
                last_rate = rate;
            }
            for tape in [TapeSpeed::Ips30, TapeSpeed::Ips15, TapeSpeed::Ips7_5, TapeSpeed::Ips3_75] {
                let (level, _, _) = measure(&mut treated(color, Treatment { tape, ..Treatment::default() }));
                // Saturation rounds peaks and takes a little level; the filters are compensated.
                assert!((level - base).abs() < 1.0, "{color:?} {tape:?}: {level:.2} vs {base:.2} dB");
            }
        }
        // Slower tape is darker.
        let rates: Vec<f64> = [TapeSpeed::Ips30, TapeSpeed::Ips15, TapeSpeed::Ips7_5, TapeSpeed::Ips3_75]
            .into_iter()
            .map(|tape| measure(&mut treated(NoiseColor::White, Treatment { tape, ..Treatment::default() })).1)
            .collect();
        assert!(rates.windows(2).all(|w| w[1] < w[0]), "{rates:?}");
    }

    #[test]
    fn changes_glide_without_clicks() {
        let mut n = treated(NoiseColor::White, Treatment::default());
        let steps: Vec<f32> = {
            let mut prev = n.sample().0;
            (0..192_000)
                .map(|i| {
                    match i {
                        48_000 => n.set_treatment(Treatment { softness: 0.9, width: 0.2, tape: TapeSpeed::Ips7_5 }),
                        96_000 => n.set_treatment(Treatment { softness: 0.4, width: 1.0, tape: TapeSpeed::Ips30 }),
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
