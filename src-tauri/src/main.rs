//! Questodian as a desktop app.
//!
//! The window shows `questodian://localhost/` (`http://questodian.localhost/` on
//! Windows and Android). Every request to that address goes straight to the same
//! router `questodian serve` uses, inside this process: the UI from files compiled
//! into the binary, and the API under `/api`. Nothing listens on the network.
//!
//! Set `QUESTODIAN_LOG=1` to print every request. A few addresses belong to the desktop
//! shell itself: `POST /desktop/players` opens (or brings back) the players' window, for
//! showing a handout on a second screen, and `/desktop/updates` is the update check
//! (see `updates.rs`).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod updates;

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, Response, StatusCode, header};
use questodian_core::db::{self, Database};
use questodian_core::server::{self, App, Ui};
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;
use tower::ServiceExt;
use updates::Updates;

const SCHEME: &str = "questodian";

/// The built UI, compiled in (`npm run build` in `frontend/` first).
#[derive(rust_embed::RustEmbed)]
#[folder = "../frontend/dist"]
struct Web;

fn ui_file(path: &str) -> Option<Cow<'static, [u8]>> {
    Web::get(path).map(|file| file.data)
}

/// Where the app's pages live on this platform.
fn base_url() -> Url {
    let url = if cfg!(any(windows, target_os = "android")) {
        format!("http://{SCHEME}.localhost/")
    } else {
        format!("{SCHEME}://localhost/")
    };
    url.parse().expect("a valid URL")
}

fn main() {
    let builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());
    #[cfg(feature = "updater")]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    builder
        .register_asynchronous_uri_scheme_protocol(SCHEME, |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                responder.respond(handle(&app, request).await);
            });
        })
        .setup(|app| {
            let data_dir = db::default_data_dir();
            let database = tauri::async_runtime::block_on(Database::open(&data_dir))?;
            app.manage(server::router(App::new(database, Ui::Embedded(ui_file))));
            let updates = Arc::new(Updates::load(updates::settings_path(app.path().app_config_dir()?)));
            app.manage(updates.clone());
            // Check when due: at start, then every half hour (a check is due once a day).
            tauri::async_runtime::spawn(async move {
                loop {
                    if updates.due() {
                        updates.check_now().await;
                    }
                    tokio::time::sleep(Duration::from_secs(30 * 60)).await;
                }
            });
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(base_url()))
                .title(format!("Questodian {}", env!("CARGO_PKG_VERSION")))
                .inner_size(1600.0, 960.0)
                .min_inner_size(900.0, 600.0)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Questodian failed to start");
}

/// Answer one request from the app's windows.
async fn handle(app: &AppHandle, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    if request.method() == Method::POST && request.uri().path() == "/desktop/players" {
        let path = String::from_utf8_lossy(request.body()).into_owned();
        return match open_players(app, &path) {
            Ok(()) => status(StatusCode::NO_CONTENT),
            Err(e) => {
                eprintln!("questodian: couldn't open the players' window: {e}");
                status(StatusCode::INTERNAL_SERVER_ERROR)
            }
        };
    }
    if let Some(rest) = request.uri().path().strip_prefix("/desktop/updates") {
        let (method, rest) = (request.method().clone(), rest.to_string());
        return updates_route(app, &method, &rest, request.body()).await;
    }
    let (mut parts, body) = request.into_parts();
    let (method, path) = (parts.method.clone(), parts.uri.path().to_string());
    // Only this app's own windows can reach this scheme, so the API's guard against
    // other websites doesn't apply (and their Origin wouldn't match a Host anyway).
    parts.headers.remove(header::ORIGIN);
    let router = app.state::<axum::Router>().inner().clone();
    let response = router.oneshot(Request::from_parts(parts, Body::from(body))).await.expect("the router never fails");
    let (parts, body) = response.into_parts();
    if std::env::var_os("QUESTODIAN_LOG").is_some() {
        eprintln!("{method} {path} -> {}", parts.status.as_u16());
    }
    let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap_or_default();
    Response::from_parts(parts, bytes.to_vec())
}

/// `GET` the state; `PUT {check, announced}`; `POST /check`, `/install` or `/page`.
async fn updates_route(app: &AppHandle, method: &Method, rest: &str, body: &[u8]) -> Response<Vec<u8>> {
    let updates = app.state::<Arc<Updates>>().inner().clone();
    match (method.as_str(), rest) {
        ("GET", "") => {}
        ("PUT", "") => {
            #[derive(serde::Deserialize)]
            struct Change {
                check: Option<bool>,
                announced: Option<String>,
            }
            let Ok(change) = serde_json::from_slice::<Change>(body) else { return status(StatusCode::BAD_REQUEST) };
            updates.change(change.check, change.announced.as_deref());
            // Turning checks on checks straight away if one is due.
            if updates.due() {
                tauri::async_runtime::spawn(async move { updates.check_now().await });
            }
        }
        ("POST", "/check") => updates.check_now().await,
        ("POST", "/page") => {
            // Only ever a page this app builds itself from the version.
            let page = updates.available().map_or_else(
                || format!("https://github.com/{}/releases/latest", updates::REPOSITORY),
                |v| v.release_page(),
            );
            if let Err(e) = app.opener().open_url(page, None::<&str>) {
                eprintln!("questodian: couldn't open the release page: {e}");
            }
        }
        ("POST", "/install") => install(app, updates.clone()),
        _ => return status(StatusCode::NOT_FOUND),
    }
    let state = app.state::<Arc<Updates>>().state();
    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .body(serde_json::to_vec(&state).expect("the update state serializes"))
        .expect("a valid response")
}

/// Download, check and install the new version, then restart as it. The UI follows the
/// progress through the state.
#[cfg(feature = "updater")]
fn install(app: &AppHandle, updates: Arc<Updates>) {
    use tauri_plugin_updater::UpdaterExt;
    if updates.available().is_none() || updates.state().installing.is_some() {
        return;
    }
    updates.set_installing(Some(0.0), None);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result: Result<(), String> = async {
            let update = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?;
            let Some(update) = update else { return Err("GitHub has no update for this computer yet.".into()) };
            let mut done = 0usize;
            update
                .download_and_install(
                    |chunk, total| {
                        done += chunk;
                        let total = total.unwrap_or(0) as f64;
                        updates.set_installing(Some(if total > 0.0 { done as f64 / total } else { 0.0 }), None);
                    },
                    || {},
                )
                .await
                .map_err(|e| e.to_string())
        }
        .await;
        match result {
            Ok(()) => app.restart(),
            Err(e) => {
                eprintln!("questodian: updating failed: {e}");
                let message = "The update couldn't be downloaded or didn't check out, so nothing was changed. Try again, or download it from GitHub.";
                updates.set_installing(None, Some(message.into()));
            }
        }
    });
}

/// Without the updater plugin, "Update" opens the release page instead (see `updates_route`).
#[cfg(not(feature = "updater"))]
fn install(app: &AppHandle, updates: Arc<Updates>) {
    let page = updates.available().map(|v| v.release_page());
    if let Some(page) = page
        && let Err(e) = app.opener().open_url(page, None::<&str>)
    {
        eprintln!("questodian: couldn't open the release page: {e}");
    }
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    Response::builder().status(code).body(Vec::new()).expect("a valid response")
}

/// Open the players' window at `path`, or point the open one there and bring it forward.
fn open_players(app: &AppHandle, path: &str) -> tauri::Result<()> {
    let url = base_url().join(path.trim_start_matches('/')).unwrap_or_else(|_| base_url());
    if let Some(window) = app.get_webview_window("players") {
        window.navigate(url)?;
        window.show()?;
        return window.set_focus();
    }
    WebviewWindowBuilder::new(app, "players", WebviewUrl::External(url))
        .title("Questodian: players")
        .inner_size(1280.0, 800.0)
        .build()?;
    Ok(())
}
