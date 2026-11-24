//! Alert sounds: four synthesised tones (the same shapes the ffmpeg version
//! described as expressions), seven freedesktop sounds, or a file of the
//! user's. Everything becomes a stereo 48 kHz buffer.

use super::SAMPLE_RATE;
use std::f64::consts::{PI, TAU};
use std::fs::File;
use std::path::Path;

pub const ALERT_IDS: &[&str] = &[
    "none", "chime", "bowl", "beep", "rise", "complete", "bell", "message", "alarm", "attention", "login", "phone", "custom",
];

const FREEDESKTOP: &str = "/usr/share/sounds/freedesktop/stereo/";
/// A custom file plays for at most this long.
pub const MAX_SECONDS: f64 = 30.0;

pub fn file_for(id: &str) -> Option<String> {
    let name = match id {
        "complete" => "complete.oga",
        "bell" => "bell.oga",
        "message" => "message.oga",
        "alarm" => "alarm-clock-elapsed.oga",
        "attention" => "window-attention.oga",
        "login" => "service-login.oga",
        "phone" => "phone-incoming-call.oga",
        _ => return None,
    };
    Some(format!("{FREEDESKTOP}{name}"))
}

/// A struck note: a sine plus a quieter octave, decaying from `start`.
fn note(t: f64, freq: f64, start: f64, amp: f64, decay: f64) -> f64 {
    if t < start {
        return 0.0;
    }
    let dt = t - start;
    amp * ((TAU * freq * dt).sin() + 0.35 * (2.0 * TAU * freq * dt).sin()) * (-decay * dt).exp()
}

/// A synthesised alert: mono sample at time `t`, and its length in seconds.
type Synth = (fn(f64) -> f64, f64);

fn synth_fn(id: &str) -> Option<Synth> {
    let f: Synth = match id {
        "chime" => (|t| note(t, 659.25, 0.0, 0.3, 2.8) + note(t, 987.77, 0.3, 0.3, 2.8), 2.8),
        "bowl" => (
            |t| {
                (t * 30.0).min(1.0)
                    * (0.3 * (TAU * 196.0 * t).sin() * (-0.5 * t).exp()
                        + 0.25 * (TAU * 197.3 * t).sin() * (-0.5 * t).exp()
                        + 0.14 * (TAU * 539.0 * t).sin() * (-0.9 * t).exp()
                        + 0.07 * (TAU * 1013.0 * t).sin() * (-1.5 * t).exp())
            },
            6.0,
        ),
        "beep" => (
            |t| {
                let m = t % 0.24;
                if m < 0.13 {
                    0.3 * (TAU * 880.0 * t).sin() * (PI * m / 0.13).sin()
                } else {
                    0.0
                }
            },
            0.72,
        ),
        "rise" => (
            |t| {
                note(t, 523.25, 0.0, 0.2, 3.0) + note(t, 659.25, 0.14, 0.2, 3.0) + note(t, 783.99, 0.28, 0.2, 3.0)
                    + note(t, 1046.5, 0.42, 0.2, 3.0)
            },
            2.6,
        ),
        _ => return None,
    };
    Some(f)
}

pub fn synth(id: &str) -> Option<Vec<f32>> {
    let (f, seconds) = synth_fn(id)?;
    let frames = (seconds * SAMPLE_RATE as f64) as usize;
    let mut out = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        let v = f(i as f64 / SAMPLE_RATE as f64) as f32;
        out.push(v);
        out.push(v);
    }
    fade_out_tail(&mut out, 10);
    Some(out)
}

/// Fade the last `ms` milliseconds so a cut-off end does not click.
fn fade_out_tail(buf: &mut [f32], ms: usize) {
    let frames = buf.len() / 2;
    let n = (SAMPLE_RATE as usize / 1000 * ms).min(frames);
    for i in 0..n {
        let g = i as f32 / n as f32;
        let frame = frames - 1 - i;
        buf[frame * 2] *= g;
        buf[frame * 2 + 1] *= g;
    }
}

/// Decode a sound file (ogg/vorbis, mp3, flac, wav) to stereo 48 kHz.
pub fn decode_file(path: &Path, max_seconds: f64) -> Result<Vec<f32>, String> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
    use symphonia::core::errors::Error;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, stream, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("{}: unsupported audio ({e})", path.display()))?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| format!("{}: no audio track", path.display()))?;
    let track_id = track.id;
    let rate = track.codec_params.sample_rate.unwrap_or(SAMPLE_RATE);
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("{}: {e}", path.display()))?;

    let limit = (max_seconds * rate as f64) as usize;
    let mut stereo: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(Error::ResetRequired) => break,
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(Error::DecodeError(_)) => continue,
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        let spec = *decoded.spec();
        let channels = spec.channels.count().max(1);
        let mut samples = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        samples.copy_interleaved_ref(decoded);
        for frame in samples.samples().chunks_exact(channels) {
            let (l, r) = if channels == 1 { (frame[0], frame[0]) } else { (frame[0], frame[1]) };
            stereo.push(l);
            stereo.push(r);
        }
        if stereo.len() / 2 >= limit {
            stereo.truncate(limit * 2);
            break;
        }
    }
    if stereo.is_empty() {
        return Err(format!("{}: no audio decoded", path.display()));
    }
    let mut out = resample(&stereo, rate, SAMPLE_RATE);
    fade_out_tail(&mut out, 20);
    Ok(out)
}

/// Linear-interpolation resampling of interleaved stereo; plenty for alerts.
fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || from == 0 {
        return input.to_vec();
    }
    let frames_in = input.len() / 2;
    let frames_out = (frames_in as u64 * to as u64 / from as u64) as usize;
    let ratio = from as f64 / to as f64;
    let mut out = Vec::with_capacity(frames_out * 2);
    for i in 0..frames_out {
        let pos = i as f64 * ratio;
        let j = pos as usize;
        let frac = (pos - j as f64) as f32;
        let k = (j + 1).min(frames_in - 1);
        for c in 0..2 {
            let a = input[j * 2 + c];
            let b = input[k * 2 + c];
            out.push(a + (b - a) * frac);
        }
    }
    out
}

/// The buffer for an alert id, or Ok(None) for "none".
pub fn render(id: &str, custom_path: &str, home: &Path) -> Result<Option<Vec<f32>>, String> {
    if id == "none" {
        return Ok(None);
    }
    if let Some(buf) = synth(id) {
        return Ok(Some(buf));
    }
    if let Some(file) = file_for(id) {
        return decode_file(Path::new(&file), MAX_SECONDS).map(Some);
    }
    if id == "custom" {
        let path = expand_home(custom_path, home);
        if path.is_empty() {
            return Err("no custom sound file set".into());
        }
        if !path.starts_with('/') {
            return Err(format!("custom sound path must be absolute or start with ~/: {path}"));
        }
        return decode_file(Path::new(&path), MAX_SECONDS).map(Some);
    }
    Err(format!("unknown alert '{id}'"))
}

pub fn expand_home(path: &str, home: &Path) -> String {
    let p = path.trim();
    if p == "~" {
        home.display().to_string()
    } else if let Some(rest) = p.strip_prefix("~/") {
        home.join(rest).display().to_string()
    } else {
        p.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synth_alerts_render_quietly_ending() {
        for id in ["chime", "bowl", "beep", "rise"] {
            let buf = synth(id).unwrap();
            assert!(buf.len() > 48_000, "{id} too short");
            let peak = buf.iter().fold(0f32, |m, v| m.max(v.abs()));
            assert!(peak > 0.1 && peak <= 1.0, "{id} peak {peak}");
            assert!(buf[buf.len() - 1].abs() < 1e-6, "{id} must end silent");
        }
    }

    #[test]
    fn every_id_resolves() {
        let home = Path::new("/home/u");
        for id in ALERT_IDS {
            let got = render(id, "", home);
            match *id {
                "none" => assert_eq!(got, Ok(None)),
                "custom" => assert!(got.is_err()),
                // freedesktop sounds are only checked when installed
                other if file_for(other).is_some() && !Path::new(&file_for(other).unwrap()).exists() => {}
                _ => assert!(matches!(got, Ok(Some(ref b)) if !b.is_empty()), "{id}: {got:?}"),
            }
        }
        assert!(render("custom", "relative.ogg", home).is_err());
        assert_eq!(expand_home("~/a b.ogg", home), "/home/u/a b.ogg");
    }

    #[test]
    fn resample_keeps_duration() {
        let input = vec![0.5f32; 44_100 * 2];
        assert_eq!(resample(&input, 44_100, 48_000).len(), 48_000 * 2);
    }
}
