//! Proof: an axum Router and a SQLite database, both in WebAssembly in a browser worker. The
//! router is called as a plain function (no network); the database is a file in the browser's
//! origin-private file system (OPFS), so it survives a reload.

use std::sync::{Arc, Mutex, OnceLock};

use axum::{Json, Router, extract::State, routing::get};
use http_body_util::BodyExt;
use rusqlite::Connection;
use serde_json::{Value, json};
use tower::ServiceExt;
use wasm_bindgen::prelude::*;

type Db = Arc<Mutex<Connection>>;
static DB: OnceLock<Db> = OnceLock::new();

/// Make OPFS SQLite's default file system, then open (or create) the database.
#[wasm_bindgen]
pub async fn open() -> Result<String, JsValue> {
    use sqlite_wasm_rs::WasmOsCallback;
    use sqlite_wasm_vfs::sahpool::{OpfsSAHPoolCfg, install};
    install::<WasmOsCallback>(&OpfsSAHPoolCfg::default(), true).await.map_err(|e| JsValue::from_str(&format!("OPFS: {e:?}")))?;
    let conn = Connection::open("questodian.db").map_err(err)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS campaigns (id INTEGER PRIMARY KEY, name TEXT NOT NULL, created TEXT NOT NULL);",
    )
    .map_err(err)?;
    let version: String = conn.query_row("SELECT sqlite_version()", [], |r| r.get(0)).map_err(err)?;
    let _ = DB.set(Arc::new(Mutex::new(conn)));
    Ok(version)
}

fn err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

async fn list(State(db): State<Db>) -> Json<Value> {
    let conn = db.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id, name, created FROM campaigns ORDER BY id").unwrap();
    let rows: Vec<Value> = stmt
        .query_map([], |r| Ok(json!({"id": r.get::<_, i64>(0)?, "name": r.get::<_, String>(1)?, "created": r.get::<_, String>(2)?})))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    Json(json!({ "campaigns": rows }))
}

async fn create(State(db): State<Db>, Json(body): Json<Value>) -> Json<Value> {
    let conn = db.lock().unwrap();
    let name = body["name"].as_str().unwrap_or("New campaign");
    let created = body["created"].as_str().unwrap_or("");
    conn.execute("INSERT INTO campaigns (name, created) VALUES (?1, ?2)", (name, created)).unwrap();
    Json(json!({"id": conn.last_insert_rowid(), "name": name, "created": created}))
}

fn router(db: Db) -> Router {
    Router::new()
        .route("/api/hello", get(|| async { Json(json!({"hello": "from axum in WebAssembly"})) }))
        .route("/api/campaigns", get(list).post(create))
        .with_state(db)
}

/// Send one request through the router and return the response body as text.
#[wasm_bindgen]
pub async fn request(method: String, path: String, body: Option<String>) -> Result<String, JsValue> {
    let db = DB.get().ok_or_else(|| JsValue::from_str("call open() first"))?.clone();
    let req = axum::http::Request::builder()
        .method(method.as_str())
        .uri(path)
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body.unwrap_or_default()))
        .map_err(err)?;
    let res = router(db).oneshot(req).await.map_err(err)?;
    let bytes = res.into_body().collect().await.map_err(err)?.to_bytes();
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
