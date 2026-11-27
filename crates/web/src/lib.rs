//! Questodian in a browser: the same core and API as the desktop app, answered inside a
//! dedicated Web Worker instead of over the network, with the database in the browser's
//! origin-private file system (OPFS), so it survives reloads and works offline.
//!
//! The page's worker calls [`open`] once, then [`request`] for each API call the UI makes.
//! Only one worker (one tab or window) can have the database open at a time: [`open`] fails
//! in a second one until the first closes.
//!
//! The service worker gives each save (any request but GET and HEAD) an id in the
//! `x-questodian-save` header, and sends it again if no answer came. A save with an id that
//! was already made isn't made again: it gets the first answer (see `Database::saved`).

#![cfg(target_family = "wasm")]

use std::sync::OnceLock;

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use questodian_core::db::Database;
use questodian_core::server::{self, App, Shared, Ui};
use tower::ServiceExt;
use wasm_bindgen::prelude::*;

static ROUTER: OnceLock<(Router, Shared)> = OnceLock::new();
/// Saves with an id take turns, so each id is recorded with its own save's commit.
static SAVES: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const SAVE_ID: &str = "x-questodian-save";

/// Free storage slots kept for new files: the journals of the database and the handouts store
/// while they write, and a backup before an upgrade. The storage holds one file per slot and
/// never grows by itself, so each open drops the unused slots and adds this many afresh.
const FREE_SLOTS: usize = 6;

/// Set up the browser storage and open (or create) the database. Returns SQLite's version.
#[wasm_bindgen]
pub async fn open() -> Result<String, JsError> {
    use sqlite_wasm_rs::WasmOsCallback;
    use sqlite_wasm_vfs::sahpool::{OpfsSAHPoolCfgBuilder, install};

    if ROUTER.get().is_some() {
        return Err(JsError::new("the database is already open"));
    }
    let storage_error = |e: sqlite_wasm_vfs::sahpool::OpfsSAHError| JsError::new(&format!("browser storage: {e}"));
    let config = OpfsSAHPoolCfgBuilder::new().directory("questodian").initial_capacity(FREE_SLOTS).build();
    let storage = install::<WasmOsCallback>(&config, true).await.map_err(storage_error)?;
    storage.reduce_capacity(usize::MAX).await.map_err(storage_error)?;
    storage.add_capacity(FREE_SLOTS).await.map_err(storage_error)?;
    let db = Database::open("").await.map_err(|e| JsError::new(&e.to_string()))?;
    let app = App::new(db, Ui::Missing);
    let _ = ROUTER.set((server::router(app.clone()), app));
    Ok(rusqlite_version())
}

fn rusqlite_version() -> String {
    // SAFETY: sqlite3_libversion returns a static, NUL-terminated string.
    unsafe { std::ffi::CStr::from_ptr(sqlite_wasm_rs::sqlite3_libversion()) }.to_string_lossy().into_owned()
}

/// An API response: its status, headers (as `[[name, value], ...]` JSON) and body bytes.
#[wasm_bindgen(getter_with_clone)]
pub struct Reply {
    pub status: u16,
    pub headers: String,
    pub body: Vec<u8>,
}

/// Answer one API request, e.g. `request("GET", "/api/campaigns", "[]", undefined)`.
/// `headers` is `[[name, value], ...]` as JSON.
#[wasm_bindgen]
pub async fn request(method: String, url: String, headers: String, body: Option<Vec<u8>>) -> Result<Reply, JsError> {
    let (router, app) = ROUTER.get().ok_or_else(|| JsError::new("the database isn't open"))?;
    let pairs: Vec<(String, String)> = serde_json::from_str(&headers).map_err(|e| JsError::new(&e.to_string()))?;
    let save = pairs.iter().find(|(name, _)| name.eq_ignore_ascii_case(SAVE_ID)).map(|(_, id)| id.clone());
    let save = save.filter(|_| method != "GET" && method != "HEAD");
    let mut builder = Request::builder().method(method.as_str()).uri(url);
    for (name, value) in pairs {
        builder = builder.header(name, value);
    }
    let req = builder.body(Body::from(body.unwrap_or_default())).map_err(|e| JsError::new(&e.to_string()))?;
    let Some(id) = save else {
        return answer(router, req).await;
    };
    let _turn = SAVES.lock().await;
    let failed = |e: questodian_core::error::Error| JsError::new(&e.to_string());
    match app.db.saved(&id).await.map_err(failed)? {
        Some(saved) => Ok(match saved.answer {
            Some((status, headers, body)) => Reply { status, headers, body },
            // Made, but the window closed before its answer was kept.
            None => Reply {
                status: 409,
                headers: r#"[["content-type","application/json"]]"#.into(),
                body: br#"{"detail":"This change was saved. Reload to see it."}"#.to_vec(),
            },
        }),
        None => {
            app.db.saving(Some(id.clone()));
            let reply = answer(router, req).await;
            let (status, headers, body) = match &reply {
                Ok(r) => (r.status, r.headers.as_str(), r.body.as_slice()),
                Err(_) => (500, "[]", &[][..]),
            };
            // The save stands even if its answer can't be kept: a resend then gets "saved".
            let _ = app.db.answered(&id, status, headers, body).await;
            reply
        }
    }
}

async fn answer(router: &Router, req: Request<Body>) -> Result<Reply, JsError> {
    let res = match router.clone().oneshot(req).await {
        Ok(res) => res,
        Err(never) => match never {},
    };
    let status = res.status().as_u16();
    let headers: Vec<(&str, &str)> =
        res.headers().iter().filter_map(|(k, v)| Some((k.as_str(), v.to_str().ok()?))).collect();
    let headers = serde_json::to_string(&headers).expect("strings serialize");
    let body = res.into_body().collect().await.map_err(|e| JsError::new(&e.to_string()))?.to_bytes().to_vec();
    Ok(Reply { status, headers, body })
}
