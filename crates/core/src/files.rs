//! Handout images: files in the handouts folder, or in a browser, rows in a second database.
//!
//! A browser page has no folders to write to, so the web build keeps each image in
//! `handouts.db` (in the same browser storage as the main database), under the path it
//! would have had. Everything else sees the same three calls on every platform.

use std::path::Path;

use crate::error::Result;

/// Store `bytes` at `path`, making its folder if needed.
pub fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    imp::write(path, bytes)
}

/// The bytes at `path`, if there's a file there.
pub fn read(path: &Path) -> Option<Vec<u8>> {
    imp::read(path)
}

/// Remove the file at `path`; already gone is fine.
pub fn remove(path: &Path) {
    imp::remove(path)
}

#[cfg(not(target_family = "wasm"))]
mod imp {
    use std::path::Path;

    use crate::error::Result;

    pub fn write(path: &Path, bytes: &[u8]) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Ok(std::fs::write(path, bytes)?)
    }

    pub fn read(path: &Path) -> Option<Vec<u8>> {
        std::fs::read(path).ok()
    }

    pub fn remove(path: &Path) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(target_family = "wasm")]
mod imp {
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};

    use rusqlite::{Connection, OptionalExtension};

    use crate::error::{Error, Result};

    /// The one connection to `handouts.db` (browser storage allows one per database).
    static STORE: OnceLock<Mutex<Connection>> = OnceLock::new();

    /// Open `handouts.db`. Called once, by [`crate::db::Database::open`].
    pub fn open() -> Result<()> {
        let conn = Connection::open("handouts.db")?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS files (path TEXT PRIMARY KEY, bytes BLOB NOT NULL)")?;
        STORE.set(Mutex::new(conn)).map_err(|_| Error::Internal("the handouts store is already open".into()))
    }

    fn with<T>(f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T> {
        let store = STORE.get().ok_or_else(|| Error::Internal("the handouts store isn't open".into()))?;
        let conn = store.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(f(&conn)?)
    }

    fn key(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    pub fn write(path: &Path, bytes: &[u8]) -> Result<()> {
        with(|conn| {
            conn.execute(
                "INSERT INTO files (path, bytes) VALUES (?, ?) ON CONFLICT (path) DO UPDATE SET bytes = excluded.bytes",
                rusqlite::params![key(path), bytes],
            )
        })?;
        Ok(())
    }

    pub fn read(path: &Path) -> Option<Vec<u8>> {
        with(|conn| conn.query_row("SELECT bytes FROM files WHERE path = ?", [key(path)], |row| row.get(0)).optional())
            .ok()
            .flatten()
    }

    pub fn remove(path: &Path) {
        let _ = with(|conn| conn.execute("DELETE FROM files WHERE path = ?", [key(path)]));
    }
}

#[cfg(target_family = "wasm")]
pub(crate) use imp::open;
