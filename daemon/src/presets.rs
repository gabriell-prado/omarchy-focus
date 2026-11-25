//! Timer methods and research-based sound presets. The explanations shown to
//! the user live in the plugin's i18n catalogs (`timer.preset.<id>.note`,
//! `preset.<id>.note`); citations stay here, untranslated.

use crate::settings::{BeatMode, NoiseColor, Settings};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct TimerPreset {
    pub id: &'static str,
    pub focus: u32,
    pub short: u32,
    pub long: u32,
    pub sessions: u32,
    pub refs: &'static str,
}

pub const TIMER_PRESETS: &[TimerPreset] = &[
    TimerPreset { id: "classic", focus: 25, short: 5, long: 15, sessions: 4, refs: "Cirillo 2006 · Biwer et al. 2023" },
    TimerPreset { id: "deep", focus: 50, short: 10, long: 30, sessions: 3, refs: "" },
    TimerPreset { id: "desktime", focus: 52, short: 17, long: 17, sessions: 4, refs: "DeskTime 2014 (company data)" },
    TimerPreset { id: "ultradian", focus: 90, short: 20, long: 30, sessions: 2, refs: "Kleitman 1982" },
];

/// Shown for a schedule that matches no preset.
pub const CUSTOM_TIMER_REFS: &str = "Ariga & Lleras 2011";

/// Evidence: "moderate" (replicated or meta-analytic support), "limited"
/// (single or small studies, or indirect support), "experimental" (mostly
/// animal work or mechanism only).
///
/// A preset says what the sound is (noise color, tone and its frequencies),
/// never how loud: the volume sliders stay where the user put them.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundPreset {
    pub id: &'static str,
    pub evidence: &'static str,
    pub noise_color: NoiseColor,
    pub beat_mode: BeatMode,
    pub beat_hz: u32,
    pub carrier_hz: u32,
    pub refs: &'static str,
}

const fn noise(id: &'static str, evidence: &'static str, color: NoiseColor, refs: &'static str) -> SoundPreset {
    SoundPreset { id, evidence, noise_color: color, beat_mode: BeatMode::Off, beat_hz: 0, carrier_hz: 0, refs }
}

const fn beats(
    id: &'static str, evidence: &'static str, color: NoiseColor, mode: BeatMode, hz: u32, carrier: u32, refs: &'static str,
) -> SoundPreset {
    SoundPreset { id, evidence, noise_color: color, beat_mode: mode, beat_hz: hz, carrier_hz: carrier, refs }
}

pub const SOUND_PRESETS: &[SoundPreset] = &[
    noise("speech", "moderate", NoiseColor::Pink, "Colle & Welsh 1976 · Hongisto 2005"),
    noise("white", "moderate", NoiseColor::White, "Söderlund et al. 2007, 2010"),
    noise("brown", "limited", NoiseColor::Brown, ""),
    beats("beta", "limited", NoiseColor::Pink, BeatMode::Binaural, 16, 250, "Lane et al. 1998"),
    beats("alpha", "limited", NoiseColor::Brown, BeatMode::Binaural, 10, 200, "Garcia-Argibay et al. 2019"),
    beats("iso40", "experimental", NoiseColor::Brown, BeatMode::Isochronic, 40, 300, "Martorell et al. 2019"),
];

pub fn match_timer(s: &Settings) -> &'static str {
    TIMER_PRESETS
        .iter()
        .find(|p| {
            p.focus == s.focus_minutes
                && p.short == s.short_break_minutes
                && p.long == s.long_break_minutes
                && p.sessions == s.sessions_before_long_break
        })
        .map(|p| p.id)
        .unwrap_or("custom")
}

pub fn timer_refs(id: &str) -> &'static str {
    TIMER_PRESETS.iter().find(|p| p.id == id).map(|p| p.refs).unwrap_or(CUSTOM_TIMER_REFS)
}

pub fn apply_timer(id: &str, s: &mut Settings) -> bool {
    let Some(p) = TIMER_PRESETS.iter().find(|p| p.id == id) else { return false };
    s.focus_minutes = p.focus;
    s.short_break_minutes = p.short;
    s.long_break_minutes = p.long;
    s.sessions_before_long_break = p.sessions;
    true
}

/// Presets are matched on what they are (color, tone kind and frequencies),
/// not on volume, so turning a preset up or down keeps its name.
pub fn match_sound(s: &Settings) -> &'static str {
    SOUND_PRESETS
        .iter()
        .find(|p| {
            p.noise_color == s.noise_color
                && p.beat_mode == s.beat_mode
                && (p.beat_mode == BeatMode::Off || (p.beat_hz == s.beat_hz && p.carrier_hz == s.carrier_hz))
        })
        .map(|p| p.id)
        .unwrap_or("custom")
}

/// Apply a preset's sound; volumes are left untouched.
pub fn apply_sound(id: &str, s: &mut Settings) -> bool {
    let Some(p) = SOUND_PRESETS.iter().find(|p| p.id == id) else { return false };
    s.noise_color = p.noise_color;
    s.beat_mode = p.beat_mode;
    if p.beat_mode != BeatMode::Off {
        s.beat_hz = p.beat_hz;
        s.carrier_hz = p.carrier_hz;
    }
    true
}

/// EEG band of a beat frequency.
pub fn band_for(hz: u32) -> &'static str {
    match hz {
        0..=3 => "delta",
        4..=7 => "theta",
        8..=12 => "alpha",
        13..=29 => "beta",
        _ => "gamma",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn timer_presets_round_trip() {
        for p in TIMER_PRESETS {
            let mut s = Settings::default();
            assert!(apply_timer(p.id, &mut s));
            assert_eq!(match_timer(&s), p.id);
        }
        let s = Settings { focus_minutes: 33, ..Settings::default() };
        assert_eq!(match_timer(&s), "custom");
        assert_eq!(timer_refs("custom"), CUSTOM_TIMER_REFS);
    }

    #[test]
    fn sound_presets_round_trip_and_are_distinct() {
        let mut seen = HashSet::new();
        for p in SOUND_PRESETS {
            let mut s = Settings::default();
            assert!(apply_sound(p.id, &mut s));
            assert_eq!(match_sound(&s), p.id);
            s.noise_volume = 3;
            assert_eq!(match_sound(&s), p.id, "volume must not change the preset");
            let before = Settings { noise_volume: 17, tone_volume: 83, ..Settings::default() };
            let mut after = before.clone();
            apply_sound(p.id, &mut after);
            assert_eq!((after.noise_volume, after.tone_volume), (17, 83), "{}: a preset never changes the volumes", p.id);
            assert!(seen.insert((p.noise_color, p.beat_mode, p.beat_hz, p.carrier_hz)), "duplicate preset {}", p.id);
        }
    }

    #[test]
    fn bands() {
        let got: Vec<_> = [2, 6, 10, 16, 40].into_iter().map(band_for).collect();
        assert_eq!(got, ["delta", "theta", "alpha", "beta", "gamma"]);
    }
}
