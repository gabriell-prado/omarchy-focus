//! Focus engine for Omarchy.
//!
//! A small daemon owns everything the Focus plugin does except drawing: the
//! Pomodoro timer, the focus-sound synthesis, alerts, notifications, settings
//! and saved state. The Omarchy shell plugin (QML) is a thin client that
//! talks to it over a Unix socket; see PROTOCOL.md at the repository root.

pub mod audio;
pub mod control;
pub mod core;
pub mod daemon;
pub mod i18n;
pub mod notify;
pub mod paths;
pub mod presets;
pub mod protocol;
pub mod settings;
pub mod state;
pub mod timer;

/// The bar widget's id in the Omarchy shell; used to find the settings the
/// previous, QML-only version kept in shell.json.
pub const PLUGIN_ID: &str = "local.focus";
