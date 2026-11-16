//! The HTTP API (everything under `/api`) and the built UI, as one axum [`Router`].
//!
//! The desktop app calls this router directly, without a network port; `questodian
//! serve` puts it behind a real HTTP server for browsers and the players' screen.

use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, FromRequest, FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

use crate::error::{Error, Result, invalid, not_found};
use crate::schemas::{self, Character, Check, ClassesIn, Expression, Lenient, Projection};
use crate::{dice, rules, srd};

/// Request bodies above this are refused with 413 (imports with images can be large).
pub const MAX_BODY: usize = 20 * 1024 * 1024;

/// Where the built UI comes from.
pub enum Ui {
    /// Not built: `/` explains how to build it.
    Missing,
    /// A folder on disk, e.g. `src/questodian/web`.
    Folder(PathBuf),
    /// Files compiled into the binary, looked up by path (`index.html`, `assets/app.js`).
    Embedded(fn(&str) -> Option<Cow<'static, [u8]>>),
}

/// Everything the routes share.
pub struct App {
    pub data_dir: Option<PathBuf>,
    pub database_path: Option<PathBuf>,
    /// What the players' screen shows. Kept in memory; the players' window polls it.
    pub projection: Mutex<Projection>,
    pub ui: Ui,
}

pub type Shared = Arc<App>;

impl App {
    pub fn new(data_dir: Option<PathBuf>, database_path: Option<PathBuf>, ui: Ui) -> Shared {
        Arc::new(App { data_dir, database_path, projection: Mutex::new(Projection::default()), ui })
    }
}

pub fn router(app: Shared) -> Router {
    let api = Router::new()
        .route("/meta", get(meta))
        .route("/roll", get(roll_get).post(roll_post))
        .route("/stats", post(stats))
        .route("/derive", post(derive))
        .route("/slots", post(slots))
        .route("/encounter-budget", get(encounter_budget))
        .route("/srd/categories", get(srd_categories))
        .route("/srd/search", get(srd_search))
        .route("/srd/entry", get(srd_entry))
        .route("/projection", get(get_projection).put(set_projection))
        .fallback(|| async { not_found("not found") });

    Router::new()
        .nest("/api", api)
        .fallback(serve_ui)
        .layer(middleware::from_fn(guard_writes))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(app)
}

// --------------------------------------------------------------------------- request helpers

/// A JSON request body, read leniently into a schema and range-checked. Problems are 422s.
pub struct Valid<T>(pub T);

impl<S: Send + Sync, T: Lenient + Check> FromRequest<S> for Valid<T> {
    type Rejection = Error;

    async fn from_request(req: Request, state: &S) -> Result<Self> {
        let bytes = Bytes::from_request(req, state).await.map_err(|e| match e.status() {
            StatusCode::PAYLOAD_TOO_LARGE => Error::TooLarge,
            _ => body_error(e.body_text()),
        })?;
        let value: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).map_err(|e| body_error(format!("invalid JSON: {e}")))?
        };
        schemas::validate(value).map(Valid).map_err(body_error)
    }
}

fn body_error(message: impl Into<String>) -> Error {
    Error::Unprocessable { location: vec!["body".into()], message: message.into() }
}

/// The query string, with typed lookups that fail with 422 like FastAPI's.
pub struct Params(pub Vec<(String, String)>);

impl<S: Send + Sync> FromRequestParts<S> for Params {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        let query = parts.uri.query().unwrap_or("");
        let pairs = form_urlencoded_pairs(query);
        Ok(Params(pairs))
    }
}

impl Params {
    /// The last value given for `name` (FastAPI takes the last one too).
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.iter().rev().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    pub fn text(&self, name: &str) -> String {
        self.get(name).unwrap_or("").to_string()
    }

    pub fn required(&self, name: &str) -> Result<String> {
        self.get(name).map(String::from).ok_or_else(|| query_error(name, "Field required"))
    }

    /// A whole number between `low` and `high`, or `default` when absent.
    pub fn int(&self, name: &str, default: i64, low: i64, high: i64) -> Result<i64> {
        let Some(raw) = self.get(name) else { return Ok(default) };
        let n = i64::from_json(Value::String(raw.to_string())).map_err(|e| query_error(name, e))?;
        if !(low..=high).contains(&n) {
            return Err(query_error(name, format!("must be from {low} to {high}")));
        }
        Ok(n)
    }

    pub fn edition(&self) -> Result<schemas::Edition> {
        srd::parse_edition(self.get("edition").unwrap_or("")).map_err(Error::Invalid)
    }
}

fn query_error(name: &str, message: impl Into<String>) -> Error {
    Error::Unprocessable { location: vec!["query".into(), name.into()], message: message.into() }
}

/// Split `a=1&b=two%20words` into decoded pairs.
fn form_urlencoded_pairs(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            (percent_decode(key, true), percent_decode(value, true))
        })
        .collect()
}

/// Decode `%XX` escapes (and, in query strings, `+` as a space).
fn percent_decode(text: &str, plus_is_space: bool) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let escaped = (bytes[i] == b'%')
            .then(|| bytes.get(i + 1..i + 3))
            .flatten()
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match (bytes[i], escaped) {
            (b'%', Some(decoded)) => {
                out.push(decoded);
                i += 3;
                continue;
            }
            (b'+', _) if plus_is_space => out.push(b' '),
            (byte, _) => out.push(byte),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// --------------------------------------------------------------------------- tools

async fn meta(State(app): State<Shared>) -> Json<Value> {
    let path = |p: &Option<PathBuf>| p.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
    let mut out = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "data_dir": path(&app.data_dir),
        "database": path(&app.database_path),
        "srd": srd::meta(schemas::Edition::E2014),
        "editions": {"2014": srd::edition_label(schemas::Edition::E2014), "2024": srd::edition_label(schemas::Edition::E2024)},
    });
    if let (Value::Object(out), Value::Object(tables)) = (&mut out, rules::tables_json()) {
        out.extend(tables);
    }
    Json(out)
}

fn roll_times(expression: &str, times: i64) -> Result<Value> {
    if expression.trim().is_empty() {
        return Err(invalid("missing expression"));
    }
    let mut results = Vec::with_capacity(times as usize);
    for _ in 0..times {
        results.push(serde_json::to_value(dice::roll(expression)?).expect("rolls serialize"));
    }
    Ok(if times == 1 { results.remove(0) } else { json!({"results": results}) })
}

async fn roll_get(params: Params) -> Result<Json<Value>> {
    let times = params.int("times", 1, 1, 100)?;
    Ok(Json(roll_times(&params.required("expr")?, times)?))
}

async fn roll_post(params: Params, Valid(body): Valid<Expression>) -> Result<Json<Value>> {
    let times = params.int("times", 1, 1, 100)?;
    Ok(Json(roll_times(&body.expression, times)?))
}

async fn stats() -> Json<Value> {
    Json(json!({"results": dice::roll_stats()}))
}

async fn derive(params: Params, Valid(character): Valid<Character>) -> Result<Json<Value>> {
    let derived = rules::derive(&character, params.edition()?);
    Ok(Json(serde_json::to_value(derived).expect("derived numbers serialize")))
}

async fn slots(Valid(body): Valid<ClassesIn>) -> Json<Value> {
    Json(json!({"slots": rules::spell_slots_for(&body.classes, body.edition)}))
}

async fn encounter_budget(params: Params) -> Result<Json<Value>> {
    let size = params.int("party_size", 4, i64::MIN, i64::MAX)?;
    let level = params.int("party_level", 1, i64::MIN, i64::MAX)?;
    let xp = params.int("xp", 0, i64::MIN, i64::MAX)?;
    Ok(Json(json!({
        "budget": rules::encounter_budget(size, level),
        "difficulty": rules::encounter_difficulty(xp, size, level),
    })))
}

async fn srd_categories(params: Params) -> Result<Json<Value>> {
    let edition = params.edition()?;
    Ok(Json(json!({"categories": srd::categories(edition), "meta": srd::meta(edition)})))
}

async fn srd_search(params: Params) -> Result<Json<Value>> {
    let limit = params.int("limit", 50, 1, 500)?;
    let offset = params.int("offset", 0, 0, i64::MAX)?;
    let edition = params.edition()?;
    // Any other query parameter filters on the entry's structured data (level=3, cr_label=1/4, ...).
    const RESERVED: [&str; 5] = ["q", "category", "limit", "offset", "edition"];
    let mut filters: Vec<(String, String)> = Vec::new();
    for (key, value) in &params.0 {
        if RESERVED.contains(&key.as_str()) {
            continue;
        }
        // Like a dict built from the query: a repeated key keeps its last value.
        filters.retain(|(k, _)| k != key);
        filters.push((key.clone(), value.clone()));
    }
    let found =
        srd::search(edition, &params.text("q"), &params.text("category"), &filters, limit as usize, offset as usize);
    Ok(Json(serde_json::to_value(found).expect("search results serialize")))
}

async fn srd_entry(params: Params) -> Result<Json<Value>> {
    let id = params.required("id")?;
    let entry = srd::get(params.edition()?, &id).ok_or_else(|| not_found("entry not found"))?;
    Ok(Json(serde_json::to_value(entry).expect("entries serialize")))
}

async fn get_projection(State(app): State<Shared>) -> Json<Projection> {
    Json(app.projection.lock().expect("projection lock").clone())
}

async fn set_projection(State(app): State<Shared>, Valid(mut body): Valid<Projection>) -> Json<Projection> {
    let mut current = app.projection.lock().expect("projection lock");
    body.seq = current.seq + 1;
    *current = body.clone();
    Json(body)
}

// --------------------------------------------------------------------------- write guard

/// Refuse writes from other websites open in the same browser (their `Origin` won't
/// match our `Host`), and request bodies that say they're over the limit.
async fn guard_writes(headers: HeaderMap, request: Request, next: Next) -> Response {
    let is_write = !matches!(*request.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if is_write && request.uri().path().starts_with("/api/") {
        if let Some(origin) = headers.get(header::ORIGIN) {
            let origin_host = origin.to_str().ok().map(host_of).unwrap_or_default();
            let host = headers.get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("");
            if origin_host != host {
                return Error::CrossOrigin.into_response();
            }
        }
        let length =
            headers.get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<usize>().ok());
        if length.is_some_and(|n| n > MAX_BODY) {
            return Error::TooLarge.into_response();
        }
    }
    next.run(request).await
}

/// `http://localhost:8765/x` -> `localhost:8765`; anything without a scheme -> "".
fn host_of(url: &str) -> String {
    let Some((_, rest)) = url.split_once("://") else { return String::new() };
    rest.split(['/', '?', '#']).next().unwrap_or("").to_string()
}

// --------------------------------------------------------------------------- the UI

const NOT_BUILT: &str = "<h1>Questodian</h1><p>The UI hasn't been built yet. Run <code>npm run build</code> \
in <code>frontend/</code>, or use the Vite dev server (<code>npm run dev</code>).</p>";

/// Serve a file of the built UI, or `index.html` for client-side routes such as `/characters/abc`.
async fn serve_ui(State(app): State<Shared>, request: Request) -> Response {
    let path = request.uri().path().trim_start_matches('/');
    let path = percent_decode(path, false);
    if path.starts_with("api/") {
        return not_found("not found").into_response();
    }
    if matches!(app.ui, Ui::Missing) {
        return if path.is_empty() { Html(NOT_BUILT).into_response() } else { not_found("Not Found").into_response() };
    }
    let safe = !path.is_empty() && Path::new(&path).components().all(|c| matches!(c, Component::Normal(_)));
    if safe && let Some(bytes) = ui_file(&app.ui, &path) {
        return file_response(&path, bytes);
    }
    if path.starts_with("assets/") {
        return not_found("Not Found").into_response();
    }
    match ui_file(&app.ui, "index.html") {
        Some(bytes) => file_response("index.html", bytes),
        None => Html(NOT_BUILT).into_response(),
    }
}

fn ui_file(ui: &Ui, path: &str) -> Option<Cow<'static, [u8]>> {
    match ui {
        Ui::Missing => None,
        Ui::Folder(root) => std::fs::read(root.join(path)).ok().map(Cow::Owned),
        Ui::Embedded(lookup) => lookup(path),
    }
}

fn file_response(path: &str, bytes: Cow<'static, [u8]>) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    ([(header::CONTENT_TYPE, mime.essence_str().to_string())], Body::from(bytes)).into_response()
}
