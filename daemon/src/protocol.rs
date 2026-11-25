//! The control protocol: one JSON object per line over a Unix socket.
//! See PROTOCOL.md at the repository root for the full contract.

use serde::Deserialize;
use serde_json::{json, Map, Value};

pub const VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PresetKind {
    Timer,
    Sound,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "cmd", rename_all = "kebab-case")]
pub enum Command {
    /// Keep the connection open and receive `catalog` once, then `state`
    /// whenever it changes.
    Subscribe,
    Status,
    Toggle,
    Start,
    Pause,
    Reset,
    Skip,
    /// Add (or, negative, take away) minutes from the session in progress.
    Adjust { minutes: f64 },
    Set { patch: Map<String, Value> },
    Preset { kind: PresetKind, id: String },
    /// Switch the focus sound on or off; without `on`, toggle it.
    Sound {
        #[serde(default)]
        on: Option<bool>,
    },
    /// While on, a sound that follows the timer also plays outside focus
    /// sessions (the panel auditions a choice). Ends with the connection.
    Audition { on: bool },
    Alert { id: String },
    StopAlert,
}

/// Longest request line accepted.
pub const MAX_LINE: usize = 64 * 1024;

pub fn parse(line: &str) -> Result<Command, String> {
    serde_json::from_str(line).map_err(|e| format!("bad request: {e}"))
}

pub fn ok() -> Value {
    json!({ "ok": true })
}

pub fn ok_with(key: &str, value: Value) -> Value {
    let mut m = Map::new();
    m.insert("ok".into(), Value::Bool(true));
    m.insert(key.into(), value);
    Value::Object(m)
}

pub fn error(message: impl Into<String>) -> Value {
    json!({ "ok": false, "error": message.into() })
}

pub fn event(name: &str, key: &str, value: Value) -> Value {
    let mut m = Map::new();
    m.insert("event".into(), Value::from(name));
    m.insert(key.into(), value);
    Value::Object(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands() {
        assert_eq!(parse(r#"{"v":1,"cmd":"toggle"}"#).unwrap(), Command::Toggle);
        assert_eq!(parse(r#"{"cmd":"sound"}"#).unwrap(), Command::Sound { on: None });
        assert_eq!(parse(r#"{"cmd":"stop-alert"}"#).unwrap(), Command::StopAlert);
        assert_eq!(
            parse(r#"{"cmd":"preset","kind":"sound","id":"beta"}"#).unwrap(),
            Command::Preset { kind: PresetKind::Sound, id: "beta".into() }
        );
        assert_eq!(parse(r#"{"cmd":"adjust","minutes":-5}"#).unwrap(), Command::Adjust { minutes: -5.0 });
        assert!(parse(r#"{"cmd":"explode"}"#).is_err());
        assert!(parse("not json").is_err());
    }
}
