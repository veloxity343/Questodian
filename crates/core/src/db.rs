//! Where the data lives, and (from M2) the SQLite connection and migrations.

use std::path::PathBuf;

pub const DB_FILENAME: &str = "questodian.db";

/// `$QUESTODIAN_DATA`, else the usual per-user data folder for this OS.
pub fn default_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("QUESTODIAN_DATA").filter(|d| !d.is_empty()) {
        return expand_home(PathBuf::from(dir));
    }
    let home = std::env::home_dir().unwrap_or_default();
    if cfg!(windows) {
        let roaming =
            std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| home.join("AppData").join("Roaming"));
        roaming.join("Questodian")
    } else if cfg!(target_os = "macos") {
        home.join("Library").join("Application Support").join("Questodian")
    } else {
        let base = std::env::var_os("XDG_DATA_HOME").filter(|d| !d.is_empty()).map(PathBuf::from);
        base.unwrap_or_else(|| home.join(".local").join("share")).join("questodian")
    }
}

/// `~/x` -> `/home/me/x`.
fn expand_home(path: PathBuf) -> PathBuf {
    match path.strip_prefix("~") {
        Ok(rest) => std::env::home_dir().unwrap_or_default().join(rest),
        Err(_) => path,
    }
}
