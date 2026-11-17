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
    // Edits this version can't use (from a later one, say) are skipped, not fatal.
    let mut bundle = bundle;
    let edits = bundle["collections"]["rule_overrides"].as_array_mut().unwrap();
    let score_cap = edits.iter().find(|e| e["key"] == "score_cap").unwrap().clone();
    edits.push(json!({"kind": "feats", "key": "Renamed Later", "data": null, "updated": "x"}));
    edits.push(json!({"kind": "spells", "key": "Fireball", "data": {}, "updated": "x"}));
    let (status, report) = call(&restored, Method::POST, "/api/import?mode=overwrite", Some(bundle)).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["imported"]["rule_overrides"], 4);
    assert_eq!(report["skipped"]["rule_overrides"], 2);
    let (_, again) = call(&restored, Method::GET, "/api/export", None).await;
    let kept = again["collections"]["rule_overrides"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["key"] == "score_cap")
        .unwrap()
        .clone();
    assert_eq!(kept, score_cap, "kept exactly, time included");
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

#[tokio::test]
async fn resources_come_back_on_the_right_rest() {
    let dir = tempfile::tempdir().unwrap();
    let router = app(dir.path()).await;
    let resources = json!([
        {"name": "Action Surge", "current": 0, "max": 1, "reset": "short"},
        {"name": "Second Wind", "current": 0, "max": 3, "reset": "short_one"},
        {"name": "Indomitable", "current": 0, "max": 1, "reset": "long"},
        {"name": "Lucky charm", "current": 0, "max": 1, "reset": "none"},
    ]);
    let body = json!({"name": "Fighter", "hp": {"max": 12, "current": 5, "temp": 0}, "resources": resources});
    let (status, ch) = call(&router, Method::POST, "/api/characters", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{ch}");
    let id = ch["id"].as_str().unwrap();
    let current = |ch: &Value| {
        ch["resources"].as_array().unwrap().iter().map(|r| r["current"].as_i64().unwrap()).collect::<Vec<_>>()
    };

    let (_, ch) =
        call(&router, Method::POST, &format!("/api/characters/{id}/rest"), Some(json!({"type": "short"}))).await;
    assert_eq!(current(&ch), [1, 1, 0, 0]);
    let (_, ch) =
        call(&router, Method::POST, &format!("/api/characters/{id}/rest"), Some(json!({"type": "short"}))).await;
    assert_eq!(current(&ch), [1, 2, 0, 0]);
    let (_, ch) =
        call(&router, Method::POST, &format!("/api/characters/{id}/rest"), Some(json!({"type": "long"}))).await;
    assert_eq!(current(&ch), [1, 3, 1, 0]);
}

#[tokio::test]
async fn reimporting_a_backup_doesnt_leave_old_images_behind() {
    let dir = tempfile::tempdir().unwrap();
    let router = app(dir.path()).await;
    let (_, handout) = call(&router, Method::POST, "/api/handouts", Some(json!({"title": "Map"}))).await;
    let id = handout["id"].as_str().unwrap().to_string();
    let request = Request::builder()
        .method(Method::PUT)
        .uri(format!("/api/handouts/{id}/file"))
        .header("content-type", "image/png")
        .body(Body::from(vec![137u8, 80, 78, 71, 1, 2, 3]))
        .unwrap();
    assert_eq!(router.clone().oneshot(request).await.unwrap().status(), StatusCode::OK);
    let images = || std::fs::read_dir(dir.path().join("handouts")).unwrap().count();
    assert_eq!(images(), 1);

    let (_, bundle) = call(&router, Method::GET, "/api/export", None).await;
    let (status, report) = call(&router, Method::POST, "/api/import?mode=overwrite", Some(bundle)).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(images(), 1, "the replaced image is removed");
    let (_, after) = call(&router, Method::GET, &format!("/api/handouts/{id}"), None).await;
    assert!(dir.path().join("handouts").join(after["filename"].as_str().unwrap()).is_file());
}
