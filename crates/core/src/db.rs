//! The SQLite database: where it lives, opening it, and bringing its schema up to date.
//!
//! The schema version is kept in Alembic's `alembic_version` table, so the Python app
//! and this one agree on what a file contains. A database newer than this app knows
//! is refused untouched. An older one is copied to a backup file before upgrading.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Connection, Row, SqliteConnection, SqlitePool};

use crate::error::{Error, Result};
use crate::kinds::{Column, ColumnKind, Kind};
use crate::repo::{Repo, new_id, now};

pub const DB_FILENAME: &str = "questodian.db";

/// The newest schema this app understands.
pub const HEAD: &str = "0002";
/// What a database made before migrations existed contains.
const BASELINE: &str = "0001";

const SCHEMA_0002: &str = include_str!("migrations/0002_schema.sql");
const UPGRADE_0001_TO_0002: &str = include_str!("migrations/0001_to_0002.sql");

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
}

impl Database {
    /// Open (or create) `questodian.db` in `data_dir`, upgrading it if it's older.
    pub async fn open(data_dir: impl AsRef<Path>) -> Result<Database> {
        let data_dir = data_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&data_dir)?;
        let path = data_dir.join(DB_FILENAME);
        let backup = upgrade(&path).await?;

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(10));
        let pool = SqlitePoolOptions::new().max_connections(4).connect_with(options).await?;
        let columns = read_columns(&pool).await?;
        Ok(Database { pool, data_dir, path, backup, columns })
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

// --------------------------------------------------------------------------- migrations

/// The schema version stored in the file: `None` for a new, empty database.
async fn current_version(conn: &mut SqliteConnection) -> Result<Option<String>> {
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table'").fetch_all(&mut *conn).await?;
    if tables.iter().any(|t| t == "alembic_version") {
        let version: Option<String> =
            sqlx::query_scalar("SELECT version_num FROM alembic_version").fetch_optional(&mut *conn).await?;
        return Ok(version);
    }
    // Tables but no version: made before migrations existed, so it has the baseline schema.
    let known = Kind::ALL.iter().any(|k| tables.iter().any(|t| t == k.table()));
    Ok(known.then(|| BASELINE.to_string()))
}

/// Bring the file at `path` up to [`HEAD`]. Returns the backup made first, if any.
async fn upgrade(path: &Path) -> Result<Option<PathBuf>> {
    // Foreign keys stay off while tables change, so nothing cascades; the pragma only
    // takes effect outside a transaction.
    let options = SqliteConnectOptions::new().filename(path).create_if_missing(true).foreign_keys(false);
    let mut conn = SqliteConnection::connect_with(&options).await?;
    let version = current_version(&mut conn).await?;
    let result = match version.as_deref() {
        None => {
            create_schema(&mut conn).await?;
            Ok(None)
        }
        Some(HEAD) => Ok(None),
        Some(BASELINE) => {
            let backup = backup_database(&mut conn, path, BASELINE).await?;
            upgrade_0001_to_0002(&mut conn).await?;
            Ok(Some(backup))
        }
        Some(other) => Err(Error::Internal(format!(
            "{} was made by a newer version of Questodian (schema {other}); this version can't open it. \
             Nothing was changed.",
            path.display()
        ))),
    };
    conn.close().await?;
    result
}

async fn create_schema(conn: &mut SqliteConnection) -> Result<()> {
    let mut tx = conn.begin().await?;
    sqlx::raw_sql(sqlx::AssertSqlSafe(SCHEMA_0002)).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Revision 0002 added the DM screen tables and the campaign edition. Each session used
/// to hold one encounter; any with combatants become rows in the new encounters table
/// (linked back to the session) before the column is dropped. Mirrors
/// `src/questodian/migrations/versions/0002_dm_screen.py`.
async fn upgrade_0001_to_0002(conn: &mut SqliteConnection) -> Result<()> {
    let mut tx = conn.begin().await?;
    sqlx::raw_sql(sqlx::AssertSqlSafe(UPGRADE_0001_TO_0002)).execute(&mut *tx).await?;

    let sessions =
        sqlx::query("SELECT id, campaign_id, number, title, encounter FROM sessions").fetch_all(&mut *tx).await?;
    let stamp = now();
    for row in sessions {
        let raw: Option<String> = row.get("encounter");
        let encounter: Value = raw.as_deref().and_then(|text| serde_json::from_str(text).ok()).unwrap_or(Value::Null);
        let combatants = encounter.get("combatants").and_then(Value::as_array).cloned().unwrap_or_default();
        if combatants.is_empty() {
            continue;
        }
        let pcs = combatants.iter().filter(|c| c.get("kind").and_then(Value::as_str) == Some("pc")).count();
        let number: i64 = row.get("number");
        let title: String = row.get("title");
        sqlx::query(
            "INSERT INTO encounters (id, created, updated, campaign_id, session_id, scene_id, name, round, turn, \
             active, combatants, party_size, party_level, log, notes) \
             VALUES (?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?, 1, '[]', '')",
        )
        .bind(new_id())
        .bind(&stamp)
        .bind(&stamp)
        .bind(row.get::<Option<String>, _>("campaign_id"))
        .bind(row.get::<String, _>("id"))
        .bind(format!("Session {number}: {title}"))
        .bind(encounter.get("round").and_then(Value::as_i64).unwrap_or(0))
        .bind(encounter.get("turn").and_then(Value::as_i64).unwrap_or(0))
        .bind(encounter.get("active").is_some_and(truthy))
        .bind(Value::Array(combatants).to_string())
        .bind(pcs.max(1) as i64)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query("ALTER TABLE sessions DROP COLUMN encounter").execute(&mut *tx).await?;
    // A database from before migrations has no version table yet.
    sqlx::query("CREATE TABLE IF NOT EXISTS alembic_version (version_num VARCHAR(32) NOT NULL, CONSTRAINT alembic_version_pkc PRIMARY KEY (version_num))")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM alembic_version").execute(&mut *tx).await?;
    sqlx::query("INSERT INTO alembic_version (version_num) VALUES (?)").bind(HEAD).execute(&mut *tx).await?;
    tx.commit().await?;
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

/// Copy a live database safely (WAL included) to `<name>.backup-<label>-<time>`.
async fn backup_database(conn: &mut SqliteConnection, path: &Path, label: &str) -> Result<PathBuf> {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let dest = path.with_file_name(format!("{name}.backup-{label}-{stamp}"));
    sqlx::query("VACUUM INTO ?").bind(dest.to_string_lossy().into_owned()).execute(&mut *conn).await?;
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
