//! The Pomodoro state machine. Pure: every operation takes the current time
//! (ms since the epoch) and the local day, so it is fully testable. Times
//! are wall-clock, so a session keeps counting through suspend.

use crate::settings::Settings;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    #[default]
    Idle,
    Focus,
    Short,
    Long,
}

impl Phase {
    pub fn is_break(self) -> bool {
        matches!(self, Phase::Short | Phase::Long)
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Focus => "focus",
            Phase::Short => "short",
            Phase::Long => "long",
        }
    }
}

/// The persisted timer. Field names match the state file the QML version
/// wrote, so a running session carries over to the daemon.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Timer {
    pub version: u32,
    pub phase: Phase,
    pub running: bool,
    /// A queued phase that has not started: its length follows the settings.
    pub fresh: bool,
    pub ends_at: f64,
    pub remaining_ms: f64,
    pub phase_total_ms: f64,
    /// Focus sessions finished in the current set; a long break resets it.
    pub completed_focus: u32,
    pub completed_today: u32,
    pub day: String,
}

impl Default for Timer {
    fn default() -> Self {
        Timer {
            version: 1,
            phase: Phase::Idle,
            running: false,
            fresh: true,
            ends_at: 0.0,
            remaining_ms: 0.0,
            phase_total_ms: 0.0,
            completed_focus: 0,
            completed_today: 0,
            day: String::new(),
        }
    }
}

/// A phase ended (or was skipped).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transition {
    pub ended: Phase,
    pub next: Phase,
    pub auto_started: bool,
    /// Whether to play the alert and notify: not for a skip, nor for a phase
    /// that ended while the daemon was not running.
    pub announce: bool,
}

pub fn phase_minutes(phase: Phase, s: &Settings) -> u32 {
    match phase {
        Phase::Short => s.short_break_minutes,
        Phase::Long => s.long_break_minutes,
        _ => s.focus_minutes,
    }
}

pub fn phase_ms(phase: Phase, s: &Settings) -> f64 {
    phase_minutes(phase, s) as f64 * 60_000.0
}

/// The break after a focus session; `completed` already counts it.
pub fn break_after(completed: u32, s: &Settings) -> Phase {
    let every = s.sessions_before_long_break.max(1);
    if completed > 0 && completed.is_multiple_of(every) {
        Phase::Long
    } else {
        Phase::Short
    }
}

/// 1-based position of the current (or just finished) focus session in its set.
pub fn session_in_set(completed: u32, phase: Phase, s: &Settings) -> u32 {
    let every = s.sessions_before_long_break.max(1);
    match phase {
        Phase::Focus | Phase::Idle => completed % every + 1,
        _ => match completed % every {
            0 => every,
            n => n,
        },
    }
}

/// Sessions late by more than this when the daemon starts end silently.
pub const QUIET_AFTER_MS: f64 = 60_000.0;

/// A session can be stretched to at most this much time left.
pub const MAX_LEFT_MS: f64 = 6.0 * 3_600_000.0;

impl Timer {
    pub fn paused(&self) -> bool {
        self.phase != Phase::Idle && !self.running && !self.fresh
    }

    pub fn duration_ms(&self, s: &Settings) -> f64 {
        phase_ms(if self.phase == Phase::Idle { Phase::Focus } else { self.phase }, s)
    }

    pub fn left_ms(&self, now: f64, s: &Settings) -> f64 {
        if self.running {
            (self.ends_at - now).max(0.0)
        } else if self.fresh {
            self.duration_ms(s)
        } else {
            self.remaining_ms
        }
    }

    pub fn total_ms(&self, s: &Settings) -> f64 {
        if !self.fresh && self.phase_total_ms > 0.0 {
            self.phase_total_ms
        } else {
            self.duration_ms(s)
        }
    }

    pub fn progress(&self, now: f64, s: &Settings) -> f64 {
        let total = self.total_ms(s);
        if self.phase == Phase::Idle || total <= 0.0 {
            return 0.0;
        }
        (1.0 - self.left_ms(now, s) / total).clamp(0.0, 1.0)
    }

    pub fn done_in_set(&self, s: &Settings) -> u32 {
        let every = s.sessions_before_long_break.max(1);
        if self.phase == Phase::Long {
            every
        } else {
            self.completed_focus % every
        }
    }

    pub fn session(&self, s: &Settings) -> u32 {
        session_in_set(self.completed_focus, self.phase, s)
    }

    pub fn today_count(&self, today: &str) -> u32 {
        if self.day == today {
            self.completed_today
        } else {
            0
        }
    }

    pub fn start(&mut self, now: f64, s: &Settings) {
        if self.phase == Phase::Idle {
            self.prepare(Phase::Focus, s);
        }
        if self.running {
            return;
        }
        let length = if self.fresh { phase_ms(self.phase, s) } else { self.remaining_ms };
        if self.fresh {
            self.phase_total_ms = length;
        }
        self.ends_at = now + length;
        self.fresh = false;
        self.running = true;
    }

    pub fn pause(&mut self, now: f64) {
        if !self.running {
            return;
        }
        self.remaining_ms = (self.ends_at - now).max(0.0);
        self.running = false;
    }

    pub fn toggle(&mut self, now: f64, s: &Settings) {
        if self.running {
            self.pause(now)
        } else {
            self.start(now, s)
        }
    }

    /// Add (or, negative, take away) time from the session in progress,
    /// running or paused. A running session cut to zero ends on the next tick,
    /// as if its time had run out; a paused one keeps at least a second so
    /// resuming does not end it at once. Returns false when no session is in
    /// progress (idle, or a phase queued but not started).
    pub fn adjust(&mut self, now: f64, delta_ms: f64) -> bool {
        if self.phase == Phase::Idle || self.fresh || !delta_ms.is_finite() {
            return false;
        }
        let left = if self.running { (self.ends_at - now).max(0.0) } else { self.remaining_ms };
        let floor = if self.running { 0.0 } else { 1000.0f64.min(left) };
        let new_left = (left + delta_ms).clamp(floor, MAX_LEFT_MS.max(left));
        if self.running {
            self.ends_at = now + new_left;
        } else {
            self.remaining_ms = new_left;
        }
        // Keep the progress bar truthful: the session is now this much longer
        // (or shorter) in total.
        self.phase_total_ms = (self.phase_total_ms + (new_left - left)).max(new_left);
        true
    }

    /// Set how much time is left in the session in progress (dragging the
    /// time bar). Unlike adjust(), the session's total length is kept, so this
    /// moves the bar rather than stretching it. Running and set to zero, the
    /// session ends on the next tick; paused, at least a second is left.
    pub fn seek(&mut self, now: f64, left_ms: f64) -> bool {
        if self.phase == Phase::Idle || self.fresh || !left_ms.is_finite() {
            return false;
        }
        let floor = if self.running { 0.0 } else { 1000.0 };
        let left = left_ms.clamp(floor, MAX_LEFT_MS);
        if self.running {
            self.ends_at = now + left;
        } else {
            self.remaining_ms = left;
        }
        self.phase_total_ms = self.phase_total_ms.max(left);
        true
    }

    /// Back to idle; the set starts over. Today's count is kept.
    pub fn reset(&mut self) {
        self.running = false;
        self.phase = Phase::Idle;
        self.fresh = true;
        self.remaining_ms = 0.0;
        self.phase_total_ms = 0.0;
        self.completed_focus = 0;
    }

    pub fn skip(&mut self, now: f64, s: &Settings, today: &str) -> Option<Transition> {
        (self.phase != Phase::Idle).then(|| self.finish(now, s, today, true, false))
    }

    /// Finish the phase when its time is up.
    pub fn tick(&mut self, now: f64, s: &Settings, today: &str) -> Option<Transition> {
        (self.running && now >= self.ends_at).then(|| self.finish(now, s, today, false, false))
    }

    /// After loading saved state: a phase that ended long ago moves on silently
    /// and is not auto-started; one that just ended is left for tick().
    pub fn catch_up(&mut self, now: f64, s: &Settings, today: &str) -> Option<Transition> {
        (self.running && self.phase != Phase::Idle && now - self.ends_at > QUIET_AFTER_MS)
            .then(|| self.finish(now, s, today, false, true))
    }

    fn prepare(&mut self, next: Phase, s: &Settings) {
        self.phase = next;
        self.running = false;
        self.fresh = true;
        self.remaining_ms = 0.0;
        self.phase_total_ms = phase_ms(next, s);
    }

    fn count_today(&mut self, today: &str) {
        if self.day != today {
            self.day = today.to_string();
            self.completed_today = 0;
        }
        self.completed_today += 1;
    }

    fn finish(&mut self, now: f64, s: &Settings, today: &str, skipped: bool, quiet: bool) -> Transition {
        let ended = self.phase;
        let next = if ended == Phase::Focus {
            self.completed_focus += 1;
            if !skipped {
                self.count_today(today);
            }
            break_after(self.completed_focus, s)
        } else {
            if ended == Phase::Long {
                self.completed_focus = 0;
            }
            Phase::Focus
        };
        let auto_started = !quiet && if next == Phase::Focus { s.auto_start_focus } else { s.auto_start_breaks };
        self.prepare(next, s);
        if auto_started {
            self.start(now, s);
        }
        Transition { ended, next, auto_started, announce: !skipped && !quiet }
    }
}

/// `mm:ss`, or `h:mm:ss` from an hour up; rounds up so 00:00 means done.
pub fn format_clock(ms: f64) -> String {
    let total = (ms.max(0.0) / 1000.0).ceil() as u64;
    let (h, m, s) = (total / 3600, total % 3600 / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: f64 = 1_800_000_000_000.0;
    const MIN: f64 = 60_000.0;
    const DAY: &str = "2026-11-24";

    fn s() -> Settings {
        Settings::default()
    }

    #[test]
    fn long_break_every_n_sessions() {
        let got: Vec<_> = [1, 2, 3, 4, 5, 8].into_iter().map(|n| break_after(n, &s())).collect();
        assert_eq!(got, [Phase::Short, Phase::Short, Phase::Short, Phase::Long, Phase::Short, Phase::Long]);
        let one = Settings { sessions_before_long_break: 1, ..s() };
        assert_eq!(break_after(1, &one), Phase::Long);
    }

    #[test]
    fn session_position() {
        assert_eq!(session_in_set(0, Phase::Idle, &s()), 1);
        assert_eq!(session_in_set(1, Phase::Focus, &s()), 2);
        assert_eq!(session_in_set(1, Phase::Short, &s()), 1);
        assert_eq!(session_in_set(4, Phase::Long, &s()), 4);
    }

    #[test]
    fn clock() {
        assert_eq!(format_clock(25.0 * MIN), "25:00");
        assert_eq!(format_clock(59_001.0), "01:00");
        assert_eq!(format_clock(999.0), "00:01");
        assert_eq!(format_clock(-5.0), "00:00");
        assert_eq!(format_clock(90.0 * MIN), "1:30:00");
    }

    #[test]
    fn start_pause_resume_keeps_remaining_time() {
        let mut t = Timer::default();
        t.start(T0, &s());
        assert_eq!((t.phase, t.running), (Phase::Focus, true));
        assert_eq!(t.left_ms(T0 + 1000.0, &s()), 25.0 * MIN - 1000.0);
        t.pause(T0 + 60_000.0);
        assert!(t.paused());
        assert_eq!(t.left_ms(T0 + 999_999.0, &s()), 24.0 * MIN, "paused time does not run");
        t.start(T0 + 120_000.0, &s());
        assert_eq!(t.ends_at, T0 + 120_000.0 + 24.0 * MIN);
    }

    #[test]
    fn natural_finish_counts_and_auto_starts_break() {
        let mut t = Timer::default();
        t.start(T0, &s());
        assert_eq!(t.tick(T0 + 10.0 * MIN, &s(), DAY), None);
        let tr = t.tick(T0 + 25.0 * MIN, &s(), DAY).unwrap();
        assert_eq!(tr, Transition { ended: Phase::Focus, next: Phase::Short, auto_started: true, announce: true });
        assert_eq!((t.phase, t.running, t.today_count(DAY)), (Phase::Short, true, 1));
        // Break over: focus is not auto-started by default.
        let tr = t.tick(T0 + 31.0 * MIN, &s(), DAY).unwrap();
        assert_eq!((tr.next, tr.auto_started), (Phase::Focus, false));
        assert!(!t.running && t.fresh);
        assert_eq!(t.session(&s()), 2);
    }

    #[test]
    fn skip_is_silent_and_not_counted() {
        let mut t = Timer::default();
        t.start(T0, &s());
        let tr = t.skip(T0 + MIN, &s(), DAY).unwrap();
        assert!(!tr.announce);
        assert_eq!(t.today_count(DAY), 0);
        assert_eq!(t.phase, Phase::Short);
        assert_eq!(Timer::default().skip(T0, &s(), DAY), None);
    }

    #[test]
    fn long_break_resets_the_set() {
        let mut t = Timer::default();
        for _ in 0..4 {
            t.start(T0, &s());
            t.skip(T0, &s(), DAY);
            if t.phase == Phase::Short {
                t.skip(T0, &s(), DAY);
            }
        }
        assert_eq!(t.phase, Phase::Long);
        assert_eq!(t.done_in_set(&s()), 4);
        t.skip(T0, &s(), DAY);
        assert_eq!((t.phase, t.completed_focus), (Phase::Focus, 0));
    }

    #[test]
    fn adjusting_the_session_in_progress() {
        let mut t = Timer::default();
        assert!(!t.adjust(T0, MIN), "nothing to adjust while idle");
        t.start(T0, &s());
        assert!(t.adjust(T0 + MIN, 5.0 * MIN));
        assert_eq!(t.left_ms(T0 + MIN, &s()), 29.0 * MIN, "24 left + 5");
        assert_eq!(t.total_ms(&s()), 30.0 * MIN, "the progress total grows too");
        assert!((t.progress(T0 + MIN, &s()) - 1.0 / 30.0).abs() < 1e-9);

        t.adjust(T0 + MIN, -10.0 * MIN);
        assert_eq!(t.left_ms(T0 + MIN, &s()), 19.0 * MIN);

        // Paused: the remaining time changes, and never drops below a second.
        t.pause(T0 + 2.0 * MIN);
        t.adjust(T0 + 2.0 * MIN, 1.0 * MIN);
        assert_eq!(t.left_ms(T0 + 99.0 * MIN, &s()), 19.0 * MIN);
        t.adjust(T0, -500.0 * MIN);
        assert_eq!(t.remaining_ms, 1000.0);

        // Running and cut to zero: the next tick ends it with an announcement.
        t.start(T0 + 3.0 * MIN, &s());
        t.adjust(T0 + 3.0 * MIN, -60.0 * MIN);
        let tr = t.tick(T0 + 3.0 * MIN, &s(), DAY).unwrap();
        assert!(tr.announce && tr.ended == Phase::Focus);

        // A queued (not started) phase is not "in progress".
        let mut q = Timer::default();
        q.start(T0, &s());
        q.skip(T0, &s(), DAY); // short break, auto-started
        q.skip(T0, &s(), DAY); // focus queued, not started
        assert!(q.fresh && !q.adjust(T0, MIN));
        assert!(!q.adjust(T0, f64::NAN));
    }

    #[test]
    fn seeking_moves_the_session_without_changing_its_length() {
        let mut t = Timer::default();
        assert!(!t.seek(T0, MIN), "nothing to seek while idle");
        t.start(T0, &s());
        assert!(t.seek(T0, 10.0 * MIN));
        assert_eq!(t.left_ms(T0, &s()), 10.0 * MIN);
        assert_eq!(t.total_ms(&s()), 25.0 * MIN, "the length is kept");
        assert!((t.progress(T0, &s()) - 0.6).abs() < 1e-9);

        t.pause(T0);
        t.seek(T0, 0.0);
        assert_eq!(t.remaining_ms, 1000.0, "paused keeps a second");
        t.seek(T0, 40.0 * MIN);
        assert_eq!(t.total_ms(&s()), 40.0 * MIN, "seeking past the length stretches it");

        t.start(T0, &s());
        t.seek(T0, 0.0);
        assert!(t.tick(T0, &s(), DAY).unwrap().announce, "running and seeked to the end: it ends normally");
        assert!(!t.seek(T0, f64::NAN));
    }

    #[test]
    fn catch_up_after_downtime_is_quiet() {
        let mut t = Timer::default();
        t.start(T0, &s());
        assert_eq!(t.catch_up(T0 + 25.5 * MIN, &s(), DAY), None, "just ended: left for tick()");
        let tr = t.catch_up(T0 + 120.0 * MIN, &s(), DAY).unwrap();
        assert!(!tr.announce && !tr.auto_started);
        assert_eq!((t.phase, t.running), (Phase::Short, false));
    }

    #[test]
    fn day_rollover() {
        let mut t = Timer::default();
        t.start(T0, &s());
        t.tick(T0 + 25.0 * MIN, &s(), DAY);
        assert_eq!(t.today_count(DAY), 1);
        assert_eq!(t.today_count("2026-11-25"), 0);
    }

    #[test]
    fn reads_the_qml_state_file() {
        let old = r#"{"version":1,"phase":"focus","running":true,"fresh":false,"endsAt":1795540072252,"remainingMs":0,"phaseTotalMs":1500000,"completedFocus":0,"completedToday":0,"day":""}"#;
        let t: Timer = serde_json::from_str(old).unwrap();
        assert_eq!((t.phase, t.running, t.ends_at), (Phase::Focus, true, 1795540072252.0));
    }
}
