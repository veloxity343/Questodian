//! The HTTP API end to end: rules data edits and backups, rests, handout images (replacing and
//! removing them), map tokens, and import rules (copying, newer files).

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

    // The UI sends back the whole entry it was given, changed.
    let mut fighter = entry(&rules, "classes", "Fighter").unwrap().clone();
    fighter["hit_die"] = json!(12);
    fighter["name"] = json!("ignored");
    let (status, rules) = call(&router, Method::PUT, "/api/rules/classes/Fighter", Some(fighter)).await;
    assert_eq!(status, StatusCode::OK);
    let saved = entry(&rules, "classes", "Fighter").unwrap();
    assert_eq!(
        (&saved["name"], &saved["hit_die"], &saved["source"]),
        (&json!("Fighter"), &json!(12), &json!("edited"))
    );
    let (status, refused) =
        call(&router, Method::PUT, "/api/rules/classes/Fighter", Some(json!({"hit_die": 12}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "an entry the builder can't read: {refused}");

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
    let (status, _) = call(&router, Method::PUT, "/api/rules/settings/%20score_cap%20", Some(json!(21))).await;
    assert_eq!(status, StatusCode::OK, "the key is trimmed");
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

#[tokio::test]
async fn removing_a_handouts_image_deletes_the_file() {
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

    let (status, after) = call(&router, Method::DELETE, &format!("/api/handouts/{id}/file"), None).await;
    assert_eq!(status, StatusCode::OK, "{after}");
    assert_eq!(after["filename"], "");
    assert_eq!(after["mime"], "");
    assert_eq!(images(), 0, "the image file is gone");
    let (status, _) = call(&router, Method::GET, &format!("/api/handouts/{id}/file"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // Again, with no image: nothing to remove, still fine.
    let (status, _) = call(&router, Method::DELETE, &format!("/api/handouts/{id}/file"), None).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn importing_with_skip_or_copy_keeps_your_rule_edits() {
    let dir = tempfile::tempdir().unwrap();
    let router = app(dir.path()).await;
    call(&router, Method::PUT, "/api/rules/settings/score_cap", Some(json!(22))).await;
    let (_, mut bundle) = call(&router, Method::GET, "/api/export", None).await;
    bundle["collections"]["rule_overrides"][0]["data"] = json!(24);
    for mode in ["skip", "copy"] {
        let (status, report) =
            call(&router, Method::POST, &format!("/api/import?mode={mode}"), Some(bundle.clone())).await;
        assert_eq!(status, StatusCode::OK, "{report}");
        assert_eq!(report["skipped"]["rule_overrides"], 1, "{mode}");
        let (_, rules) = call(&router, Method::GET, "/api/rules", None).await;
        assert_eq!(rules["score_cap"], 22, "{mode}");
    }
    let (_, report) = call(&router, Method::POST, "/api/import?mode=overwrite", Some(bundle)).await;
    assert_eq!(report["imported"]["rule_overrides"], 1);
    let (_, rules) = call(&router, Method::GET, "/api/rules", None).await;
    assert_eq!(rules["score_cap"], 24);
}

#[tokio::test]
async fn builder_characters_rest_by_the_2024_rules_in_a_2014_campaign() {
    let dir = tempfile::tempdir().unwrap();
    let router = app(dir.path()).await;
    let (_, campaign) = call(&router, Method::POST, "/api/campaigns", Some(json!({"edition": "2014"}))).await;
    let body = |build: Value| {
        json!({"campaign_id": campaign["id"], "classes": [{"name": "Fighter", "level": 4}], "build": build,
               "hp": {"max": 30, "current": 30, "temp": 0}, "hit_dice": {"total": "4d10", "remaining": 0}})
    };
    // Built with a class from the rules data: 2024 (all Hit Dice back). By hand, without a
    // build, or with a class the rules data no longer has: the campaign's 2014 rules (half).
    let cases = [
        (json!({"class": "Fighter"}), 4),
        (json!({"class": "Fighter", "manual": true}), 2),
        (json!({}), 2),
        (json!({"class": "Gunslinger"}), 2),
    ];
    for (build, regained) in cases {
        let (_, ch) = call(&router, Method::POST, "/api/characters", Some(body(build.clone()))).await;
        let id = ch["id"].as_str().unwrap();
        let (_, ch) =
            call(&router, Method::POST, &format!("/api/characters/{id}/rest"), Some(json!({"type": "long"}))).await;
        assert_eq!(ch["hit_dice"]["remaining"], regained, "{build}");
    }
}

#[tokio::test]
async fn players_move_existing_tokens_only_and_only_when_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let router = app(dir.path()).await;
    let map = json!({"grid": {"type": "square", "cols": 4, "rows": 3, "size": 40}, "cells": {"0,0": "water"},
                     "tokens": [{"id": "t1", "c": 0, "r": 0, "name": "Ann", "character_id": "ann"}], "players_move": false});
    let (status, handout) =
        call(&router, Method::POST, "/api/handouts", Some(json!({"title": "Mill", "map": map}))).await;
    assert_eq!(status, StatusCode::CREATED, "{handout}");
    let id = handout["id"].as_str().unwrap();
    let path = format!("/api/handouts/{id}/tokens");
    let step = |c: i64, r: i64| json!({"moves": [{"id": "t1", "c": c, "r": r}]});
    let (status, _) = call(&router, Method::POST, &path, Some(step(2, 1))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "not allowed yet");

    // The DM allows moves, renames the token and adds another; a player then moves the first.
    let mut dm = handout.clone();
    dm["map"]["players_move"] = json!(true);
    dm["map"]["tokens"] = json!([
        {"id": "t1", "c": 0, "r": 0, "name": "Ann (renamed)", "character_id": "ann"},
        {"id": "t2", "c": 3, "r": 2, "name": "Ogre"}
    ]);
    call(&router, Method::PUT, &format!("/api/handouts/{id}"), Some(dm)).await;
    let (status, saved) = call(&router, Method::POST, &path, Some(step(2, 1))).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    let tokens = &saved["map"]["tokens"];
    assert_eq!((&tokens[0]["c"], &tokens[0]["r"], &tokens[0]["name"]), (&json!(2), &json!(1), &json!("Ann (renamed)")));
    assert_eq!(tokens[1]["name"], "Ogre", "the DM's new token stays");
    assert_eq!(saved["map"]["cells"]["0,0"], "water", "the rest of the map is kept");

    for bad in [step(4, 0), step(0, -1), json!({"moves": [{"id": "t1", "c": "x", "r": 0}]}), json!({"tokens": []})] {
        let (status, _) = call(&router, Method::POST, &path, Some(bad.clone())).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }
    let (_, unknown) =
        call(&router, Method::POST, &path, Some(json!({"moves": [{"id": "nobody", "c": 1, "r": 1}]}))).await;
    assert_eq!(unknown["map"]["tokens"].as_array().unwrap().len(), 2, "an unknown token isn't added");
    let (_, ogre) = call(&router, Method::POST, &path, Some(json!({"moves": [{"id": "t2", "c": 0, "r": 0}]}))).await;
    assert_eq!(ogre["map"]["tokens"][1]["c"], 3, "players move only their characters' tokens");

    let (_, image) = call(&router, Method::POST, "/api/handouts", Some(json!({"title": "Portrait"}))).await;
    let image_path = format!("/api/handouts/{}/tokens", image["id"].as_str().unwrap());
    let (status, _) = call(&router, Method::POST, &image_path, Some(step(0, 0))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "an image handout has no tokens");
}

#[tokio::test]
async fn copying_a_campaign_keeps_map_tokens_on_their_characters() {
    let dir = tempfile::tempdir().unwrap();
    let router = app(dir.path()).await;
    let (_, ch) = call(&router, Method::POST, "/api/characters", Some(json!({"name": "Pip"}))).await;
    let map = json!({"grid": {"type": "square", "cols": 2, "rows": 2, "size": 40},
                     "tokens": [{"id": "t1", "c": 0, "r": 0, "name": "Pip", "character_id": ch["id"]}]});
    call(&router, Method::POST, "/api/handouts", Some(json!({"title": "Mill", "map": map}))).await;
    let (_, bundle) = call(&router, Method::GET, "/api/export", None).await;
    let (status, _) = call(&router, Method::POST, "/api/import?mode=copy", Some(bundle)).await;
    assert_eq!(status, StatusCode::OK);
    // Tokens that aren't objects, or have no character, copy without fuss.
    let odd = json!({"format": "questodian", "version": 3, "collections": {"handouts": [
        {"id": "h9", "title": "Odd", "map": {"grid": {"cols": 1, "rows": 1}, "tokens": [5, {"id": "t9", "c": 0, "r": 0}]}}
    ]}});
    let (status, report) = call(&router, Method::POST, "/api/import?mode=copy", Some(odd)).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    let (_, all) = call(&router, Method::GET, "/api/handouts", None).await;
    let (odd, mill): (Vec<Value>, Vec<Value>) =
        all["handouts"].as_array().unwrap().iter().cloned().partition(|h| h["title"] == "Odd");
    assert_eq!(odd[0]["map"]["tokens"], json!([5, {"id": "t9", "c": 0, "r": 0}]), "nothing added to the tokens");
    let handouts = json!({"handouts": mill});
    let (_, characters) = call(&router, Method::GET, "/api/characters", None).await;
    let ids: Vec<&Value> = characters["characters"].as_array().unwrap().iter().map(|c| &c["id"]).collect();
    let linked: Vec<&Value> =
        handouts["handouts"].as_array().unwrap().iter().map(|h| &h["map"]["tokens"][0]["character_id"]).collect();
    assert_eq!(linked.len(), 2);
    assert_ne!(linked[0], linked[1], "the copy's token points at the copied character");
    assert!(linked.iter().all(|l| ids.contains(l)));
}

#[tokio::test]
async fn files_from_a_newer_version_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let router = app(dir.path()).await;
    let (status, error) = call(
        &router,
        Method::POST,
        "/api/import",
        Some(json!({"format": "questodian", "version": 4, "collections": {}})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error.to_string().contains("newer version"), "{error}");
    let (_, bundle) = call(&router, Method::GET, "/api/export", None).await;
    assert_eq!(bundle["version"], 3);
}

async fn raw(router: &Router, method: Method, path: &str, body: Vec<u8>, content_type: &str) -> (StatusCode, Vec<u8>) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", content_type)
        .body(Body::from(body))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    (status, axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec())
}

/// One sync through the API, as the page does it, with `folder` standing in for the cloud.
async fn sync_through_api(router: &Router, folder: &mut std::collections::BTreeMap<String, Vec<u8>>) -> Value {
    let listing: Vec<&String> = folder.keys().collect();
    let (status, wanted) = call(router, Method::POST, "/api/sync/wanted", Some(json!({"listing": listing}))).await;
    assert_eq!(status, StatusCode::OK, "{wanted}");
    let files: Vec<Value> = wanted["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| serde_json::from_slice(&folder[n.as_str().unwrap()]).unwrap())
        .collect();
    let (_, needed) = call(router, Method::POST, "/api/sync/images-needed", Some(json!({"files": files}))).await;
    for name in needed["images"].as_array().unwrap() {
        let name = name.as_str().unwrap();
        let (status, _) = raw(
            router,
            Method::PUT,
            &format!("/api/sync/images/{name}"),
            folder[name].clone(),
            "application/octet-stream",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    let (status, applied) = call(router, Method::POST, "/api/sync/apply", Some(json!({"files": files}))).await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    loop {
        let listing: Vec<String> = folder.keys().cloned().collect();
        let (status, out) = call(router, Method::POST, "/api/sync/outgoing", Some(json!({"listing": listing}))).await;
        assert_eq!(status, StatusCode::OK, "{out}");
        if out.is_null() {
            break;
        }
        for image in out["images"].as_array().unwrap() {
            let (status, bytes) = raw(
                router,
                Method::GET,
                &format!("/api/handouts/{}/file", image["handout"].as_str().unwrap()),
                vec![],
                "",
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            folder.insert(image["name"].as_str().unwrap().to_string(), bytes);
        }
        folder.insert(out["name"].as_str().unwrap().to_string(), out["file"].to_string().into_bytes());
        let (status, _) = call(router, Method::POST, "/api/sync/sent", Some(json!({"name": out["name"]}))).await;
        assert_eq!(status, StatusCode::OK);
    }
    applied
}

#[tokio::test]
async fn sync_through_the_api_moves_records_and_images() {
    let (one, two) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (a, b) = (app(one.path()).await, app(two.path()).await);
    let mut folder = std::collections::BTreeMap::new();
    let (_, handout) = call(&a, Method::POST, "/api/handouts", Some(json!({"title": "Map"}))).await;
    let id = handout["id"].as_str().unwrap().to_string();
    let png = vec![137u8, 80, 78, 71, 9, 9, 9];
    raw(&a, Method::PUT, &format!("/api/handouts/{id}/file"), png.clone(), "image/png").await;

    sync_through_api(&a, &mut folder).await;
    let (_, status) = call(&a, Method::GET, "/api/sync", None).await;
    assert_eq!((status["started"].clone(), status["waiting"].clone()), (json!(true), json!(0)));
    let applied = sync_through_api(&b, &mut folder).await;
    assert_eq!(applied["changed"], 1, "{applied}");
    let (_, copy) = raw(&b, Method::GET, &format!("/api/handouts/{id}/file"), vec![], "").await;
    assert_eq!(copy, png);
    assert!(
        !two.path().join("handouts").join("sync-fetched").read_dir().unwrap().any(|_| true),
        "fetched images are cleared"
    );

    let (status, refused) =
        raw(&b, Method::PUT, "/api/sync/images/..%2Fescape.png", vec![1], "application/octet-stream").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{}", String::from_utf8_lossy(&refused));
}

#[tokio::test]
async fn replacing_with_a_full_backup_makes_the_data_exactly_that() {
    let dir = tempfile::tempdir().unwrap();
    let router = app(dir.path()).await;
    let (_, campaign) = call(&router, Method::POST, "/api/campaigns", Some(json!({"name": "Kept"}))).await;
    let cid = campaign["id"].as_str().unwrap().to_string();
    let (_, note) = call(&router, Method::POST, "/api/notes", Some(json!({"title": "Before"}))).await;
    let (_, backup) = call(&router, Method::GET, "/api/export", None).await;

    // After the backup: an edit, new records (one with an image) and a rules edit.
    call(
        &router,
        Method::PUT,
        &format!("/api/notes/{}", note["id"].as_str().unwrap()),
        Some(json!({"title": "After"})),
    )
    .await;
    call(&router, Method::POST, "/api/sessions", Some(json!({"campaign_id": cid}))).await;
    let (_, handout) = call(&router, Method::POST, "/api/handouts", Some(json!({"title": "New map"}))).await;
    raw(
        &router,
        Method::PUT,
        &format!("/api/handouts/{}/file", handout["id"].as_str().unwrap()),
        vec![137, 80, 78, 71, 1],
        "image/png",
    )
    .await;
    call(&router, Method::PUT, "/api/rules/settings/score_cap", Some(json!(22))).await;

    let (_, scoped) = call(&router, Method::GET, &format!("/api/export?campaign_id={cid}"), None).await;
    let (status, refused) = call(&router, Method::POST, "/api/import?mode=replace", Some(scoped)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "a campaign's export can't stand for everything: {refused}");

    let (status, report) = call(&router, Method::POST, "/api/import?mode=replace", Some(backup.clone())).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["mode"], "replace");
    let (_, now) = call(&router, Method::GET, "/api/export", None).await;
    assert_eq!(now["collections"], backup["collections"]);
    assert_eq!(
        std::fs::read_dir(dir.path().join("handouts")).unwrap().count(),
        0,
        "the new handout's image went with it"
    );
}

#[tokio::test]
async fn a_cloud_backups_images_come_by_name_so_large_ones_restore() {
    use sha2::Digest;
    let dir = tempfile::tempdir().unwrap();
    let router = app(dir.path()).await;
    let (_, big) = call(&router, Method::POST, "/api/handouts", Some(json!({"title": "Big map"}))).await;
    let (_, gone) = call(&router, Method::POST, "/api/handouts", Some(json!({"title": "Its file went"}))).await;
    let mut png = vec![137u8, 80, 78, 71];
    png.resize(15 * 1024 * 1024 - 10, 7); // just under the image limit; with base64, over the request limit
    for (handout, bytes) in [(&big, png.clone()), (&gone, vec![137, 80, 78, 71, 2])] {
        let (status, _) = raw(
            &router,
            Method::PUT,
            &format!("/api/handouts/{}/file", handout["id"].as_str().unwrap()),
            bytes,
            "image/png",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    // As frontend/src/sync/backups.ts makes it: images by name, and one whose file couldn't be read.
    let (_, mut backup) = call(&router, Method::GET, "/api/export", None).await;
    let hash: String = sha2::Sha256::digest(&png).iter().map(|b| format!("{b:02x}")).collect();
    let name = format!("image-{hash}.png");
    for handout in backup["collections"]["handouts"].as_array_mut().unwrap() {
        let handout = handout.as_object_mut().unwrap();
        handout.remove("file_b64");
        if handout["title"] == "Big map" {
            handout.insert("sync_image".into(), json!(name));
        } else {
            handout.insert("keep_image".into(), json!(true));
        }
    }
    call(&router, Method::DELETE, &format!("/api/handouts/{}", big["id"].as_str().unwrap()), None).await;

    let (status, _) =
        raw(&router, Method::PUT, &format!("/api/sync/images/{name}"), png.clone(), "application/octet-stream").await;
    assert_eq!(status, StatusCode::OK);
    let (status, report) = call(&router, Method::POST, "/api/import?mode=replace", Some(backup)).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    let (_, restored) =
        raw(&router, Method::GET, &format!("/api/handouts/{}/file", big["id"].as_str().unwrap()), vec![], "").await;
    assert_eq!(restored.len(), png.len());
    let (_, kept) =
        raw(&router, Method::GET, &format!("/api/handouts/{}/file", gone["id"].as_str().unwrap()), vec![], "").await;
    assert_eq!(kept, vec![137, 80, 78, 71, 2], "an image the backup couldn't hold is kept");
    assert!(!dir.path().join("handouts").join("sync-fetched").join(&name).exists(), "the fetched image is used up");
    assert_eq!(
        std::fs::read_dir(dir.path().join("handouts"))
            .unwrap()
            .filter(|e| e.as_ref().unwrap().path().is_file())
            .count(),
        2
    );
}
