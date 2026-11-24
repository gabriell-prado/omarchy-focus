//! User settings: a typed struct, lenient loading (bad values fall back or
//! clamp, as the QML version did), strict patches (so the CLI reports typos),
//! and the TOML file they are kept in.

use crate::audio::alerts::ALERT_IDS;
use crate::paths::write_atomic;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;
use std::io;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoiseColor {
    Off,
    White,
    Pink,
    Brown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BeatMode {
    Off,
    Binaural,
    Isochronic,
}

pub const NOISE_COLORS: [NoiseColor; 4] = [NoiseColor::Off, NoiseColor::White, NoiseColor::Pink, NoiseColor::Brown];
pub const BEAT_MODES: [BeatMode; 3] = [BeatMode::Off, BeatMode::Binaural, BeatMode::Isochronic];

impl NoiseColor {
    pub fn parse(s: &str) -> Option<NoiseColor> {
        serde_json::from_value(Value::String(s.to_string())).ok()
    }
}

impl BeatMode {
    pub fn parse(s: &str) -> Option<BeatMode> {
        serde_json::from_value(Value::String(s.to_string())).ok()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub focus_minutes: u32,
    pub short_break_minutes: u32,
    pub long_break_minutes: u32,
    pub sessions_before_long_break: u32,
    pub auto_start_breaks: bool,
    pub auto_start_focus: bool,

    pub sound_enabled: bool,
    pub sound_only_during_focus: bool,
    pub noise_color: NoiseColor,
    pub noise_volume: u32,
    pub beat_mode: BeatMode,
    pub beat_hz: u32,
    pub carrier_hz: u32,
    pub tone_volume: u32,

    pub focus_end_sound: String,
    pub break_end_sound: String,
    pub alert_volume: u32,
    pub custom_sound_path: String,
    pub desktop_notifications: bool,
    pub show_time_in_bar: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            focus_minutes: 25,
            short_break_minutes: 5,
            long_break_minutes: 15,
            sessions_before_long_break: 4,
            auto_start_breaks: true,
            auto_start_focus: false,
            sound_enabled: false,
            sound_only_during_focus: true,
            noise_color: NoiseColor::Pink,
            noise_volume: 50,
            beat_mode: BeatMode::Off,
            beat_hz: 16,
            carrier_hz: 250,
            tone_volume: 35,
            focus_end_sound: "bowl".into(),
            break_end_sound: "chime".into(),
            alert_volume: 70,
            custom_sound_path: String::new(),
            desktop_notifications: true,
            show_time_in_bar: true,
        }
    }
}

/// Allowed range of every integer setting.
pub const INT_LIMITS: &[(&str, u32, u32)] = &[
    ("focusMinutes", 1, 180),
    ("shortBreakMinutes", 1, 60),
    ("longBreakMinutes", 1, 90),
    ("sessionsBeforeLongBreak", 1, 12),
    ("noiseVolume", 0, 100),
    ("beatHz", 1, 45),
    ("carrierHz", 100, 600),
    ("toneVolume", 0, 100),
    ("alertVolume", 0, 100),
];

fn limits(key: &str) -> Option<(u32, u32)> {
    INT_LIMITS.iter().find(|(k, _, _)| *k == key).map(|(_, lo, hi)| (*lo, *hi))
}

fn number_of(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn bool_of(v: &Value, lenient_strings: bool) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::String(s) if lenient_strings => match s.trim() {
            "true" | "on" | "yes" | "1" => Some(true),
            "false" | "off" | "no" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

impl Settings {
    /// Every key optional; a missing or bad value keeps the default, numbers
    /// are rounded and clamped. Unknown keys are ignored.
    pub fn from_value(raw: &Value) -> Settings {
        let defaults = serde_json::to_value(Settings::default()).expect("settings serialize");
        let mut merged = defaults.as_object().cloned().unwrap_or_default();
        if let Some(obj) = raw.as_object() {
            for (key, value) in obj {
                let Some(default) = merged.get(key).cloned() else { continue };
                if let Some(v) = coerce(key, value, &default, false) {
                    merged.insert(key.clone(), v);
                }
            }
        }
        serde_json::from_value(Value::Object(merged)).unwrap_or_default()
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("settings serialize")
    }

    /// Apply a patch from a client. Unlike loading, a value that is not
    /// understood is an error, so `omarchy-focus set` reports typos.
    pub fn patched(&self, patch: &Map<String, Value>) -> Result<Settings, String> {
        let mut merged = self.to_value().as_object().cloned().unwrap_or_default();
        for (key, value) in patch {
            let Some(default) = merged.get(key).cloned() else {
                return Err(format!("unknown setting '{key}'"));
            };
            let v = coerce(key, value, &default, true).ok_or_else(|| format!("invalid value for '{key}': {value}"))?;
            merged.insert(key.clone(), v);
        }
        Ok(Settings::from_value(&Value::Object(merged)))
    }

    pub fn keys() -> Vec<String> {
        Settings::default().to_value().as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default()
    }

    /// Load the TOML file. Ok(None) when it does not exist yet.
    pub fn load(path: &Path) -> io::Result<Option<Settings>> {
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let value: Value = toml::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        Ok(Some(Settings::from_value(&value)))
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let body = toml::to_string_pretty(self).map_err(|e| io::Error::other(e.to_string()))?;
        let text = format!(
            "# Omarchy Focus settings. Edited by the panel and `omarchy-focus set`;\n\
             # hand edits are picked up while the daemon runs.\n\n{body}"
        );
        write_atomic(path, text.as_bytes())
    }

    /// The settings the QML-only version kept in this widget's shell.json
    /// entry, for a one-time import.
    pub fn import_from_shell_json(path: &Path, plugin_id: &str) -> Option<Settings> {
        let text = fs::read_to_string(path).ok()?;
        let config: Value = serde_json::from_str(&text).ok()?;
        let layout = config.get("bar")?.get("layout")?.as_object()?;
        for entries in layout.values() {
            for entry in entries.as_array().into_iter().flatten() {
                if entry.get("id").and_then(Value::as_str) == Some(plugin_id) {
                    let has_settings = entry.as_object().map(|o| o.keys().any(|k| k != "id")).unwrap_or(false);
                    return has_settings.then(|| Settings::from_value(entry));
                }
            }
        }
        None
    }
}

/// Turn `value` into a valid value for `key`, shaped like `default`.
fn coerce(key: &str, value: &Value, default: &Value, strict: bool) -> Option<Value> {
    if let Some((lo, hi)) = limits(key) {
        let n = number_of(value)?;
        if !n.is_finite() {
            return None;
        }
        return Some(Value::from(n.round().clamp(lo as f64, hi as f64) as u32));
    }
    match default {
        Value::Bool(_) => bool_of(value, strict).map(Value::Bool),
        Value::String(_) => {
            let s = value.as_str()?;
            match key {
                "noiseColor" => NoiseColor::parse(s).map(|_| Value::from(s)),
                "beatMode" => BeatMode::parse(s).map(|_| Value::from(s)),
                "focusEndSound" | "breakEndSound" => ALERT_IDS.contains(&s).then(|| Value::from(s)),
                _ => Some(Value::from(s.chars().take(1024).collect::<String>())),
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_round_trip() {
        let d = Settings::default();
        assert_eq!(Settings::from_value(&d.to_value()), d);
        assert_eq!(Settings::from_value(&json!({})), d);
    }

    #[test]
    fn lenient_load_falls_back_or_clamps() {
        let s = Settings::from_value(&json!({
            "focusMinutes": "999", "shortBreakMinutes": -3, "noiseColor": "purple",
            "beatMode": 4, "soundEnabled": "yes", "alertVolume": null,
            "focusEndSound": "nope", "customSoundPath": 7, "extra": 1
        }));
        assert_eq!(s.focus_minutes, 180);
        assert_eq!(s.short_break_minutes, 1);
        assert_eq!(s.noise_color, NoiseColor::Pink);
        assert_eq!(s.beat_mode, BeatMode::Off);
        assert!(!s.sound_enabled);
        assert_eq!(s.alert_volume, 70);
        assert_eq!(s.focus_end_sound, "bowl");
        assert_eq!(s.custom_sound_path, "");
    }

    #[test]
    fn strict_patch_reports_mistakes() {
        let d = Settings::default();
        let ok = d.patched(json!({"focusMinutes": 50, "soundEnabled": "on", "noiseColor": "brown"}).as_object().unwrap()).unwrap();
        assert_eq!((ok.focus_minutes, ok.sound_enabled, ok.noise_color), (50, true, NoiseColor::Brown));
        assert!(d.patched(json!({"focusMinuts": 50}).as_object().unwrap()).is_err());
        assert!(d.patched(json!({"noiseColor": "purple"}).as_object().unwrap()).is_err());
        assert!(d.patched(json!({"autoStartBreaks": "maybe"}).as_object().unwrap()).is_err());
        assert_eq!(d.patched(json!({"focusMinutes": 500}).as_object().unwrap()).unwrap().focus_minutes, 180);
    }

    #[test]
    fn toml_round_trip_and_import() {
        let dir = std::env::temp_dir().join(format!("focus-settings-{}", std::process::id()));
        let path = dir.join("config.toml");
        let s = Settings { noise_color: NoiseColor::Brown, beat_mode: BeatMode::Binaural, ..Settings::default() };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), Some(s));
        assert_eq!(Settings::load(&dir.join("missing.toml")).unwrap(), None);

        let shell = dir.join("shell.json");
        fs::write(&shell, r#"{"bar":{"layout":{"centerEnd":[{"id":"local.focus","noiseVolume":40,"beatMode":"binaural"}],"left":[{"id":"x"}]}}}"#).unwrap();
        let imported = Settings::import_from_shell_json(&shell, "local.focus").unwrap();
        assert_eq!((imported.noise_volume, imported.beat_mode), (40, BeatMode::Binaural));
        fs::write(&shell, r#"{"bar":{"layout":{"centerEnd":[{"id":"local.focus"}]}}}"#).unwrap();
        assert!(Settings::import_from_shell_json(&shell, "local.focus").is_none());
        fs::remove_dir_all(dir).ok();
    }
}
