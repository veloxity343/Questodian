//! Proof, part 1: an axum Router, called as a plain function (no network), compiled to WebAssembly.

use axum::{Json, Router, routing::get};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use wasm_bindgen::prelude::*;

fn router() -> Router {
    Router::new().route("/api/hello", get(|| async { Json(json!({"hello": "from axum in WebAssembly"})) }))
}

/// Send one request through the router and return the response body as text.
#[wasm_bindgen]
pub async fn request(method: String, path: String) -> Result<String, JsValue> {
    let req = axum::http::Request::builder().method(method.as_str()).uri(path).body(axum::body::Body::empty())
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let res = router().oneshot(req).await.map_err(|e| JsValue::from_str(&e.to_string()))?;
    let bytes = res.into_body().collect().await.map_err(|e| JsValue::from_str(&e.to_string()))?.to_bytes();
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Ok(value.to_string())
}
