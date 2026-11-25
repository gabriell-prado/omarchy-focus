use focus::audio::engine::{Mixer, BLOCK_FRAMES};
use focus::audio::{alerts, linear_gain, wav, Mix, SAMPLE_RATE};
use focus::paths::Paths;
use focus::settings::{NoiseColor, Settings, NOISE_BANDS};
use focus::timer::format_clock;
use focus::{daemon, presets};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

const USAGE: &str = "\
omarchy-focus: Pomodoro timer and research-based focus sounds for Omarchy

Usage:
  omarchy-focus toggle | start | pause | reset | skip
  omarchy-focus adjust <minutes>             add (+5) or take away (-5) time now
  omarchy-focus sound [on|off|toggle]        focus sound on/off
  omarchy-focus preset timer <id>            classic | deep | desktime | ultradian
  omarchy-focus preset sound <id>            speech | white | brown | beta | gamma | alpha | iso40
  omarchy-focus set <key> <value>            e.g. set focusMinutes 50, set noiseColor brown
  omarchy-focus alert <id>                   play an alert sound (none stops it)
  omarchy-focus status [--full]              JSON summary (or the full state)
  omarchy-focus daemon                       run the engine (systemd does this)
  omarchy-focus render (--preset <id> | --noise <color> | --alert <id>)
                       [--bands l1,...,l10] [--width 0-100] [--softness 0-100]
                       [--seconds N] --out <file.wav>
                                             offline render, for checks";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("-h" | "--help" | "help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some("-V" | "--version" | "version") => {
            println!("omarchy-focus {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("daemon") => match daemon::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => fail(&e),
        },
        Some("render") => match render(&args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => fail(&e),
        },
        Some(_) => client(&args),
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("omarchy-focus: {message}");
    ExitCode::FAILURE
}

fn usage_error(message: &str) -> ExitCode {
    eprintln!("omarchy-focus: {message}\n\n{USAGE}");
    ExitCode::from(2)
}

fn request_for(args: &[String]) -> Result<Value, String> {
    let arg = |i: usize, what: &str| args.get(i).cloned().ok_or_else(|| format!("missing {what}"));
    Ok(match args[0].as_str() {
        cmd @ ("toggle" | "start" | "pause" | "reset" | "skip" | "status") => json!({ "cmd": cmd }),
        "sound" => match args.get(1).map(String::as_str) {
            None | Some("toggle") => json!({ "cmd": "sound" }),
            Some("on") => json!({ "cmd": "sound", "on": true }),
            Some("off") => json!({ "cmd": "sound", "on": false }),
            Some(other) => return Err(format!("sound takes on, off or toggle, not '{other}'")),
        },
        "alert" => json!({ "cmd": "alert", "id": arg(1, "alert id")? }),
        "adjust" => {
            let raw = arg(1, "minutes (e.g. +5 or -2)")?;
            let minutes: f64 = raw.trim().parse().map_err(|_| format!("not a number of minutes: '{raw}'"))?;
            json!({ "cmd": "adjust", "minutes": minutes })
        }
        "preset" => json!({ "cmd": "preset", "kind": arg(1, "preset kind (timer|sound)")?, "id": arg(2, "preset id")? }),
        "set" => {
            let key = arg(1, "setting name")?;
            let raw = arg(2, "value")?;
            let value = serde_json::from_str::<Value>(&raw).unwrap_or(Value::String(raw));
            json!({ "cmd": "set", "patch": { key: value } })
        }
        other => return Err(format!("unknown command '{other}'")),
    })
}

fn client(args: &[String]) -> ExitCode {
    let request = match request_for(args) {
        Ok(r) => r,
        Err(e) => return usage_error(&e),
    };
    let paths = Paths::from_env();
    let Ok(stream) = UnixStream::connect(&paths.socket) else {
        return fail("the Focus engine is not running (start it with: systemctl --user start omarchy-focus)");
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut writer = match stream.try_clone() {
        Ok(w) => w,
        Err(e) => return fail(&e.to_string()),
    };
    if let Err(e) = writeln!(writer, "{request}") {
        return fail(&e.to_string());
    }
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let reply: Value = loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => return fail("the engine closed the connection"),
            Ok(_) => {}
            Err(e) => return fail(&e.to_string()),
        }
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        if v.get("ok").is_some() {
            break v;
        }
    };
    if reply["ok"] != json!(true) {
        return fail(reply["error"].as_str().unwrap_or("request failed"));
    }
    if args[0] == "status" {
        let state = &reply["state"];
        if args.get(1).map(String::as_str) == Some("--full") {
            println!("{state}");
        } else {
            let summary = json!({
                "phase": state["phase"],
                "running": state["running"],
                "paused": state["paused"],
                "left": format_clock(state["leftMs"].as_f64().unwrap_or(0.0)),
                "leftMs": state["leftMs"],
                "session": state["session"],
                "sessions": state["sessions"],
                "today": state["today"],
                "sound": state["sound"]["playing"],
                "soundPreset": state["soundPreset"],
            });
            println!("{summary}");
        }
    }
    ExitCode::SUCCESS
}

/// Render a sound offline to a WAV file, for loudness and listening checks.
fn render(args: &[String]) -> Result<(), String> {
    let mut seconds = 10.0f64;
    let mut out = None;
    let mut mix = None;
    let mut alert = None;
    let mut bands = [100f32; NOISE_BANDS];
    let mut treatment = focus::audio::Treatment::default();
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1).cloned().ok_or_else(|| format!("{} needs a value", args[i]))?;
        match args[i].as_str() {
            "--seconds" => seconds = value.parse().map_err(|_| "bad --seconds")?,
            "--out" => out = Some(value),
            "--preset" => {
                let mut s = Settings { sound_enabled: true, ..Settings::default() };
                if !presets::apply_sound(&value, &mut s) {
                    return Err(format!("unknown preset '{value}'"));
                }
                mix = Some(Mix::from_settings(&s));
            }
            "--noise" => {
                let color = NoiseColor::parse(&value).ok_or("bad --noise")?;
                mix = Some(Mix { noise: Some(color), noise_gain: 1.0, ..Mix::default() });
            }
            "--alert" => alert = Some(value),
            "--width" => treatment.width = value.parse::<f32>().map_err(|_| "bad --width")?.clamp(0.0, 100.0) / 100.0,
            "--softness" => treatment.softness = value.parse::<f32>().map_err(|_| "bad --softness")?.clamp(0.0, 100.0) / 100.0,
            "--bands" => {
                let levels: Vec<f32> = value.split(',').filter_map(|v| v.trim().parse().ok()).collect();
                bands = levels.try_into().map_err(|_| format!("--bands needs {NOISE_BANDS} comma-separated levels"))?;
            }
            other => return Err(format!("unknown option {other}")),
        }
        i += 2;
    }
    let out = out.ok_or("--out is required")?;
    let mut mixer = Mixer::new();
    if let Some(mut m) = mix {
        m.noise_bands = bands;
        m.treatment = treatment;
        mixer.set_mix(&m);
    }
    if let Some(id) = alert {
        let home = Paths::from_env().home;
        let buf = alerts::render(&id, "", &home)?.ok_or("alert 'none' has no sound")?;
        mixer.play_alert(Arc::new(buf), linear_gain(100));
    }
    let frames = (seconds * SAMPLE_RATE as f64) as usize;
    let mut data = vec![0f32; frames.div_ceil(BLOCK_FRAMES) * BLOCK_FRAMES * 2];
    for block in data.chunks_mut(BLOCK_FRAMES * 2) {
        mixer.render(block);
    }
    data.truncate(frames * 2);
    wav::write(Path::new(&out), &data).map_err(|e| e.to_string())
}
