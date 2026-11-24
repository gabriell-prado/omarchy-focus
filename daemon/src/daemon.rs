//! Daemon start-up: claim the socket, load settings and state, start the
//! audio, clock and control threads, then run the core on this thread.

use crate::audio::engine::Engine;
use crate::audio::output::SinkKind;
use crate::core::{now_ms, today, Core, Msg};
use crate::notify::Notifier;
use crate::paths::Paths;
use crate::settings::Settings;
use crate::{control, state, PLUGIN_ID};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const TICK: Duration = Duration::from_millis(250);

pub fn run() -> Result<(), String> {
    let paths = Paths::from_env();
    let listener = bind(&paths.socket)?;
    let settings = load_settings(&paths);
    let mut timer = state::load(&paths.state);
    // A phase that ended while nothing was running moves on silently.
    timer.catch_up(now_ms(), &settings, &today());

    let (tx, rx) = mpsc::channel::<Msg>();
    let audio_tx = tx.clone();
    let engine = Engine::spawn(SinkKind::from_env(), move |status| {
        let _ = audio_tx.send(Msg::Audio(status));
    });
    let mut core = Core::new(paths.clone(), settings, timer, engine, Notifier::new());

    let tick_tx = tx.clone();
    thread::Builder::new()
        .name("clock".into())
        .spawn(move || loop {
            thread::sleep(TICK);
            if tick_tx.send(Msg::Tick).is_err() {
                break;
            }
        })
        .map_err(|e| e.to_string())?;

    let control_tx = tx.clone();
    thread::Builder::new()
        .name("control".into())
        .spawn(move || control::serve(listener, control_tx))
        .map_err(|e| e.to_string())?;
    drop(tx);

    eprintln!("omarchy-focus: listening on {}", paths.socket.display());
    core.after_change();
    for msg in rx {
        core.handle(msg);
    }
    Ok(())
}

fn bind(path: &Path) -> Result<UnixListener, String> {
    if path.exists() {
        if UnixStream::connect(path).is_ok() {
            return Err(format!("already running ({})", path.display()));
        }
        fs::remove_file(path).map_err(|e| format!("cannot remove stale {}: {e}", path.display()))?;
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let listener = UnixListener::bind(path).map_err(|e| format!("cannot listen on {}: {e}", path.display()))?;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    Ok(listener)
}

/// config.toml; on first start, whatever the QML-only version kept in
/// shell.json; else defaults. A broken file is reported and left alone.
fn load_settings(paths: &Paths) -> Settings {
    match Settings::load(&paths.config) {
        Ok(Some(s)) => s,
        Ok(None) => {
            let imported = Settings::import_from_shell_json(&paths.shell_json, PLUGIN_ID);
            if imported.is_some() {
                eprintln!("omarchy-focus: imported settings from {}", paths.shell_json.display());
            }
            let s = imported.unwrap_or_default();
            if let Err(e) = s.save(&paths.config) {
                eprintln!("omarchy-focus: cannot write {}: {e}", paths.config.display());
            }
            s
        }
        Err(e) => {
            eprintln!("omarchy-focus: {} is invalid ({e}); using defaults until it is fixed", paths.config.display());
            Settings::default()
        }
    }
}
