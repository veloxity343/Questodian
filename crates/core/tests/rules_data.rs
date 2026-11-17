//! Editing the character builder's rules data through the API, and carrying the edits
//! through a full export and import.

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use questodian_core::db::Database;
use questodian_core::server::{self, App, Ui};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn call(router: &Router, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let body = body.map_or_else(Body::empty, |b| Body::from(b.to_string()));
    let request = Request::builder().method(method).uri(path).body(body).unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn entry<'a>(rules: &'a Value, kind: &str, name: &str) -> Option<&'a Value> {
    rules[kind].as_array().unwrap().iter().find(|e| e["name"] == name)
}

async fn app(dir: &std::path::Path) -> Router {
    server::router(App::new(Database::open(dir).await.unwrap(), Ui::Missing))
}

#[tokio::test]
async fn edit_hide_reset_and_carry_through_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let router = app(dir.path()).await;

    let (status, rules) = call(&router, Method::GET, "/api/rules", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(entry(&rules, "classes", "Fighter").unwrap()["source"], "bundled");
    assert_eq!(rules["point_buy"]["budget"], 27);

    let fighter = json!({"name": "ignored", "hit_die": 12});
    let (status, rules) = call(&router, Method::PUT, "/api/rules/classes/Fighter", Some(fighter)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        entry(&rules, "classes", "Fighter").unwrap(),
        &json!({"name": "Fighter", "hit_die": 12, "source": "edited"})
    );

    let (_, rules) = call(&router, Method::PUT, "/api/rules/feats/Lucky", Some(Value::Null)).await;
    assert!(entry(&rules, "feats", "Lucky").is_none());
    assert_eq!(rules["hidden"], json!({"feats": ["Lucky"]}));

    let pack = json!({"weight": 30.0, "cost": "5 GP", "kind": "Gear", "note": ""});
    call(&router, Method::PUT, "/api/rules/items/Monster%20Hunter's%20Pack", Some(pack)).await;
    call(&router, Method::PUT, "/api/rules/settings/score_cap", Some(json!(22))).await;

    let (status, refused) = call(&router, Method::PUT, "/api/rules/settings/score_cap", Some(json!("high"))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");

    // A full export carries the edits; importing it into a new data folder restores them.
    let (_, bundle) = call(&router, Method::GET, "/api/export", None).await;
    assert_eq!(bundle["collections"]["rule_overrides"].as_array().unwrap().len(), 4);
    let other = tempfile::tempdir().unwrap();
    let restored = app(other.path()).await;
    let (status, report) = call(&restored, Method::POST, "/api/import?mode=overwrite", Some(bundle)).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["imported"]["rule_overrides"], 4);
    let (_, rules) = call(&restored, Method::GET, "/api/rules", None).await;
    assert_eq!(entry(&rules, "items", "Monster Hunter's Pack").unwrap()["source"], "added");
    assert_eq!(rules["score_cap"], 22);
    assert!(entry(&rules, "feats", "Lucky").is_none());

    // Resetting brings back the bundled entry.
    let (status, rules) = call(&router, Method::DELETE, "/api/rules/classes/Fighter", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(entry(&rules, "classes", "Fighter").unwrap()["hit_die"], 10);
    let (status, _) = call(&router, Method::DELETE, "/api/rules/classes/Fighter", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
