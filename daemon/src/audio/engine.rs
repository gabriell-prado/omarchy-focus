//! The mixer and the audio thread.
//!
//! Every change is a ramp, never a cut: a volume change slides in 80 ms, a
//! different sound crossfades over 400 ms, stopping fades out over 250 ms,
//! and a new alert takes over from the previous one in 15 ms. The output
//! stream is opened when there is something to hear and closed 2 s after the
//! last sound ends.

use super::noise::Noise;
use super::output::{Sink, SinkKind};
use super::tone::Tone;
use super::{ms_to_samples, Mix, Ramp, ToneSpec};
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub enum Cmd {
    Mix(Mix),
    Alert { buf: Arc<Vec<f32>>, gain: f32 },
    StopAlert,
    Shutdown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// The focus sound (noise or tone) is being heard.
    pub playing: bool,
    /// The audio output could not be opened; retried every few seconds.
    pub failed: bool,
}

pub const BLOCK_FRAMES: usize = 480; // 10 ms
const XFADE_MS: u32 = 400;
const GAIN_MS: u32 = 80;
const STOP_MS: u32 = 250;
const ALERT_CUT_MS: u32 = 15;
const CLOSE_AFTER: Duration = Duration::from_secs(2);
const RETRY_AFTER: Duration = Duration::from_secs(3);

struct NoiseVoice {
    noise: Noise,
    gain: Ramp,
    retiring: bool,
}

struct ToneVoice {
    tone: Tone,
    spec: ToneSpec,
    gain: Ramp,
    retiring: bool,
}

struct AlertVoice {
    buf: Arc<Vec<f32>>,
    pos: usize,
    gain: f32,
    envelope: Ramp,
}

fn faded_out(r: &Ramp) -> bool {
    r.settled() && r.value == 0.0
}

/// Soft knee above 0.8 so stacked sources never hard-clip.
#[inline]
fn soft_clip(x: f32) -> f32 {
    const KNEE: f32 = 0.8;
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        x.signum() * (KNEE + (1.0 - KNEE) * ((a - KNEE) / (1.0 - KNEE)).tanh())
    }
}

#[derive(Default)]
pub struct Mixer {
    noise: Vec<NoiseVoice>,
    tone: Vec<ToneVoice>,
    alerts: Vec<AlertVoice>,
    seed: u64,
}

impl Mixer {
    pub fn new() -> Mixer {
        Mixer::default()
    }

    pub fn set_mix(&mut self, mix: &Mix) {
        // Noise: same color → change its volume; otherwise crossfade.
        let live = self.noise.iter().position(|v| !v.retiring);
        match (live, mix.noise) {
            (Some(i), Some(color)) if self.noise[i].noise.color == color => {
                self.noise[i].gain.set(mix.noise_gain, ms_to_samples(GAIN_MS));
            }
            (live, wanted) => {
                if let Some(i) = live {
                    let v = &mut self.noise[i];
                    v.retiring = true;
                    v.gain.set(0.0, ms_to_samples(if wanted.is_some() { XFADE_MS } else { STOP_MS }));
                }
                if let Some(color) = wanted {
                    self.seed = self.seed.wrapping_add(1);
                    let mut gain = Ramp::new(0.0);
                    gain.set(mix.noise_gain, ms_to_samples(XFADE_MS));
                    self.noise.push(NoiseVoice { noise: Noise::new(color, self.seed), gain, retiring: false });
                }
            }
        }

        // Tone: same kind → glide to the new frequencies; otherwise crossfade.
        let live = self.tone.iter().position(|v| !v.retiring);
        match (live, mix.tone) {
            (Some(i), Some(spec)) if self.tone[i].spec.mode == spec.mode => {
                let v = &mut self.tone[i];
                if v.spec != spec {
                    v.tone.retarget(spec);
                    v.spec = spec;
                }
                v.gain.set(mix.tone_gain, ms_to_samples(GAIN_MS));
            }
            (live, wanted) => {
                if let Some(i) = live {
                    let v = &mut self.tone[i];
                    v.retiring = true;
                    v.gain.set(0.0, ms_to_samples(if wanted.is_some() { XFADE_MS } else { STOP_MS }));
                }
                if let Some(spec) = wanted {
                    let mut gain = Ramp::new(0.0);
                    gain.set(mix.tone_gain, ms_to_samples(XFADE_MS));
                    self.tone.push(ToneVoice { tone: Tone::new(spec), spec, gain, retiring: false });
                }
            }
        }
    }

    /// One alert at a time: any alert still sounding fades out at once.
    pub fn play_alert(&mut self, buf: Arc<Vec<f32>>, gain: f32) {
        self.stop_alert();
        self.alerts.push(AlertVoice { buf, pos: 0, gain, envelope: Ramp::new(1.0) });
    }

    pub fn stop_alert(&mut self) {
        for a in &mut self.alerts {
            a.envelope.set(0.0, ms_to_samples(ALERT_CUT_MS));
        }
    }

    pub fn render(&mut self, out: &mut [f32]) {
        for frame in out.as_chunks_mut::<2>().0 {
            let (mut l, mut r) = (0.0f32, 0.0f32);
            for v in &mut self.noise {
                let g = v.gain.advance();
                let (a, b) = v.noise.sample();
                l += a * g;
                r += b * g;
            }
            for v in &mut self.tone {
                let g = v.gain.advance();
                let (a, b) = v.tone.sample();
                l += a * g;
                r += b * g;
            }
            for a in &mut self.alerts {
                if a.pos + 1 < a.buf.len() {
                    let g = a.envelope.advance() * a.gain;
                    l += a.buf[a.pos] * g;
                    r += a.buf[a.pos + 1] * g;
                    a.pos += 2;
                }
            }
            frame[0] = soft_clip(l);
            frame[1] = soft_clip(r);
        }
        self.noise.retain(|v| !(v.retiring && faded_out(&v.gain)));
        self.tone.retain(|v| !(v.retiring && faded_out(&v.gain)));
        self.alerts.retain(|a| a.pos + 1 < a.buf.len() && !faded_out(&a.envelope));
    }

    /// A focus sound (not counting alerts or fade-outs) is playing.
    pub fn has_sound(&self) -> bool {
        self.noise.iter().any(|v| !v.retiring) || self.tone.iter().any(|v| !v.retiring)
    }

    /// Anything at all is still sounding.
    pub fn audible(&self) -> bool {
        !(self.noise.is_empty() && self.tone.is_empty() && self.alerts.is_empty())
    }
}

pub struct Engine {
    tx: mpsc::Sender<Cmd>,
}

impl Engine {
    pub fn spawn(kind: SinkKind, on_status: impl Fn(Status) + Send + 'static) -> Engine {
        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name("audio".into())
            .spawn(move || run(kind, rx, on_status))
            .expect("spawn audio thread");
        Engine { tx }
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Shutdown);
    }
}

/// Apply a command; false means shut down.
fn apply(mixer: &mut Mixer, cmd: Cmd) -> bool {
    match cmd {
        Cmd::Mix(mix) => mixer.set_mix(&mix),
        Cmd::Alert { buf, gain } => mixer.play_alert(buf, gain),
        Cmd::StopAlert => mixer.stop_alert(),
        Cmd::Shutdown => return false,
    }
    true
}

fn run(kind: SinkKind, rx: mpsc::Receiver<Cmd>, on_status: impl Fn(Status)) {
    let mut mixer = Mixer::new();
    let mut sink: Option<Box<dyn Sink>> = None;
    let mut status = Status::default();
    let mut silent_since: Option<Instant> = None;
    let mut retry_at: Option<Instant> = None;
    let mut block = vec![0f32; BLOCK_FRAMES * 2];
    let mut report = |next: Status| {
        if next != status {
            status = next;
            on_status(next);
        }
    };

    loop {
        // Nothing to play and no stream open: sleep until told otherwise.
        if !mixer.audible() && sink.is_none() {
            match rx.recv() {
                Ok(cmd) => {
                    if !apply(&mut mixer, cmd) {
                        return;
                    }
                }
                Err(_) => return,
            }
        }
        loop {
            match rx.try_recv() {
                Ok(cmd) => {
                    if !apply(&mut mixer, cmd) {
                        return;
                    }
                }
                Err(TryRecvError::Disconnected) => return,
                Err(TryRecvError::Empty) => break,
            }
        }

        if mixer.audible() {
            silent_since = None;
        } else if silent_since.get_or_insert_with(Instant::now).elapsed() >= CLOSE_AFTER {
            sink = None;
            silent_since = None;
            report(Status::default());
            continue;
        }

        if sink.is_none() {
            let waiting = retry_at.is_some_and(|t| Instant::now() < t);
            if !waiting {
                match kind.open() {
                    Ok(s) => {
                        sink = Some(s);
                        retry_at = None;
                    }
                    Err(e) => {
                        eprintln!("omarchy-focus: {e}");
                        retry_at = Some(Instant::now() + RETRY_AFTER);
                    }
                }
            }
            if sink.is_none() {
                // No device: keep time moving so fades and alerts still end.
                report(Status { playing: false, failed: mixer.has_sound() });
                mixer.render(&mut block);
                match rx.recv_timeout(Duration::from_millis(10)) {
                    Ok(cmd) => {
                        if !apply(&mut mixer, cmd) {
                            return;
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => return,
                    Err(RecvTimeoutError::Timeout) => {}
                }
                continue;
            }
        }

        mixer.render(&mut block);
        if let Some(s) = sink.as_mut() {
            if let Err(e) = s.write(&block) {
                eprintln!("omarchy-focus: {e}");
                sink = None;
                retry_at = Some(Instant::now() + Duration::from_secs(1));
                continue;
            }
        }
        report(Status { playing: mixer.has_sound(), failed: false });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{BeatMode, NoiseColor};

    fn render_ms(m: &mut Mixer, ms: usize) -> Vec<f32> {
        let mut out = vec![0f32; 48 * ms * 2];
        m.render(&mut out);
        out
    }

    fn max_step(buf: &[f32]) -> f32 {
        buf.as_chunks::<2>().0.windows(2).map(|w| (w[1][0] - w[0][0]).abs()).fold(0.0, f32::max)
    }

    #[test]
    fn stop_fades_out_then_goes_quiet() {
        let mut m = Mixer::new();
        m.set_mix(&Mix { noise: Some(NoiseColor::Brown), noise_gain: 1.0, ..Mix::default() });
        render_ms(&mut m, 500);
        assert!(m.has_sound());
        m.set_mix(&Mix::silent());
        assert!(!m.has_sound() && m.audible(), "fading out");
        render_ms(&mut m, 300);
        assert!(!m.audible());
        assert!(render_ms(&mut m, 10).iter().all(|v| *v == 0.0));
    }

    #[test]
    fn volume_change_keeps_the_same_voice() {
        let mut m = Mixer::new();
        m.set_mix(&Mix { noise: Some(NoiseColor::Pink), noise_gain: 0.2, ..Mix::default() });
        render_ms(&mut m, 500);
        m.set_mix(&Mix { noise: Some(NoiseColor::Pink), noise_gain: 0.8, ..Mix::default() });
        assert_eq!(m.noise.len(), 1, "no new generator for a volume change");
        m.set_mix(&Mix { noise: Some(NoiseColor::White), noise_gain: 0.8, ..Mix::default() });
        assert_eq!(m.noise.len(), 2, "color change crossfades");
        render_ms(&mut m, 450);
        assert_eq!(m.noise.len(), 1);
    }

    #[test]
    fn tone_changes_are_click_free() {
        let spec = |hz| ToneSpec { mode: BeatMode::Binaural, beat_hz: hz, carrier_hz: 250.0 };
        let mut m = Mixer::new();
        m.set_mix(&Mix { tone: Some(spec(16.0)), tone_gain: 1.0, ..Mix::default() });
        let mut all = render_ms(&mut m, 500);
        m.set_mix(&Mix { tone: Some(spec(40.0)), tone_gain: 0.3, ..Mix::default() });
        all.extend(render_ms(&mut m, 500));
        m.set_mix(&Mix::silent());
        all.extend(render_ms(&mut m, 400));
        assert!(max_step(&all) < 0.02, "max step {}", max_step(&all));
    }

    #[test]
    fn a_new_alert_replaces_the_old_one() {
        let mut m = Mixer::new();
        m.play_alert(Arc::new(vec![0.5; 48_000 * 2]), 1.0);
        render_ms(&mut m, 100);
        m.play_alert(Arc::new(vec![0.25; 48_000 * 2]), 1.0);
        render_ms(&mut m, 20);
        assert_eq!(m.alerts.len(), 1, "the old alert is gone after its 15 ms fade");
        let tail = render_ms(&mut m, 5);
        assert!((tail[0] - 0.25).abs() < 1e-6);
    }
}
