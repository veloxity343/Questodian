//! The SQLite database: where it lives, opening it, and bringing its schema up to date.
//!
//! The schema version is kept in an `alembic_version` table: the first (Python) version of
//! Questodian managed its schema with Alembic, and databases it made open unchanged.
//! Schema changes are now written here: add the upgrade below and bump [`HEAD`], keep
//! writing `alembic_version`, and add a fixture database at the old version to the
//! tests. A database newer than this app knows is refused untouched; an older one is
//! copied to a backup file before upgrading.
//!
//! SQLite calls are synchronous (`rusqlite`) and quick; anything that can wait long (the
//! folder lock, an upgrade) runs on a blocking thread.
//!
//! The same code runs in a browser (the web build), with two differences: the database is
//! in the page's own storage, where only one connection may have a file open (so there's one
//! connection, and every unit of work takes its turn, reads too), and that storage has no
//! write-ahead log, so SQLite keeps its default rollback journal there.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags, params};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::kinds::{Column, ColumnKind, Kind};
use crate::repo::{Repo, new_id, now};

pub const DB_FILENAME: &str = "questodian.db";
#[cfg(not(target_family = "wasm"))]
const LOCK_FILENAME: &str = "questodian.lock";

/// The newest schema this app understands.
pub const HEAD: &str = "0004";
/// What a database made before migrations existed contains.
const BASELINE: &str = "0001";

const SCHEMA_0002: &str = include_str!("migrations/0002_schema.sql");
const UPGRADE_0001_TO_0002: &str = include_str!("migrations/0001_to_0002.sql");
/// The upgrades written in SQL, in order: from, and what takes it to the next revision.
const STEPS: [(&str, &str); 2] =
    [("0002", include_str!("migrations/0002_to_0003.sql")), ("0003", include_str!("migrations/0003_to_0004.sql"))];
/// Every revision this app can upgrade from, oldest first.
const KNOWN: [&str; 4] = [BASELINE, "0002", "0003", HEAD];

/// How long a statement waits for another copy of the app to finish writing.
const BUSY_TIMEOUT: Duration = Duration::from_secs(10);
/// In a browser: see the top of this file.
const WEB: bool = cfg!(target_family = "wasm");
/// Connections kept open for reuse between units of work.
const IDLE_CONNECTIONS: usize = if WEB { 1 } else { 4 };
/// Prepared statements kept per connection (each kind has a few; sqlx kept 100 too).
const STATEMENT_CACHE: usize = 100;

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

/// An open database and what's known about its tables.
pub struct Database {
    pool: Pool,
    /// Units of work that write (and in a browser, every unit of work) take turns here,
    /// waiting without holding a thread. (Writes from another copy of the app are waited
    /// for inside SQLite, up to [`BUSY_TIMEOUT`].)
    turns: tokio::sync::Mutex<()>,
    pub data_dir: PathBuf,
    pub path: PathBuf,
    /// The copy saved before an upgrade, if this open upgraded the schema.
    pub backup: Option<PathBuf>,
    columns: HashMap<Kind, Vec<Column>>,
    /// Held (shared) while the database is open; see [`Database::open`]. `None` where the
    /// folder doesn't support file locks.
    _lock: Option<std::fs::File>,
}

impl Database {
    /// Open (or create) `questodian.db` in `data_dir`, upgrading it if it's older.
    ///
    /// Every open copy of the app shares a lock on `questodian.lock` in the data folder.
    /// Changing the schema needs it exclusively, so a database is never upgraded under
    /// another running copy (which would then break).
    ///
    /// In a browser `data_dir` is a name within the page's storage (empty for its top), and
    /// the storage's own lock (one open file at a time) keeps other copies out instead. The
    /// web build sets that storage up before calling this.
    pub async fn open(data_dir: impl AsRef<Path>) -> Result<Database> {
        let data_dir = data_dir.as_ref().to_path_buf();
        let path = data_dir.join(DB_FILENAME);
        #[cfg(not(target_family = "wasm"))]
        let (backup, lock) = {
            std::fs::create_dir_all(&data_dir)?;
            let (data_dir, path) = (data_dir.clone(), path.clone());
            tokio::task::spawn_blocking(move || open_guarded(&data_dir, &path))
                .await
                .map_err(|e| Error::Internal(e.to_string()))??
        };
        #[cfg(target_family = "wasm")]
        let (backup, lock) = (upgrade(&path, true)?, None);
        let pool = Pool { path: path.clone(), idle: Mutex::new(Vec::new()) };
        let conn = pool.get()?;
        let columns = read_columns(&conn)?;
        pool.put(conn);
        #[cfg(target_family = "wasm")]
        crate::files::open()?;
        Ok(Database { pool, turns: tokio::sync::Mutex::new(()), data_dir, path, backup, columns, _lock: lock })
    }

    /// A unit of work that may write: nothing is saved unless [`Repo::commit`] is called.
    /// `BEGIN IMMEDIATE` takes the write lock up front, so two writers queue instead of failing.
    /// Writers take turns, so a task holding one must not ask for another: it would wait forever.
    pub async fn write(&self) -> Result<Repo<'_>> {
        let turn = self.turns.lock().await;
        Repo::begin(&self.pool, "BEGIN IMMEDIATE", Some(turn), &self.columns, self.files_dir())
    }

    /// A unit of work that only reads.
    pub async fn read(&self) -> Result<Repo<'_>> {
        let turn = if WEB { Some(self.turns.lock().await) } else { None };
        Repo::begin(&self.pool, "BEGIN", turn, &self.columns, self.files_dir())
    }

    /// Where handout images live.
    pub fn files_dir(&self) -> PathBuf {
        self.data_dir.join("handouts")
    }

    /// Close the idle connections (the last one to close tidies up the write-ahead log).
    /// Connections in use close when their unit of work ends and the database is dropped.
    pub async fn close(&self) {
        self.pool.idle().clear();
    }
}

/// Connections to the database file, kept open and reused between units of work.
pub(crate) struct Pool {
    path: PathBuf,
    idle: Mutex<Vec<Connection>>,
}

impl Pool {
    /// An idle connection, or a new one.
    pub(crate) fn get(&self) -> Result<Connection> {
        let idle = self.idle().pop();
        match idle {
            Some(conn) => Ok(conn),
            None => connect(&self.path),
        }
    }

    /// Give a connection back for reuse. One still inside a transaction (its rollback
    /// failed) is closed instead.
    pub(crate) fn put(&self, conn: Connection) {
        let mut idle = self.idle();
        if conn.is_autocommit() && idle.len() < IDLE_CONNECTIONS {
            idle.push(conn);
        }
    }

    fn idle(&self) -> MutexGuard<'_, Vec<Connection>> {
        self.idle.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A connection for units of work. The upgrade has made (or checked) the file, so a missing
/// one is an error rather than a new empty database.
fn connect(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    conn.busy_timeout(BUSY_TIMEOUT)?;
    conn.set_prepared_statement_cache_capacity(STATEMENT_CACHE);
    conn.execute_batch("PRAGMA foreign_keys = ON")?;
    if !WEB {
        conn.execute_batch("PRAGMA journal_mode = WAL")?;
    }
    Ok(conn)
}

/// Each table's columns and what they hold, from the database itself.
fn read_columns(conn: &Connection) -> Result<HashMap<Kind, Vec<Column>>> {
    let mut columns = HashMap::new();
    for kind in Kind::ALL {
        let mut statement = conn.prepare(&format!("PRAGMA table_info({})", kind.table()))?;
        let list = statement
            .query_map([], |row| {
                let name: String = row.get("name")?;
                let declared: String = row.get("type")?;
                Ok(Column { kind: ColumnKind::from_declared(&name, &declared), name })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        columns.insert(kind, list);
    }
    Ok(columns)
}

/// The first column of every row.
fn strings(conn: &Connection, sql: &str) -> Result<Vec<String>> {
    let mut statement = conn.prepare(sql)?;
    let rows = statement.query_map([], |row| row.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

// --------------------------------------------------------------------------- the folder lock

/// Bring the schema up to date under the folder lock, and return the lock held shared.
///
/// - No other copy open: upgrade if needed while holding the lock exclusively, then share it.
/// - Another copy open: wait for a shared lock (an upgrade in progress finishes first), then
///   check the version, refusing anything but the current one.
/// - No file locks on this filesystem (some network drives): carry on unguarded, as before
///   the lock existed. The upgrade still re-reads the version inside its own transaction.
///
/// Whenever the version was read before the shared lock was taken, it's read again after,
/// so a copy never runs on a schema another (newer) copy changed in between.
#[cfg(not(target_family = "wasm"))]
fn open_guarded(data_dir: &Path, path: &Path) -> Result<(Option<PathBuf>, Option<std::fs::File>)> {
    use std::fs::TryLockError;

    let lock = std::fs::File::options().create(true).truncate(false).write(true).open(data_dir.join(LOCK_FILENAME));
    let lock = match lock {
        Ok(file) => file,
        Err(e) => return unguarded(path, &e),
    };
    let lock_error = |e: std::io::Error| Error::Internal(format!("couldn't lock {}: {e}", data_dir.display()));
    match lock.try_lock() {
        Ok(()) => {
            let backup = upgrade(path, true);
            let shared = lock.unlock().and_then(|()| lock.lock_shared());
            let backup = backup?;
            shared.map_err(lock_error)?;
            // Between unlock and lock_shared another copy could have changed the schema.
            upgrade(path, false)?;
            Ok((backup, Some(lock)))
        }
        Err(TryLockError::WouldBlock) => {
            // Waits while another copy upgrades.
            lock.lock_shared().map_err(lock_error)?;
            Ok((upgrade(path, false)?, Some(lock)))
        }
        Err(TryLockError::Error(e)) => unguarded(path, &e),
    }
}

#[cfg(not(target_family = "wasm"))]
fn unguarded(path: &Path, why: &std::io::Error) -> Result<(Option<PathBuf>, Option<std::fs::File>)> {
    eprintln!("questodian: file locks aren't available here ({why}); open only one copy of Questodian at a time");
    Ok((upgrade(path, true)?, None))
}

// --------------------------------------------------------------------------- migrations

/// What the file holds, before any change.
#[derive(Debug, PartialEq)]
enum State {
    /// No tables yet.
    Empty,
    /// At a known schema revision.
    At(String),
}

/// Read the schema version. Anything unrecognised is refused with a plain message.
fn read_state(conn: &Connection, path: &Path) -> Result<State> {
    let tables = strings(conn, "SELECT name FROM sqlite_master WHERE type = 'table'")?;
    let has_app_tables = Kind::ALL.iter().any(|k| tables.iter().any(|t| t == k.table()));
    if !tables.iter().any(|t| t == "alembic_version") {
        // Tables but no version: made before migrations existed, so it has the baseline schema.
        return Ok(if has_app_tables { State::At(BASELINE.into()) } else { State::Empty });
    }
    let versions = strings(conn, "SELECT version_num FROM alembic_version")?;
    match versions.as_slice() {
        [version] => Ok(State::At(version.clone())),
        [] if !has_app_tables => {
            // An empty database apart from an empty version table: start it afresh.
            conn.execute_batch("DROP TABLE alembic_version")?;
            Ok(State::Empty)
        }
        _ => Err(refused(path, "its schema version record is missing or duplicated")),
    }
}

fn refused(path: &Path, why: &str) -> Error {
    Error::Internal(format!("Questodian can't open {}: {why}. Nothing was changed.", path.display()))
}

/// Bring the file at `path` up to [`HEAD`]. Returns the backup made first, if any.
/// `alone` says no other copy of the app has the folder open, so the schema may change.
fn upgrade(path: &Path, alone: bool) -> Result<Option<PathBuf>> {
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(path, flags)?;
    conn.busy_timeout(BUSY_TIMEOUT)?;
    // Foreign keys stay off while tables change, so nothing cascades; the pragma only
    // takes effect outside a transaction.
    conn.execute_batch("PRAGMA foreign_keys = OFF")?;
    let result = upgrade_with(&conn, path, alone);
    conn.close().map_err(|(_, e)| e)?;
    result
}

fn upgrade_with(conn: &Connection, path: &Path, alone: bool) -> Result<Option<PathBuf>> {
    let state = read_state(conn, path)?;
    let version = match &state {
        State::Empty => {
            run_locked(conn, path, &state, create_schema)?;
            return Ok(None);
        }
        State::At(version) => version.clone(),
    };
    if version == HEAD {
        return Ok(None);
    }
    if !KNOWN.contains(&version.as_str()) {
        return Err(refused(
            path,
            &format!("it was made by a newer version of Questodian (schema {version}), or isn't a Questodian database"),
        ));
    }
    // Only an older version this app would upgrade needs the folder to itself.
    if !alone {
        return Err(refused(
            path,
            "it needs upgrading to this version of Questodian, but another copy of Questodian has it open. \
             Close the other copy and try again",
        ));
    }
    // Check everything that can be checked before touching anything.
    let moves = if version == BASELINE { encounters_to_move(conn, path)? } else { Vec::new() };
    let backup = backup_database(conn, path, &version)?;
    let upgraded = run_locked(conn, path, &state, |conn| upgrade_from(conn, &version, moves));
    upgraded.map_err(|e| {
        Error::Internal(format!(
            "Upgrading {} failed: {e}. Nothing was changed; a copy made before the upgrade is at {}.",
            path.display(),
            backup.display()
        ))
    })?;
    Ok(Some(backup))
}

/// Run every upgrade from `version` to [`HEAD`], in one transaction.
fn upgrade_from(conn: &Connection, version: &str, moves: Vec<EncounterMove>) -> Result<()> {
    let mut at = version;
    if at == BASELINE {
        upgrade_0001_to_0002(conn, moves)?;
        at = "0002";
    }
    let first = STEPS
        .iter()
        .position(|(from, _)| *from == at)
        .ok_or_else(|| Error::Internal(format!("no upgrade from revision {at}")))?;
    for (_, sql) in &STEPS[first..] {
        conn.execute_batch(sql)?;
    }
    set_version(conn, HEAD)
}

/// Record the schema revision (in Alembic's table, so every version of the app agrees).
fn set_version(conn: &Connection, version: &str) -> Result<()> {
    // A database from before migrations has no version table yet.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS alembic_version (version_num VARCHAR(32) NOT NULL, \
         CONSTRAINT alembic_version_pkc PRIMARY KEY (version_num)); \
         DELETE FROM alembic_version;",
    )?;
    conn.execute("INSERT INTO alembic_version (version_num) VALUES (?)", [version])?;
    Ok(())
}

/// Run `step` in one write-locked transaction, if the version is still what was read
/// (another copy of the app may have upgraded the file in the meantime).
fn run_locked(
    conn: &Connection,
    path: &Path,
    expected: &State,
    step: impl FnOnce(&Connection) -> Result<()>,
) -> Result<()> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = match read_state(conn, path) {
        Ok(state) if state != *expected => Ok(()), // someone else got there first
        Ok(_) => step(conn),
        Err(e) => Err(e),
    };
    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT")?;
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

fn create_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA_0002)?;
    for (_, sql) in STEPS {
        conn.execute_batch(sql)?;
    }
    set_version(conn, HEAD)
}

/// A session's encounter, checked and ready to become an encounters row.
struct EncounterMove {
    session_id: String,
    campaign_id: Option<String>,
    name: String,
    round: i64,
    turn: i64,
    active: bool,
    combatants: Value,
    party_size: i64,
}

/// Read every session's encounter (revision 0001 stored one per session). Empty ones are
/// skipped; anything that can't be read refuses the upgrade, so no data is ever dropped.
fn encounters_to_move(conn: &Connection, path: &Path) -> Result<Vec<EncounterMove>> {
    use crate::schemas::{Combatant, Lenient};

    let mut statement = conn.prepare("SELECT id, campaign_id, number, title, encounter FROM sessions")?;
    let mut rows = statement.query([])?;
    let mut moves = Vec::new();
    while let Some(row) = rows.next()? {
        let session_id: String = row.get("id")?;
        let title: String = row.get("title").unwrap_or_default();
        let unreadable = |why: &str| {
            refused(path, &format!("the encounter saved with session '{title}' ({session_id}) can't be read ({why})"))
        };
        let encounter: Value = match row.get_ref("encounter")? {
            ValueRef::Null => Value::Null,
            ValueRef::Text(bytes) => {
                let text = std::str::from_utf8(bytes).map_err(|_| unreadable("it isn't text"))?;
                serde_json::from_str(text).map_err(|_| unreadable("it isn't valid JSON"))?
            }
            _ => return Err(unreadable("it isn't text")),
        };
        let encounter = match encounter {
            Value::Null => continue,
            Value::Object(map) => map,
            _ => return Err(unreadable("it isn't an object")),
        };
        let combatants = match encounter.get("combatants") {
            None | Some(Value::Null) => continue,
            Some(Value::Array(list)) if list.is_empty() => continue,
            Some(Value::Array(list)) => list.clone(),
            Some(_) => return Err(unreadable("its combatants aren't a list")),
        };
        for c in &combatants {
            Combatant::from_json(c.clone()).map_err(|e| unreadable(&format!("a combatant is invalid: {e}")))?;
        }
        let number_field = |key: &str| match encounter.get(key) {
            None | Some(Value::Null) => Ok(0),
            Some(v) => i64::from_json(v.clone()).map_err(|_| unreadable(&format!("its {key} isn't a whole number"))),
        };
        let (round, turn) = (number_field("round")?, number_field("turn")?);
        let pcs = combatants.iter().filter(|c| c.get("kind").and_then(Value::as_str) == Some("pc")).count();
        let number: i64 = row.get("number").unwrap_or(0);
        moves.push(EncounterMove {
            campaign_id: row.get("campaign_id")?,
            name: format!("Session {number}: {title}"),
            round,
            turn,
            active: encounter.get("active").is_some_and(truthy),
            combatants: Value::Array(combatants),
            party_size: pcs.max(1) as i64,
            session_id,
        });
    }
    Ok(moves)
}

/// Revision 0002 added the DM screen tables and the campaign edition, and moved each
/// session's encounter into the new encounters table (linked back to the session).
/// Mirrors the Python app's Alembic migration 0002 (`0002_dm_screen.py`).
fn upgrade_0001_to_0002(conn: &Connection, moves: Vec<EncounterMove>) -> Result<()> {
    let tables = strings(conn, "SELECT name FROM sqlite_master WHERE type = 'table'")?;
    if tables.iter().any(|t| t == "encounters") {
        return Err(Error::Internal("an earlier upgrade stopped halfway (the encounters table already exists)".into()));
    }
    conn.execute_batch(UPGRADE_0001_TO_0002)?;
    let stamp = now();
    let mut insert = conn.prepare(
        "INSERT INTO encounters (id, created, updated, campaign_id, session_id, scene_id, name, round, turn, \
         active, combatants, party_size, party_level, log, notes) \
         VALUES (?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?, 1, '[]', '')",
    )?;
    for m in moves {
        insert.execute(params![
            new_id(),
            stamp,
            stamp,
            m.campaign_id,
            m.session_id,
            m.name,
            m.round,
            m.turn,
            m.active,
            m.combatants.to_string(),
            m.party_size,
        ])?;
    }
    drop(insert);
    conn.execute_batch("ALTER TABLE sessions DROP COLUMN encounter")?;
    Ok(())
}

/// Python's `bool()` of a JSON value, as the Python migration used.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Copy a live database safely (WAL included) to `<name>.backup-<label>-<time>`, and make
/// sure the copy is on disk before anything changes. If a backup of this revision was made
/// after the database last changed (an earlier attempt that failed), it's kept instead of
/// making another; an older one (say, from before going back to an earlier version and
/// carrying on there) isn't a copy of the current data, so a new one is made.
fn backup_database(conn: &Connection, path: &Path, label: &str) -> Result<PathBuf> {
    let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let prefix = format!("{name}.backup-{label}-");
    let folder = path.parent().unwrap_or(Path::new("."));
    if let Some(existing) = reusable_backup(path, folder, &prefix)? {
        return Ok(existing);
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dest = folder.join(format!("{prefix}{stamp}"));
    conn.execute("VACUUM INTO ?", [dest.to_string_lossy()])?;
    #[cfg(not(target_family = "wasm"))]
    {
        std::fs::File::open(&dest)?.sync_all()?;
        if let Ok(dir) = std::fs::File::open(folder) {
            let _ = dir.sync_all(); // not possible on every OS; the file itself is synced
        }
    }
    Ok(dest)
}

/// The newest backup starting with `prefix`, if it was made after the database last changed.
#[cfg(not(target_family = "wasm"))]
fn reusable_backup(path: &Path, folder: &Path, prefix: &str) -> Result<Option<PathBuf>> {
    let existing = std::fs::read_dir(folder)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(prefix)))
        .max();
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    let wal = PathBuf::from(wal);
    let changed = [modified(path), modified(&wal)].into_iter().flatten().max();
    Ok(existing.filter(|e| modified(e).zip(changed).is_some_and(|(backup, data)| backup >= data)))
}

/// Browser storage can't be listed or dated from here, so a backup is always made afresh.
#[cfg(target_family = "wasm")]
fn reusable_backup(_path: &Path, _folder: &Path, _prefix: &str) -> Result<Option<PathBuf>> {
    Ok(None)
}

/// For tests and tools: run a statement list (e.g. a fixture dump) on a database file.
pub async fn load_sql(path: &Path, sql: &str) -> Result<()> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA foreign_keys = OFF")?;
    conn.execute_batch(sql)?;
    conn.close().map_err(|(_, e)| e)?;
    Ok(())
}
