//! The SQLite database: where it lives, opening it, and bringing its schema up to date.
//!
//! The schema version is kept in an `alembic_version` table: the first (Python) version of
//! Questodian managed its schema with Alembic, and databases it made open unchanged.
//! Schema changes are now written here: add the upgrade below and bump [`HEAD`], keep
//! writing `alembic_version`, and add a fixture database at the old version to the
//! tests. A database newer than this app knows is refused untouched; an older one is
//! copied to a backup file before upgrading.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Connection, Row, SqliteConnection, SqlitePool, ValueRef};

use crate::error::{Error, Result};
use crate::kinds::{Column, ColumnKind, Kind};
use crate::repo::{Repo, new_id, now};

pub const DB_FILENAME: &str = "questodian.db";
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
    pool: SqlitePool,
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
    pub async fn open(data_dir: impl AsRef<Path>) -> Result<Database> {
        let data_dir = data_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&data_dir)?;
        let path = data_dir.join(DB_FILENAME);
        let (backup, lock) = open_guarded(&data_dir, &path).await?;

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(10));
        let pool = SqlitePoolOptions::new().max_connections(4).connect_with(options).await?;
        let columns = read_columns(&pool).await?;
        Ok(Database { pool, data_dir, path, backup, columns, _lock: lock })
    }

    /// A unit of work that may write: nothing is saved unless [`Repo::commit`] is called.
    /// `BEGIN IMMEDIATE` takes the write lock up front, so two writers queue instead of failing.
    pub async fn write(&self) -> Result<Repo<'_>> {
        Ok(Repo::new(self.pool.begin_with("BEGIN IMMEDIATE").await?, &self.columns, self.files_dir()))
    }

    /// A unit of work that only reads.
    pub async fn read(&self) -> Result<Repo<'_>> {
        Ok(Repo::new(self.pool.begin().await?, &self.columns, self.files_dir()))
    }

    /// Where handout images live.
    pub fn files_dir(&self) -> PathBuf {
        self.data_dir.join("handouts")
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }
}

/// Each table's columns and what they hold, from the database itself.
async fn read_columns(pool: &SqlitePool) -> Result<HashMap<Kind, Vec<Column>>> {
    let mut columns = HashMap::new();
    for kind in Kind::ALL {
        let sql = format!("PRAGMA table_info({})", kind.table());
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql)).fetch_all(pool).await?;
        let list = rows
            .iter()
            .map(|row| {
                let name: String = row.get("name");
                let declared: String = row.get("type");
                let column_kind = ColumnKind::from_declared(&name, &declared);
                Column { name, kind: column_kind }
            })
            .collect();
        columns.insert(kind, list);
    }
    Ok(columns)
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
async fn open_guarded(data_dir: &Path, path: &Path) -> Result<(Option<PathBuf>, Option<std::fs::File>)> {
    use std::fs::TryLockError;

    let lock = std::fs::File::options().create(true).truncate(false).write(true).open(data_dir.join(LOCK_FILENAME));
    let lock = match lock {
        Ok(file) => file,
        Err(e) => return unguarded(path, &e).await,
    };
    match lock.try_lock() {
        Ok(()) => {
            let backup = upgrade(path, true).await;
            let shared = lock.unlock().and_then(|()| lock.lock_shared());
            let backup = backup?;
            if let Err(e) = shared {
                return Err(Error::Internal(format!("couldn't lock {}: {e}", data_dir.display())));
            }
            // Between unlock and lock_shared another copy could have changed the schema.
            upgrade(path, false).await?;
            Ok((backup, Some(lock)))
        }
        Err(TryLockError::WouldBlock) => {
            // Waits while another copy upgrades; on a blocking thread so the runtime keeps going.
            let lock = tokio::task::spawn_blocking(move || lock.lock_shared().map(|()| lock))
                .await
                .map_err(|e| Error::Internal(e.to_string()))?
                .map_err(|e| Error::Internal(format!("couldn't lock {}: {e}", data_dir.display())))?;
            Ok((upgrade(path, false).await?, Some(lock)))
        }
        Err(TryLockError::Error(e)) => unguarded(path, &e).await,
    }
}

async fn unguarded(path: &Path, why: &std::io::Error) -> Result<(Option<PathBuf>, Option<std::fs::File>)> {
    eprintln!("questodian: file locks aren't available here ({why}); open only one copy of Questodian at a time");
    Ok((upgrade(path, true).await?, None))
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
async fn read_state(conn: &mut SqliteConnection, path: &Path) -> Result<State> {
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table'").fetch_all(&mut *conn).await?;
    let has_app_tables = Kind::ALL.iter().any(|k| tables.iter().any(|t| t == k.table()));
    if !tables.iter().any(|t| t == "alembic_version") {
        // Tables but no version: made before migrations existed, so it has the baseline schema.
        return Ok(if has_app_tables { State::At(BASELINE.into()) } else { State::Empty });
    }
    let versions: Vec<String> =
        sqlx::query_scalar("SELECT version_num FROM alembic_version").fetch_all(&mut *conn).await?;
    match versions.as_slice() {
        [version] => Ok(State::At(version.clone())),
        [] if !has_app_tables => {
            // An empty database apart from an empty version table: start it afresh.
            sqlx::query("DROP TABLE alembic_version").execute(&mut *conn).await?;
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
async fn upgrade(path: &Path, alone: bool) -> Result<Option<PathBuf>> {
    // Foreign keys stay off while tables change, so nothing cascades; the pragma only
    // takes effect outside a transaction.
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(false)
        .busy_timeout(Duration::from_secs(10));
    let mut conn = SqliteConnection::connect_with(&options).await?;
    let result = upgrade_with(&mut conn, path, alone).await;
    conn.close().await?;
    result
}

async fn upgrade_with(conn: &mut SqliteConnection, path: &Path, alone: bool) -> Result<Option<PathBuf>> {
    let state = read_state(conn, path).await?;
    let version = match &state {
        State::Empty => {
            run_locked(conn, path, &state, |tx| Box::pin(async move { create_schema(tx).await })).await?;
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
    let moves = if version == BASELINE { encounters_to_move(conn, path).await? } else { Vec::new() };
    let backup = backup_database(conn, path, &version).await?;
    let from = version.clone();
    let upgraded = run_locked(conn, path, &state, move |tx| Box::pin(upgrade_from(tx, from, moves))).await;
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
async fn upgrade_from(conn: &mut SqliteConnection, version: String, moves: Vec<EncounterMove>) -> Result<()> {
    let mut at = version.as_str();
    if at == BASELINE {
        upgrade_0001_to_0002(conn, moves).await?;
        at = "0002";
    }
    let first = STEPS.iter().position(|(from, _)| *from == at).unwrap_or(STEPS.len());
    for (_, sql) in &STEPS[first..] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(*sql)).execute(&mut *conn).await?;
    }
    set_version(conn, HEAD).await
}

/// Record the schema revision (in Alembic's table, so every version of the app agrees).
async fn set_version(conn: &mut SqliteConnection, version: &str) -> Result<()> {
    // A database from before migrations has no version table yet.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS alembic_version (version_num VARCHAR(32) NOT NULL, \
         CONSTRAINT alembic_version_pkc PRIMARY KEY (version_num))",
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query("DELETE FROM alembic_version").execute(&mut *conn).await?;
    sqlx::query("INSERT INTO alembic_version (version_num) VALUES (?)").bind(version).execute(&mut *conn).await?;
    Ok(())
}

type Step<'c> = std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'c>>;

/// Run `step` in one write-locked transaction, if the version is still what was read
/// (another copy of the app may have upgraded the file in the meantime).
async fn run_locked<F>(conn: &mut SqliteConnection, path: &Path, expected: &State, step: F) -> Result<()>
where
    F: for<'c> FnOnce(&'c mut SqliteConnection) -> Step<'c>,
{
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let result = async {
        if read_state(conn, path).await? != *expected {
            return Ok(()); // someone else got there first
        }
        step(conn).await
    }
    .await;
    match result {
        Ok(()) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            Ok(())
        }
        Err(e) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(e)
        }
    }
}

async fn create_schema(conn: &mut SqliteConnection) -> Result<()> {
    sqlx::raw_sql(sqlx::AssertSqlSafe(SCHEMA_0002)).execute(&mut *conn).await?;
    for (_, sql) in STEPS {
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql)).execute(&mut *conn).await?;
    }
    set_version(conn, HEAD).await
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
async fn encounters_to_move(conn: &mut SqliteConnection, path: &Path) -> Result<Vec<EncounterMove>> {
    use crate::schemas::{Combatant, Lenient};

    let rows =
        sqlx::query("SELECT id, campaign_id, number, title, encounter FROM sessions").fetch_all(&mut *conn).await?;
    let mut moves = Vec::new();
    for row in rows {
        let session_id: String = row.try_get("id")?;
        let title: String = row.try_get("title").unwrap_or_default();
        let unreadable = |why: &str| {
            refused(path, &format!("the encounter saved with session '{title}' ({session_id}) can't be read ({why})"))
        };
        let encounter: Value = match row.try_get_raw("encounter")?.is_null() {
            true => Value::Null,
            false => match row.try_get::<String, _>("encounter") {
                Ok(text) => serde_json::from_str(&text).map_err(|_| unreadable("it isn't valid JSON"))?,
                Err(_) => return Err(unreadable("it isn't text")),
            },
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
        let number: i64 = row.try_get("number").unwrap_or(0);
        moves.push(EncounterMove {
            campaign_id: row.try_get("campaign_id")?,
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
async fn upgrade_0001_to_0002(conn: &mut SqliteConnection, moves: Vec<EncounterMove>) -> Result<()> {
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table'").fetch_all(&mut *conn).await?;
    if tables.iter().any(|t| t == "encounters") {
        return Err(Error::Internal("an earlier upgrade stopped halfway (the encounters table already exists)".into()));
    }
    sqlx::raw_sql(sqlx::AssertSqlSafe(UPGRADE_0001_TO_0002)).execute(&mut *conn).await?;
    let stamp = now();
    for m in moves {
        sqlx::query(
            "INSERT INTO encounters (id, created, updated, campaign_id, session_id, scene_id, name, round, turn, \
             active, combatants, party_size, party_level, log, notes) \
             VALUES (?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?, 1, '[]', '')",
        )
        .bind(new_id())
        .bind(&stamp)
        .bind(&stamp)
        .bind(m.campaign_id)
        .bind(m.session_id)
        .bind(m.name)
        .bind(m.round)
        .bind(m.turn)
        .bind(m.active)
        .bind(m.combatants.to_string())
        .bind(m.party_size)
        .execute(&mut *conn)
        .await?;
    }
    sqlx::query("ALTER TABLE sessions DROP COLUMN encounter").execute(&mut *conn).await?;
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
async fn backup_database(conn: &mut SqliteConnection, path: &Path, label: &str) -> Result<PathBuf> {
    let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let prefix = format!("{name}.backup-{label}-");
    let folder = path.parent().unwrap_or(Path::new("."));
    let existing = std::fs::read_dir(folder)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&prefix)))
        .max();
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let wal = folder.join(format!("{name}-wal"));
    let changed = [modified(path), modified(&wal)].into_iter().flatten().max();
    if let Some(existing) = existing
        && modified(&existing).zip(changed).is_some_and(|(backup, data)| backup >= data)
    {
        return Ok(existing);
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dest = folder.join(format!("{prefix}{stamp}"));
    sqlx::query("VACUUM INTO ?").bind(dest.to_string_lossy().into_owned()).execute(&mut *conn).await?;
    std::fs::File::open(&dest)?.sync_all()?;
    if let Ok(dir) = std::fs::File::open(folder) {
        let _ = dir.sync_all(); // not possible on every OS; the file itself is synced
    }
    Ok(dest)
}

/// For tests and tools: run a statement list (e.g. a fixture dump) on a database file.
pub async fn load_sql(path: &Path, sql: &str) -> Result<()> {
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))
        .map_err(|e| Error::Internal(e.to_string()))?
        .create_if_missing(true)
        .foreign_keys(false);
    let mut conn = SqliteConnection::connect_with(&options).await?;
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_string())).execute(&mut conn).await?;
    conn.close().await?;
    Ok(())
}
