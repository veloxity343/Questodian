//! Questodian as a desktop app.
//!
//! The window shows `questodian://localhost/` (`http://questodian.localhost/` on
//! Windows and Android). Every request to that address goes straight to the same
//! router `questodian serve` uses, inside this process: the UI from files compiled
//! into the binary, and the API under `/api`. Nothing listens on the network.
//!
//! Set `QUESTODIAN_LOG=1` to print every request. One address belongs to the desktop
//! shell itself: `POST /desktop/players` opens (or
//! brings back) the players' window, for showing a handout on a second screen.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::borrow::Cow;

use axum::body::Body;
use axum::http::{Method, Request, Response, StatusCode, header};
use questodian_core::db::{self, Database};
use questodian_core::server::{self, App, Ui};
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder};
use tower::ServiceExt;

const SCHEME: &str = "questodian";

/// The built UI, compiled in (`npm run build` in `frontend/` first).
#[derive(rust_embed::RustEmbed)]
#[folder = "../src/questodian/web"]
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
    tauri::Builder::default()
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
