//! End to end: start the real daemon in a temporary home with a silent audio
//! sink and no notifications, drive it with the CLI and a raw socket client.

use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_omarchy-focus");

struct Daemon {
    child: Child,
    root: PathBuf,
}

impl Daemon {
    fn start(name: &str, seed: impl FnOnce(&Path)) -> Daemon {
        let root = std::env::temp_dir().join(format!("focus-it-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for d in ["run", "config", "state", "home"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        seed(&root);
        let child = Command::new(BIN).arg("daemon").envs(env(&root)).spawn().expect("start daemon");
        let d = Daemon { child, root };
        let deadline = Instant::now() + Duration::from_secs(10);
        while UnixStream::connect(d.socket()).is_err() {
            assert!(Instant::now() < deadline, "daemon did not start");
            std::thread::sleep(Duration::from_millis(20));
        }
        d
    }

    fn socket(&self) -> PathBuf {
        self.root.join("run/omarchy-focus.sock")
    }

    fn cli(&self, args: &[&str]) -> Output {
        Command::new(BIN).args(args).envs(env(&self.root)).output().unwrap()
    }

    fn ok(&self, args: &[&str]) {
        let out = self.cli(args);
        assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    fn status(&self) -> Value {
        let out = self.cli(&["status", "--full"]);
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        serde_json::from_slice(&out.stdout).unwrap()
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn env(root: &Path) -> Vec<(String, String)> {
    let p = |d: &str| root.join(d).display().to_string();
    vec![
        ("XDG_RUNTIME_DIR".into(), p("run")),
        ("XDG_CONFIG_HOME".into(), p("config")),
        ("XDG_STATE_HOME".into(), p("state")),
        ("HOME".into(), p("home")),
        ("OMARCHY_FOCUS_SINK".into(), "null".into()),
        ("OMARCHY_FOCUS_NOTIFY".into(), "0".into()),
    ]
}

fn wait_for(mut cond: impl FnMut() -> bool, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !cond() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn timer_commands_and_persistence() {
    let d = Daemon::start("timer", |_| {});
    assert_eq!(d.status()["phase"], "idle");
    d.ok(&["start"]);
    let s = d.status();
    assert_eq!((s["phase"].as_str(), s["running"].as_bool()), (Some("focus"), Some(true)));
    d.ok(&["pause"]);
    assert_eq!(d.status()["paused"], true);
    d.ok(&["skip"]);
    // Skipping focus starts the break (auto-start breaks is on by default).
    let s = d.status();
    assert_eq!((s["phase"].as_str(), s["running"].as_bool()), (Some("short"), Some(true)));
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(d.root.join("state/omarchy-focus/state.json")).unwrap()).unwrap();
    assert_eq!(saved["phase"], "short");
    d.ok(&["reset"]);
    assert_eq!(d.status()["phase"], "idle");
}

#[test]
fn adjusting_the_running_timer() {
    let d = Daemon::start("adjust", |_| {});
    let refused = d.cli(&["adjust", "5"]);
    assert!(!refused.status.success(), "idle: nothing to adjust");
    assert!(String::from_utf8_lossy(&refused.stderr).contains("no session in progress"));

    d.ok(&["start"]);
    d.ok(&["adjust", "+5"]);
    let left = d.status()["leftMs"].as_f64().unwrap();
    assert!(left > 29.9 * 60_000.0 && left <= 30.0 * 60_000.0, "25 + 5 minutes, got {left}");
    d.ok(&["adjust", "-2"]);
    let left = d.status()["leftMs"].as_f64().unwrap();
    assert!(left > 27.9 * 60_000.0 && left <= 28.0 * 60_000.0, "got {left}");

    // Taking all the time away ends the session, which moves on to the break.
    d.ok(&["adjust", "-60"]);
    wait_for(|| d.status()["phase"] == "short", "the session to end");
    assert_eq!(d.status()["today"], 1, "it counts as finished");
    assert_eq!(d.cli(&["adjust", "abc"]).status.code(), Some(2));
}

#[test]
fn settings_presets_and_errors() {
    let d = Daemon::start("settings", |_| {});
    d.ok(&["set", "focusMinutes", "50"]);
    assert_eq!(d.status()["settings"]["focusMinutes"], 50);
    assert_eq!(d.status()["timerPreset"], "custom");
    d.ok(&["preset", "timer", "deep"]);
    assert_eq!(d.status()["timerPreset"], "deep");

    d.ok(&["preset", "sound", "beta"]);
    let s = d.status();
    assert_eq!((s["soundPreset"].as_str(), s["settings"]["soundEnabled"].as_bool()), (Some("beta"), Some(true)));

    let config = d.root.join("config/omarchy-focus/config.toml");
    wait_for(|| std::fs::read_to_string(&config).unwrap_or_default().contains("beatMode = \"binaural\""), "config.toml to be written");

    d.ok(&["set", "noiseColor", "custom"]);
    d.ok(&["set", "noiseBands", "[0,0,0,50,100,100,100,50,0,0]"]);
    let s = d.status();
    assert_eq!((s["settings"]["noiseColor"].as_str(), s["soundPreset"].as_str()), (Some("custom"), Some("custom")));
    assert_eq!(s["settings"]["noiseBands"][4], 100);
    assert!(!d.cli(&["set", "noiseBands", "[1,2,3]"]).status.success(), "ten bands required");

    let bad = d.cli(&["set", "noiseColor", "purple"]);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("invalid value"));
    assert!(!d.cli(&["set", "nope", "1"]).status.success());
    assert!(!d.cli(&["preset", "sound", "nope"]).status.success());
    assert_eq!(d.cli(&["frobnicate"]).status.code(), Some(2));
}

#[test]
fn sound_follows_the_timer_and_audition() {
    let d = Daemon::start("sound", |_| {});
    // Defaults: sound off, and only during focus once switched on.
    d.ok(&["sound", "on"]);
    let s = d.status();
    assert_eq!((s["sound"]["wanted"].as_bool(), s["sound"]["playing"].as_bool()), (Some(false), Some(false)));
    d.ok(&["start"]);
    wait_for(|| d.status()["sound"]["playing"] == true, "sound to play during focus");
    d.ok(&["pause"]);
    wait_for(|| d.status()["sound"]["playing"] == false, "sound to stop on pause");

    // A subscriber auditions; disconnecting ends the audition.
    let mut sock = UnixStream::connect(d.socket()).unwrap();
    writeln!(sock, r#"{{"cmd":"subscribe"}}"#).unwrap();
    writeln!(sock, r#"{{"cmd":"audition","on":true}}"#).unwrap();
    let mut reader = BufReader::new(sock.try_clone().unwrap());
    let mut first = String::new();
    reader.read_line(&mut first).unwrap();
    assert!(first.contains("\"event\":\"catalog\""), "{first}");
    wait_for(|| d.status()["sound"]["active"] == true, "audition to start the sound");
    drop(reader);
    drop(sock);
    wait_for(|| d.status()["sound"]["active"] == false, "audition to end with the connection");

    d.ok(&["set", "soundOnlyDuringFocus", "false"]);
    wait_for(|| d.status()["sound"]["playing"] == true, "continuous sound");
    d.ok(&["sound", "off"]);
    wait_for(|| d.status()["sound"]["playing"] == false, "sound off");
}

#[test]
fn imports_settings_and_resumes_a_running_session() {
    let ends_at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() + 600_000;
    let d = Daemon::start("import", |root| {
        std::fs::create_dir_all(root.join("config/omarchy")).unwrap();
        std::fs::write(
            root.join("config/omarchy/shell.json"),
            r#"{"bar":{"layout":{"centerEnd":[{"id":"local.focus","noiseVolume":40,"beatMode":"binaural","beatHz":40,"carrierHz":400}]}}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("state/omarchy-focus")).unwrap();
        std::fs::write(
            root.join("state/omarchy-focus/state.json"),
            format!(r#"{{"version":1,"phase":"focus","running":true,"fresh":false,"endsAt":{ends_at},"remainingMs":0,"phaseTotalMs":1500000,"completedFocus":1,"completedToday":2,"day":""}}"#),
        )
        .unwrap();
    });
    let s = d.status();
    assert_eq!(s["settings"]["noiseVolume"], 40);
    assert_eq!(s["soundPreset"], "gamma");
    assert_eq!((s["phase"].as_str(), s["running"].as_bool(), s["session"].as_u64()), (Some("focus"), Some(true), Some(2)));
    let left = s["leftMs"].as_f64().unwrap();
    assert!(left > 590_000.0 && left <= 600_000.0, "left {left}");
}

#[test]
fn second_daemon_refuses_to_start() {
    let d = Daemon::start("single", |_| {});
    let out = Command::new(BIN).arg("daemon").envs(env(&d.root)).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("already running"));
}

#[test]
fn a_dragged_slider_applies_every_step_and_saves_the_last() {
    let d = Daemon::start("drag", |_| {});
    let config = d.root.join("config/omarchy-focus/config.toml");
    // One connection, many changes back to back, as a slider drag sends them.
    let mut sock = UnixStream::connect(d.socket()).unwrap();
    let mut reader = BufReader::new(sock.try_clone().unwrap());
    let mut line = String::new();
    for v in (5..=95).step_by(5) {
        writeln!(sock, r#"{{"cmd":"set","patch":{{"noiseVolume":{v}}}}}"#).unwrap();
        line.clear();
        reader.read_line(&mut line).unwrap();
        assert!(line.contains("\"ok\":true"), "{line}");
        // The engine's state follows each step immediately.
        assert_eq!(d.status()["settings"]["noiseVolume"], v);
    }
    wait_for(|| std::fs::read_to_string(&config).unwrap_or_default().contains("noiseVolume = 95"), "the final value on disk");
}
