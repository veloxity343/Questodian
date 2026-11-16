//! Opening real databases made by the Python app, at every schema version.
//!
//! Fixtures come from `scripts/make_fixtures.py`: a SQL dump of each database and every
//! record as the Python app reads it after upgrading. Rust must read the same records.

use std::path::Path;

use questodian_core::db::{self, DB_FILENAME, Database, HEAD};
use questodian_core::kinds::Kind;
use serde_json::{Value, json};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

async fn open_fixture(name: &str, dir: &Path) -> Database {
    let sql = std::fs::read_to_string(format!("{FIXTURES}/{name}.sql")).unwrap();
    db::load_sql(&dir.join(DB_FILENAME), &sql).await.unwrap();
    Database::open(dir).await.unwrap()
}

async fn all_records(db: &Database) -> Value {
    let mut repo = db.read().await.unwrap();
    let mut out = serde_json::Map::new();
    for kind in Kind::ALL {
        out.insert(kind.plural().into(), Value::Array(repo.list(kind, &[]).await.unwrap()));
    }
    Value::Object(out)
}

/// Encounters created by the upgrade get a random id and the current time.
fn hide_generated(mut records: Value) -> Value {
    for encounter in records["encounters"].as_array_mut().unwrap() {
        let id = encounter["id"].as_str().unwrap();
        if id.len() == 12 && id.chars().all(|c| c.is_ascii_hexdigit()) {
            for key in ["id", "created", "updated"] {
                encounter[key] = json!("generated");
            }
        }
    }
    records
}

async fn version(db: &Database) -> String {
    sqlx_version(&db.path).await
}

async fn sqlx_version(path: &Path) -> String {
    use sqlx::Connection;
    let mut conn = sqlx::SqliteConnection::connect(&format!("sqlite://{}", path.display())).await.unwrap();
    sqlx::query_scalar("SELECT version_num FROM alembic_version").fetch_one(&mut conn).await.unwrap()
}

async fn check_fixture(name: &str, expect_backup: bool) {
    let dir = tempfile::tempdir().unwrap();
    let db = open_fixture(name, dir.path()).await;
    assert_eq!(db.backup.is_some(), expect_backup, "{name}: backup");
    if let Some(backup) = &db.backup {
        assert!(backup.is_file(), "{name}: backup file exists");
    }
    assert_eq!(version(&db).await, HEAD);
    let expected: Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{FIXTURES}/{name}.expected.json")).unwrap()).unwrap();
    assert_eq!(
        hide_generated(all_records(&db).await),
        hide_generated(expected),
        "{name}: records differ from what Python reads"
    );
    db.close().await;
}

#[tokio::test]
async fn current_schema_reads_like_python() {
    check_fixture("v0002", false).await;
}

#[tokio::test]
async fn revision_0001_upgrades_and_moves_session_encounters() {
    check_fixture("v0001", true).await;
}

#[tokio::test]
async fn database_from_before_migrations_upgrades() {
    check_fixture("pre_alembic", true).await;
}

/// Each table's columns, foreign keys and indexes, for comparing schemas.
async fn schema_shape(path: &Path) -> Vec<String> {
    use sqlx::{Connection, Row};
    let mut conn = sqlx::SqliteConnection::connect(&format!("sqlite://{}", path.display())).await.unwrap();
    let mut out = Vec::new();
    for kind in Kind::ALL {
        let t = kind.table();
        for pragma in ["table_info", "foreign_key_list", "index_list"] {
            let rows =
                sqlx::query(sqlx::AssertSqlSafe(format!("PRAGMA {pragma}({t})"))).fetch_all(&mut conn).await.unwrap();
            let mut lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let cells: Vec<String> = (0..r.len())
                        .filter(|&i| !(pragma == "index_list" && i == 0)) // seq number
                        .filter(|&i| !(pragma == "foreign_key_list" && i <= 1)) // id, seq
                        .map(|i| {
                            r.try_get::<String, _>(i)
                                .or_else(|_| r.try_get::<i64, _>(i).map(|n| n.to_string()))
                                .unwrap_or_else(|_| "NULL".into())
                        })
                        .collect();
                    format!("{t} {pragma}: {}", cells.join(" | "))
                })
                .collect();
            lines.sort();
            out.extend(lines);
        }
    }
    out
}

#[tokio::test]
async fn upgraded_schema_matches_a_new_database() {
    let fresh = tempfile::tempdir().unwrap();
    Database::open(fresh.path()).await.unwrap().close().await;
    let upgraded = tempfile::tempdir().unwrap();
    open_fixture("v0001", upgraded.path()).await.close().await;
    assert_eq!(
        schema_shape(&upgraded.path().join(DB_FILENAME)).await,
        schema_shape(&fresh.path().join(DB_FILENAME)).await
    );
}

#[tokio::test]
async fn new_database_is_created_at_head() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path().join("nested")).await.unwrap();
    assert!(db.backup.is_none());
    assert_eq!(version(&db).await, HEAD);
    let mut repo = db.write().await.unwrap();
    let campaign = repo.create(Kind::Campaigns, None).await.unwrap();
    repo.commit().await.unwrap();
    assert_eq!(campaign["name"], "New Campaign");
    db.close().await;
}

#[tokio::test]
async fn newer_database_is_refused_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(DB_FILENAME);
    let sql = std::fs::read_to_string(format!("{FIXTURES}/v0002.sql")).unwrap();
    db::load_sql(&path, &sql).await.unwrap();
    db::load_sql(&path, "UPDATE alembic_version SET version_num = '0099';").await.unwrap();
    let before = std::fs::read(&path).unwrap();
    let error = Database::open(dir.path()).await.err().expect("refused");
    assert!(error.to_string().contains("newer version"), "{error}");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "no backup or other files");
}

#[tokio::test]
async fn deleting_a_campaign_cascades_and_unlinks() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_fixture("v0002", dir.path()).await;
    let mut repo = db.write().await.unwrap();
    repo.delete(Kind::Campaigns, "camp1").await.unwrap();
    repo.commit().await.unwrap();
    let records = all_records(&db).await;
    for kind in ["sessions", "scenes", "npcs", "notes", "encounters", "trackers", "handouts"] {
        assert_eq!(records[kind], json!([]), "{kind} go with their campaign");
    }
    assert_eq!(records["characters"][0]["campaign_id"], "", "characters are unlinked");
    assert_eq!(records["monsters"].as_array().unwrap().len(), 1, "shared monsters stay");
    db.close().await;
}

#[tokio::test]
async fn a_failed_unit_of_work_saves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).await.unwrap();
    {
        let mut repo = db.write().await.unwrap();
        repo.create(Kind::Handbook, Some(json!({"title": "kept?"}))).await.unwrap();
        assert!(repo.create(Kind::Characters, Some(json!({"classes": "bad"}))).await.is_err());
        // dropped without commit
    }
    assert_eq!(all_records(&db).await["handbook"], json!([]));
    db.close().await;
}
