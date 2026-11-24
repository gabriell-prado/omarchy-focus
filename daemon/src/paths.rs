//! Where things live. Every location follows the XDG variables, so tests can
//! point the daemon at a temporary directory.

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Paths {
    pub home: PathBuf,
    pub socket: PathBuf,
    pub config: PathBuf,
    pub state: PathBuf,
    pub shell_json: PathBuf,
    pub language: PathBuf,
}

fn dir_from(var: &str, home: &Path, fallback: &str) -> PathBuf {
    match env::var_os(var) {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => home.join(fallback),
    }
}

impl Paths {
    pub fn from_env() -> Paths {
        let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
        let config = dir_from("XDG_CONFIG_HOME", &home, ".config");
        let state = dir_from("XDG_STATE_HOME", &home, ".local/state");
        let runtime = match env::var_os("XDG_RUNTIME_DIR") {
            Some(v) if !v.is_empty() => PathBuf::from(v),
            // SAFETY: getuid has no preconditions and cannot fail.
            _ => PathBuf::from(format!("/run/user/{}", unsafe { getuid() })),
        };
        Paths {
            socket: runtime.join("omarchy-focus.sock"),
            config: config.join("omarchy-focus/config.toml"),
            state: state.join("omarchy-focus/state.json"),
            shell_json: config.join("omarchy/shell.json"),
            language: env::var_os("OMARCHY_LANGUAGE_FILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| config.join("omarchy/language")),
            home,
        }
    }
}

extern "C" {
    fn getuid() -> u32;
}

/// Write a file by renaming a sibling temporary file over it, so a crash or a
/// reader never sees half a file.
pub fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)
}
