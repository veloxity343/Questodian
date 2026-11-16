//! The HTTP API (everything under `/api`) and the built UI, as one axum [`Router`].
//!
//! The desktop app calls this router directly, without a network port; `questodian
//! serve` puts it behind a real HTTP server for browsers and the players' screen.

use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, FromRequest, FromRequestParts, Path as UrlPath, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

use crate::db::Database;
use crate::error::{Error, Result, invalid, not_found};
use crate::kinds::Kind;
use crate::schemas::{
    self, Award, Character, Check, ClassesIn, EncounterAction, Expression, Lenient, LogIn, Projection, RestIn,
};
use crate::{dice, game, rules, srd, transfer};

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
    pub db: Database,
    /// What the players' screen shows. Kept in memory; the players' window polls it.
    pub projection: Mutex<Projection>,
    pub ui: Ui,
}

pub type Shared = Arc<App>;

impl App {
    pub fn new(db: Database, ui: Ui) -> Shared {
        Arc::new(App { db, projection: Mutex::new(Projection::default()), ui })
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
        .route("/encounters/{id}/action", post(encounter_action))
        .route("/sessions/{id}/award", post(award))
        .route("/sessions/{id}/log", post(add_log))
        .route("/characters/{id}/rest", post(rest))
        .route("/handouts/{id}/file", get(handout_file).put(upload_handout))
        .route("/handouts/{id}", axum::routing::delete(delete_handout).get(read_handout).put(update_handout))
        .route("/export", get(export_bundle))
        .route("/export/{kind}/{id}", get(export_one))
        .route("/import", post(import_data))
        .route("/import/markdown", post(import_markdown))
        .route("/{kind}", get(list_records).post(create_record))
        .route("/{kind}/{id}", get(read_record).put(update_record).delete(delete_record))
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
    let mut out = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "data_dir": app.db.data_dir.display().to_string(),
        "database": app.db.path.display().to_string(),
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
    let slots = if rules::is_warlock_only(&body.classes) {
        rules::pact_slots(rules::total_level(&body.classes), body.edition)
    } else {
        rules::spell_slots_for(&body.classes, body.edition)
    };
    Json(json!({ "slots": slots }))
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

// --------------------------------------------------------------------------- records

fn kind_from_path(name: &str) -> Result<Kind> {
    Kind::from_plural(name).ok_or_else(|| not_found("not found"))
}

/// A JSON body that may be empty (`None`). Malformed JSON is a 422.
pub struct JsonBody(pub Option<Value>);

impl<S: Send + Sync> FromRequest<S> for JsonBody {
    type Rejection = Error;

    async fn from_request(req: Request, state: &S) -> Result<Self> {
        let bytes = Bytes::from_request(req, state).await.map_err(|e| match e.status() {
            StatusCode::PAYLOAD_TOO_LARGE => Error::TooLarge,
            _ => body_error(e.body_text()),
        })?;
        if bytes.is_empty() {
            return Ok(JsonBody(None));
        }
        serde_json::from_slice(&bytes).map(|v| JsonBody(Some(v))).map_err(|e| body_error(format!("invalid JSON: {e}")))
    }
}

/// An object body, or 422 (FastAPI's `dict` bodies).
fn object_body(body: Option<Value>, required: bool) -> Result<Option<Value>> {
    match body {
        Some(Value::Object(map)) => Ok(Some(Value::Object(map))),
        None | Some(Value::Null) if !required => Ok(None),
        _ => Err(body_error("Input should be a valid dictionary")),
    }
}

async fn list_records(
    State(app): State<Shared>,
    UrlPath((kind,)): UrlPath<(String,)>,
    params: Params,
) -> Result<Json<Value>> {
    let kind = kind_from_path(&kind)?;
    let filters = [
        ("campaign_id", params.text("campaign_id")),
        ("session_id", params.text("session_id")),
        ("kind", params.text("kind")),
    ];
    let filters: Vec<(&str, &str)> = filters.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let docs = app.db.read().await?.list(kind, &filters).await?;
    Ok(Json(json!({ kind.plural(): docs })))
}

async fn create_record(
    State(app): State<Shared>,
    UrlPath((kind,)): UrlPath<(String,)>,
    JsonBody(body): JsonBody,
) -> Result<(StatusCode, Json<Value>)> {
    let kind = kind_from_path(&kind)?;
    let body = object_body(body, false)?;
    let mut repo = app.db.write().await?;
    let doc = repo.create(kind, body).await?;
    repo.commit().await?;
    Ok((StatusCode::CREATED, Json(doc)))
}

async fn read_record(State(app): State<Shared>, UrlPath((kind, id)): UrlPath<(String, String)>) -> Result<Json<Value>> {
    let kind = kind_from_path(&kind)?;
    Ok(Json(app.db.read().await?.get(kind, &id).await?))
}

async fn update_record(
    State(app): State<Shared>,
    UrlPath((kind, id)): UrlPath<(String, String)>,
    JsonBody(body): JsonBody,
) -> Result<Json<Value>> {
    let kind = kind_from_path(&kind)?;
    let body = object_body(body, true)?.expect("required");
    let mut repo = app.db.write().await?;
    let doc = repo.update(kind, &id, body).await?;
    repo.commit().await?;
    Ok(Json(doc))
}

async fn delete_record(
    State(app): State<Shared>,
    UrlPath((kind, id)): UrlPath<(String, String)>,
) -> Result<Json<Value>> {
    let kind = kind_from_path(&kind)?;
    let mut repo = app.db.write().await?;
    repo.delete(kind, &id).await?;
    repo.commit().await?;
    Ok(Json(json!({"deleted": id})))
}

// --------------------------------------------------------------------------- game

async fn encounter_action(
    State(app): State<Shared>,
    UrlPath((id,)): UrlPath<(String,)>,
    Valid(body): Valid<EncounterAction>,
) -> Result<Json<Value>> {
    let mut repo = app.db.write().await?;
    let doc = game::encounter_action(&mut repo, &id, body).await?;
    repo.commit().await?;
    Ok(Json(doc))
}

async fn award(
    State(app): State<Shared>,
    UrlPath((id,)): UrlPath<(String,)>,
    Valid(body): Valid<Award>,
) -> Result<Json<Value>> {
    let mut repo = app.db.write().await?;
    let doc = game::award(&mut repo, &id, body).await?;
    repo.commit().await?;
    Ok(Json(doc))
}

async fn add_log(
    State(app): State<Shared>,
    UrlPath((id,)): UrlPath<(String,)>,
    Valid(body): Valid<LogIn>,
) -> Result<Json<Value>> {
    let mut repo = app.db.write().await?;
    let doc = game::add_log(&mut repo, &id, body).await?;
    repo.commit().await?;
    Ok(Json(doc))
}

async fn rest(
    State(app): State<Shared>,
    UrlPath((id,)): UrlPath<(String,)>,
    Valid(body): Valid<RestIn>,
) -> Result<Json<Value>> {
    let mut repo = app.db.write().await?;
    let doc = game::rest(&mut repo, &id, body.r#type).await?;
    repo.commit().await?;
    Ok(Json(doc))
}

// --------------------------------------------------------------------------- handouts

/// Image types a handout may hold, and the file extension each is stored with.
const IMAGE_TYPES: [(&str, &str); 5] = [
    ("image/png", ".png"),
    ("image/jpeg", ".jpg"),
    ("image/gif", ".gif"),
    ("image/webp", ".webp"),
    ("image/svg+xml", ".svg"),
];
const MAX_IMAGE: usize = 15 * 1024 * 1024;

/// A stored handout file name is only ever a plain name inside the handouts folder.
fn handout_path(dir: &std::path::Path, filename: &str) -> Option<PathBuf> {
    let plain = !filename.is_empty()
        && Path::new(filename).components().all(|c| matches!(c, Component::Normal(_)))
        && !filename.contains(['/', '\\']);
    plain.then(|| dir.join(filename))
}

/// Store the request body (an image) as this handout's picture.
async fn upload_handout(
    State(app): State<Shared>,
    UrlPath((id,)): UrlPath<(String,)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>> {
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let Some((_, extension)) = IMAGE_TYPES.iter().find(|(m, _)| *m == mime) else {
        let types: Vec<&str> = IMAGE_TYPES.iter().map(|(m, _)| *m).collect();
        return Err(invalid(format!("handouts must be images ({})", types.join(", "))));
    };
    if body.is_empty() || body.len() > MAX_IMAGE {
        return Err(invalid("image is empty or larger than 15 MB"));
    }
    let dir = app.db.files_dir();
    tokio::fs::create_dir_all(&dir).await?;
    let mut repo = app.db.write().await?;
    let doc = repo.get(Kind::Handouts, &id).await?;
    let old = doc["filename"].as_str().unwrap_or("").to_string();
    let name = format!("{id}-{}{extension}", &crate::repo::new_id()[..6]);
    tokio::fs::write(dir.join(&name), &body).await?;
    let saved = repo.update(Kind::Handouts, &id, json!({"filename": name, "mime": mime})).await?;
    repo.commit().await?;
    if old != name
        && let Some(path) = handout_path(&dir, &old)
    {
        let _ = tokio::fs::remove_file(path).await;
    }
    Ok(Json(saved))
}

async fn handout_file(State(app): State<Shared>, UrlPath((id,)): UrlPath<(String,)>) -> Result<Response> {
    let doc = app.db.read().await?.get(Kind::Handouts, &id).await?;
    let filename = doc["filename"].as_str().unwrap_or("");
    let path = handout_path(&app.db.files_dir(), filename).ok_or_else(|| not_found("this handout has no image yet"))?;
    let bytes = tokio::fs::read(&path).await.map_err(|_| not_found("this handout has no image yet"))?;
    let mime = doc["mime"].as_str().filter(|m| !m.is_empty()).map(String::from);
    let mime = mime.unwrap_or_else(|| mime_guess::from_path(&path).first_or_octet_stream().essence_str().to_string());
    Ok(([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "no-cache".to_string())], Body::from(bytes))
        .into_response())
}

/// Deleting a handout also removes its image file.
async fn delete_handout(State(app): State<Shared>, UrlPath((id,)): UrlPath<(String,)>) -> Result<Json<Value>> {
    let mut repo = app.db.write().await?;
    let doc = repo.get(Kind::Handouts, &id).await?;
    repo.delete(Kind::Handouts, &id).await?;
    repo.commit().await?;
    if let Some(path) = handout_path(&app.db.files_dir(), doc["filename"].as_str().unwrap_or("")) {
        let _ = tokio::fs::remove_file(path).await;
    }
    Ok(Json(json!({"deleted": id})))
}

async fn read_handout(state: State<Shared>, UrlPath((id,)): UrlPath<(String,)>) -> Result<Json<Value>> {
    read_record(state, UrlPath(("handouts".to_string(), id))).await
}

async fn update_handout(
    state: State<Shared>,
    UrlPath((id,)): UrlPath<(String,)>,
    body: JsonBody,
) -> Result<Json<Value>> {
    update_record(state, UrlPath(("handouts".to_string(), id)), body).await
}

// --------------------------------------------------------------------------- import / export

/// `My Campaign!` -> `my-campaign` (for file names).
fn slug(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() { "export".into() } else { out }
}

/// `Content-Disposition` for a download, with the name percent-encoded as UTF-8.
fn attachment(filename: &str) -> (header::HeaderName, String) {
    let mut encoded = String::new();
    for byte in filename.bytes() {
        if byte.is_ascii_alphanumeric() || b"_.-~/".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    (header::CONTENT_DISPOSITION, format!("attachment; filename*=UTF-8''{encoded}"))
}

async fn export_bundle(State(app): State<Shared>, params: Params) -> Result<Response> {
    let campaign_id = params.text("campaign_id");
    let mut repo = app.db.read().await?;
    let campaign = (!campaign_id.is_empty()).then_some(campaign_id.as_str());
    let bundle = transfer::export_bundle(&mut repo, campaign).await?;
    let name = match campaign {
        Some(id) => repo.get(Kind::Campaigns, id).await?["name"].as_str().unwrap_or("").to_string(),
        None => "questodian-backup".into(),
    };
    Ok(([attachment(&format!("{}.questodian.json", slug(&name)))], Json(bundle)).into_response())
}

async fn export_one(
    State(app): State<Shared>,
    UrlPath((kind, id)): UrlPath<(String, String)>,
    params: Params,
) -> Result<Response> {
    let kind = Kind::from_plural(&kind).ok_or_else(|| not_found("unknown kind"))?;
    let mut repo = app.db.read().await?;
    let doc = repo.get(kind, &id).await?;
    let title = slug(&crate::repo::title_of(&doc));
    if params.get("format").unwrap_or("json") != "md" {
        let envelope = transfer::export_one(&mut repo, kind, &id).await?;
        return Ok(([attachment(&format!("{title}.{}.json", kind.singular()))], Json(envelope)).into_response());
    }
    let text = match kind {
        Kind::Characters => {
            let character: Character = serde_json::from_value(doc).expect("normalized records fit their schema");
            let edition = game::edition_of(&mut repo, &character.campaign_id).await?;
            transfer::character_markdown(&character, edition)
        }
        Kind::Sessions => {
            let session: schemas::Session = serde_json::from_value(doc).expect("normalized records fit their schema");
            let campaign = if session.campaign_id.is_empty() {
                String::new()
            } else {
                repo.get(Kind::Campaigns, &session.campaign_id).await?["name"].as_str().unwrap_or("").to_string()
            };
            transfer::session_markdown(&session, &campaign)
        }
        Kind::Handbook => {
            format!("# {}\n\n{}\n", doc["title"].as_str().unwrap_or(""), doc["body"].as_str().unwrap_or(""))
        }
        _ => return Err(invalid("Markdown export is available for characters, sessions and handbook entries")),
    };
    let headers =
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8".to_string()), attachment(&format!("{title}.md"))];
    Ok((headers, text).into_response())
}

async fn import_data(State(app): State<Shared>, params: Params, JsonBody(body): JsonBody) -> Result<Json<Value>> {
    let mode = transfer::Mode::parse(params.get("mode").unwrap_or("overwrite"))?;
    let payload = body.ok_or_else(|| body_error("Field required"))?;
    let mut repo = app.db.write().await?;
    let summary = transfer::import_data(&mut repo, &payload, mode).await?;
    repo.commit().await?;
    Ok(Json(summary))
}

async fn import_markdown(
    State(app): State<Shared>,
    Valid(body): Valid<schemas::MarkdownImport>,
) -> Result<Json<Value>> {
    let mut repo = app.db.write().await?;
    let mut created = Vec::new();
    for file in body.files {
        let mut entry = transfer::markdown_to_handbook(&file.text, &file.filename);
        entry["campaign_id"] = json!(body.campaign_id);
        created.push(repo.create(Kind::Handbook, Some(entry)).await?);
    }
    repo.commit().await?;
    Ok(Json(json!({"created": created})))
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
