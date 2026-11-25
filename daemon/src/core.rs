//! The daemon's single owner of state. Every message (client requests,
//! clock ticks, audio status) is handled here, on one thread, in order; after
//! each one the core persists what changed, updates the sound and pushes the
//! new state to subscribers if anything they can see is different.

use crate::audio::alerts::{self, ALERT_IDS};
use crate::audio::engine::{Cmd, Engine, Status as AudioStatus};
use crate::audio::noise::{shape_for, BAND_HZ};
use crate::audio::{linear_gain, Mix};
use crate::i18n::{self, Strings};
use crate::notify::Notifier;
use crate::paths::Paths;
use crate::presets::{self, CUSTOM_TIMER_REFS, SOUND_PRESETS, TIMER_PRESETS};
use crate::protocol::{self, Command, PresetKind};
use crate::settings::{NoiseColor, Settings, BAR_DISPLAYS, BEAT_MODES, INT_LIMITS, NOISE_COLORS};
use crate::state;
use crate::timer::{self, Phase, Timer, Transition};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub type Writer = Arc<Mutex<UnixStream>>;

/// config.toml is written at most this often. A slider dragged in the panel
/// changes settings many times a second; the sound follows every change at
/// once, the file only catches up.
const SAVE_INTERVAL: Duration = Duration::from_millis(300);

pub enum Msg {
    Request { client: u64, cmd: Command, reply: Sender<Value> },
    Subscribe { client: u64, writer: Writer, reply: Sender<Value> },
    Disconnected { client: u64 },
    Tick,
    Audio(AudioStatus),
}

pub fn now_ms() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0)
}

/// The local day, in the format the QML version stored ("2026-11-24", unpadded).
pub fn today() -> String {
    use chrono::Datelike;
    let d = chrono::Local::now().date_naive();
    format!("{}-{}-{}", d.year(), d.month(), d.day())
}

pub fn write_line(writer: &Writer, value: &Value) -> std::io::Result<()> {
    let mut text = value.to_string();
    text.push('\n');
    let mut w = writer.lock().unwrap_or_else(|e| e.into_inner());
    w.write_all(text.as_bytes())?;
    w.flush()
}

pub fn catalog() -> Value {
    let limits: Map<String, Value> = INT_LIMITS.iter().map(|(k, lo, hi)| (k.to_string(), json!([lo, hi]))).collect();
    json!({
        "version": protocol::VERSION,
        "timerPresets": TIMER_PRESETS,
        "customTimerRefs": CUSTOM_TIMER_REFS,
        "soundPresets": SOUND_PRESETS,
        "alerts": ALERT_IDS,
        "noiseColors": NOISE_COLORS,
        "beatModes": BEAT_MODES,
        "barDisplays": BAR_DISPLAYS,
        "bands": [["delta", 0], ["theta", 4], ["alpha", 8], ["beta", 13], ["gamma", 30]],
        // The custom noise's octave bands, and the slider shapes that sound
        // like each named color (all equal is pink).
        "eqBands": BAND_HZ,
        "eqShapes": {
            "white": shape_for(NoiseColor::White),
            "pink": shape_for(NoiseColor::Pink),
            "brown": shape_for(NoiseColor::Brown),
        },
        "limits": limits,
    })
}

pub struct Core {
    paths: Paths,
    settings: Settings,
    timer: Timer,
    saved_timer: Timer,
    /// The client auditioning the sound from its open panel.
    audition: Option<u64>,
    subscribers: Vec<(u64, Writer)>,
    engine: Engine,
    audio: AudioStatus,
    mix: Option<Mix>,
    notifier: Notifier,
    alert_cache: HashMap<String, Arc<Vec<f32>>>,
    last_state: String,
    config_mtime: Option<SystemTime>,
    last_config_check: Instant,
    last_save: Option<Instant>,
    save_pending: bool,
}

impl Core {
    pub fn new(paths: Paths, settings: Settings, timer: Timer, engine: Engine, notifier: Notifier) -> Core {
        let config_mtime = fs::metadata(&paths.config).and_then(|m| m.modified()).ok();
        Core {
            saved_timer: timer.clone(),
            paths,
            settings,
            timer,
            audition: None,
            subscribers: Vec::new(),
            engine,
            audio: AudioStatus::default(),
            mix: None,
            notifier,
            alert_cache: HashMap::new(),
            last_state: String::new(),
            config_mtime,
            last_config_check: Instant::now(),
            last_save: None,
            save_pending: false,
        }
    }

    pub fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Request { client, cmd, reply } => {
                let response = self.command(client, cmd);
                let _ = reply.send(response);
            }
            Msg::Subscribe { client, writer, reply } => {
                let ok = write_line(&writer, &protocol::event("catalog", "catalog", catalog())).is_ok()
                    && write_line(&writer, &protocol::event("state", "state", self.state_value(now_ms()))).is_ok();
                if ok {
                    self.subscribers.push((client, writer));
                }
                let _ = reply.send(protocol::ok());
            }
            Msg::Disconnected { client } => self.drop_client(client),
            Msg::Tick => {
                if let Some(t) = self.timer.tick(now_ms(), &self.settings, &today()) {
                    self.announce(t);
                }
                if self.save_pending && self.last_save.is_none_or(|t| t.elapsed() >= SAVE_INTERVAL) {
                    self.save_settings();
                }
                if self.last_config_check.elapsed().as_millis() >= 1000 {
                    self.last_config_check = Instant::now();
                    self.reload_config_if_edited();
                }
            }
            Msg::Audio(status) => self.audio = status,
        }
        self.after_change();
    }

    fn drop_client(&mut self, client: u64) {
        self.subscribers.retain(|(id, _)| *id != client);
        if self.audition == Some(client) {
            self.audition = None;
        }
    }

    fn command(&mut self, client: u64, cmd: Command) -> Value {
        let now = now_ms();
        let today = today();
        match cmd {
            Command::Subscribe => return protocol::error("subscribe is handled by the connection"),
            Command::Status => return protocol::ok_with("state", self.state_value(now)),
            Command::Toggle => self.timer.toggle(now, &self.settings),
            Command::Start => self.timer.start(now, &self.settings),
            Command::Pause => self.timer.pause(now),
            Command::Reset => self.timer.reset(),
            Command::Skip => {
                self.timer.skip(now, &self.settings, &today);
            }
            Command::Adjust { minutes } => {
                if !minutes.is_finite() || minutes.abs() > 360.0 {
                    return protocol::error("minutes must be between -360 and 360");
                }
                if !self.timer.adjust(now, minutes * 60_000.0) {
                    return protocol::error("no session in progress");
                }
            }
            Command::Seek { left_ms } => {
                if !left_ms.is_finite() || left_ms < 0.0 || left_ms > timer::MAX_LEFT_MS {
                    return protocol::error("leftMs must be between 0 and 6 hours");
                }
                if !self.timer.seek(now, left_ms) {
                    return protocol::error("no session in progress");
                }
            }
            Command::Set { patch } => match self.settings.patched(&patch) {
                Ok(s) => self.set_settings(s),
                Err(e) => return protocol::error(e),
            },
            Command::Preset { kind, id } => {
                let mut s = self.settings.clone();
                let found = match kind {
                    PresetKind::Timer => presets::apply_timer(&id, &mut s),
                    PresetKind::Sound => {
                        // Picking a sound means wanting to hear it.
                        s.sound_enabled = true;
                        presets::apply_sound(&id, &mut s)
                    }
                };
                if !found {
                    return protocol::error(format!("unknown preset '{id}'"));
                }
                self.set_settings(s);
            }
            Command::Sound { on } => {
                let on = on.unwrap_or(!self.settings.sound_enabled);
                if !on {
                    self.audition = None;
                }
                let s = Settings { sound_enabled: on, ..self.settings.clone() };
                self.set_settings(s);
            }
            Command::Audition { on } => {
                if on {
                    self.audition = Some(client);
                } else if self.audition.is_some() {
                    self.audition = None;
                }
            }
            Command::Alert { id } => {
                if let Err(e) = self.play_alert(&id) {
                    return protocol::error(e);
                }
            }
            Command::StopAlert => self.engine.send(Cmd::StopAlert),
        }
        protocol::ok()
    }

    fn set_settings(&mut self, s: Settings) {
        if s == self.settings {
            return;
        }
        self.settings = s;
        if self.last_save.is_none_or(|t| t.elapsed() >= SAVE_INTERVAL) {
            self.save_settings();
        } else {
            // The next tick writes it, once the interval has passed.
            self.save_pending = true;
        }
    }

    fn save_settings(&mut self) {
        if let Err(e) = self.settings.save(&self.paths.config) {
            eprintln!("omarchy-focus: cannot save {}: {e}", self.paths.config.display());
        }
        self.config_mtime = fs::metadata(&self.paths.config).and_then(|m| m.modified()).ok();
        self.last_save = Some(Instant::now());
        self.save_pending = false;
    }

    /// Pick up hand edits of config.toml.
    fn reload_config_if_edited(&mut self) {
        if self.save_pending {
            return; // what is in memory is newer than the file
        }
        let mtime = fs::metadata(&self.paths.config).and_then(|m| m.modified()).ok();
        if mtime == self.config_mtime {
            return;
        }
        self.config_mtime = mtime;
        match Settings::load(&self.paths.config) {
            Ok(Some(s)) => self.settings = s,
            Ok(None) => {}
            Err(e) => eprintln!("omarchy-focus: ignoring {}: {e}", self.paths.config.display()),
        }
    }

    fn sound_wanted(&self) -> bool {
        let s = &self.settings;
        s.sound_enabled && (!s.sound_only_during_focus || (self.timer.phase == Phase::Focus && self.timer.running))
    }

    fn sound_active(&self) -> bool {
        self.sound_wanted() || (self.settings.sound_enabled && self.audition.is_some())
    }

    fn announce(&mut self, t: Transition) {
        if !t.announce {
            return;
        }
        let sound = if t.ended == Phase::Focus { &self.settings.focus_end_sound } else { &self.settings.break_end_sound };
        if let Err(e) = self.play_alert(&sound.clone()) {
            eprintln!("omarchy-focus: alert: {e}");
        }
        if !self.settings.desktop_notifications {
            return;
        }
        let strings = Strings::new(&i18n::language_from_file(&self.paths.language));
        let minutes = timer::phase_minutes(t.next, &self.settings);
        let title = strings.t(if t.ended == Phase::Focus { "notify.focusDone.title" } else { "notify.breakDone.title" });
        let key = format!("notify.{}.{}", t.next.as_str(), if t.auto_started { "auto" } else { "manual" });
        self.notifier.send(title, strings.tn(&key, minutes));
    }

    /// Play an alert, cutting off any alert still sounding. "none" just stops.
    fn play_alert(&mut self, id: &str) -> Result<(), String> {
        let gain = linear_gain(self.settings.alert_volume);
        if id == "none" || gain == 0.0 {
            self.engine.send(Cmd::StopAlert);
            return Ok(());
        }
        let key = if id == "custom" { format!("custom:{}", self.settings.custom_sound_path) } else { id.to_string() };
        let buf = match self.alert_cache.get(&key) {
            Some(b) => b.clone(),
            None => {
                let Some(rendered) = alerts::render(id, &self.settings.custom_sound_path, &self.paths.home)? else {
                    return Ok(());
                };
                if self.alert_cache.len() >= 32 {
                    self.alert_cache.clear();
                }
                let b = Arc::new(rendered);
                self.alert_cache.insert(key, b.clone());
                b
            }
        };
        self.engine.send(Cmd::Alert { buf, gain });
        Ok(())
    }

    pub fn state_value(&self, now: f64) -> Value {
        let s = &self.settings;
        let t = &self.timer;
        json!({
            "phase": t.phase,
            "running": t.running,
            "fresh": t.fresh,
            "paused": t.paused(),
            "endsAt": t.ends_at.round() as i64,
            "leftMs": t.left_ms(now, s).round() as i64,
            "totalMs": t.total_ms(s).round() as i64,
            "progress": t.progress(now, s),
            "session": t.session(s),
            "sessions": s.sessions_before_long_break,
            "doneInSet": t.done_in_set(s),
            "today": t.today_count(&today()),
            "settings": s,
            "timerPreset": presets::match_timer(s),
            "soundPreset": presets::match_sound(s),
            "sound": {
                "wanted": self.sound_wanted(),
                "active": self.sound_active(),
                "auditioning": self.audition.is_some(),
                "playing": self.audio.playing,
                "failed": self.audio.failed,
            },
        })
    }

    /// The state minus what only moves with the clock, to decide whether to push.
    fn state_key(&self, mut v: Value) -> String {
        if self.timer.running {
            if let Some(o) = v.as_object_mut() {
                o.remove("leftMs");
                o.remove("progress");
            }
        }
        v.to_string()
    }

    pub fn after_change(&mut self) {
        let mix = if self.sound_active() { Mix::from_settings(&self.settings) } else { Mix::silent() };
        if self.mix != Some(mix) {
            self.engine.send(Cmd::Mix(mix));
            self.mix = Some(mix);
        }

        if self.timer != self.saved_timer {
            if let Err(e) = state::save(&self.paths.state, &self.timer) {
                eprintln!("omarchy-focus: cannot save {}: {e}", self.paths.state.display());
            }
            self.saved_timer = self.timer.clone();
        }

        let value = self.state_value(now_ms());
        let key = self.state_key(value.clone());
        if key == self.last_state {
            return;
        }
        self.last_state = key;
        let event = protocol::event("state", "state", value);
        let mut gone = Vec::new();
        for (id, writer) in &self.subscribers {
            if write_line(writer, &event).is_err() {
                gone.push(*id);
            }
        }
        for id in gone {
            self.drop_client(id);
        }
    }
}
