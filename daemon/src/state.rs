//! The saved timer: $XDG_STATE_HOME/omarchy-focus/state.json.

use crate::paths::write_atomic;
use crate::timer::Timer;
use std::fs;
use std::io;
use std::path::Path;

pub fn load(path: &Path) -> Timer {
    let Ok(text) = fs::read_to_string(path) else { return Timer::default() };
    let mut t: Timer = serde_json::from_str(&text).unwrap_or_default();
    // Bound what came from disk.
    let day = 24.0 * 3_600_000.0;
    t.remaining_ms = t.remaining_ms.clamp(0.0, day);
    t.phase_total_ms = t.phase_total_ms.clamp(0.0, day);
    t.completed_focus = t.completed_focus.min(1000);
    t.completed_today = t.completed_today.min(1000);
    t.day.truncate(16);
    if !t.ends_at.is_finite() || t.ends_at < 0.0 {
        t.ends_at = 0.0;
        t.running = false;
    }
    t
}

pub fn save(path: &Path, timer: &Timer) -> io::Result<()> {
    let mut text = serde_json::to_string(timer).map_err(io::Error::other)?;
    text.push('\n');
    write_atomic(path, text.as_bytes())
}
