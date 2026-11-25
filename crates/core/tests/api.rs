//! The HTTP API end to end: rules data edits and backups, rests, handout images (replacing and
//! removing them), map tokens, import rules (copying, newer files) and records that can't be read.

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

#[tokio::test]
async fn exports_say_which_records_they_leave_out() {
    let dir = tempfile::tempdir().unwrap();
    let router = app(dir.path()).await;
    let (_, campaign) = call(&router, Method::POST, "/api/campaigns", Some(json!({"name": "Vale"}))).await;
    let in_campaign = json!({"name": "Pip", "campaign_id": campaign["id"]});
    let (_, pip) = call(&router, Method::POST, "/api/characters", Some(in_campaign)).await;
    let (_, odo) = call(&router, Method::POST, "/api/characters", Some(json!({"name": "Odo"}))).await;
    call(&router, Method::POST, "/api/characters", Some(json!({"name": "Fine"}))).await;
    let (status, clean) = call(&router, Method::GET, "/api/unreadable", None).await;
    assert_eq!((status, &clean), (StatusCode::OK, &json!({"unreadable": []})));
    let (_, bundle) = call(&router, Method::GET, "/api/export", None).await;
    assert!(bundle.get("unreadable").is_none(), "a complete backup has no list: {bundle}");

    // Edited by hand into something no character can have.
    let path = dir.path().join(questodian_core::db::DB_FILENAME);
    let sql = format!(
        "UPDATE characters SET ac = 10.5 WHERE id IN ('{}', '{}');",
        pip["id"].as_str().unwrap(),
        odo["id"].as_str().unwrap()
    );
    questodian_core::db::load_sql(&path, &sql).await.unwrap();

    let (_, listed) = call(&router, Method::GET, "/api/characters", None).await;
    let names: Vec<&Value> = listed["characters"].as_array().unwrap().iter().map(|c| &c["name"]).collect();
    assert_eq!(names, [&json!("Fine")], "the others are still listed");
    let (_, found) = call(&router, Method::GET, "/api/unreadable", None).await;
    let ids = |list: &Value| {
        let mut ids: Vec<String> =
            list.as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap().to_string()).collect();
        ids.sort();
        ids
    };
    let mut both = vec![pip["id"].as_str().unwrap().to_string(), odo["id"].as_str().unwrap().to_string()];
    both.sort();
    assert_eq!(ids(&found["unreadable"]), both);
    let first = &found["unreadable"][0];
    assert_eq!(first["kind"], "characters");
    assert!(first["error"].as_str().unwrap().contains("10.5"), "says what's wrong: {first}");
    assert!(first.get("campaign_id").is_none());

    let (_, bundle) = call(&router, Method::GET, "/api/export", None).await;
    assert_eq!(ids(&bundle["unreadable"]), both);
    assert_eq!(bundle["collections"]["characters"].as_array().unwrap().len(), 1);
    // A campaign export lists only its own.
    let (_, vale) =
        call(&router, Method::GET, &format!("/api/export?campaign_id={}", campaign["id"].as_str().unwrap()), None)
            .await;
    assert_eq!(ids(&vale["unreadable"]), [pip["id"].as_str().unwrap()]);
    assert_eq!(vale["collections"]["characters"], json!([]));

    // Importing the file works, and says what it's missing.
    let (status, report) = call(&router, Method::POST, "/api/import?mode=copy", Some(bundle)).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["imported"]["characters"], 1, "{report}");
    assert_eq!(ids(&report["unreadable"]), both);
    let (_, plain) = call(&router, Method::POST, "/api/import?mode=copy", Some(json!({"collections": {}}))).await;
    assert_eq!(plain["unreadable"], json!([]));
}

#[tokio::test]
async fn campaign_exports_list_their_unreadable_records_of_every_kind() {
    let dir = tempfile::tempdir().unwrap();
    let router = app(dir.path()).await;
    let (_, vale) = call(&router, Method::POST, "/api/campaigns", Some(json!({"name": "Vale"}))).await;
    let (_, moor) = call(&router, Method::POST, "/api/campaigns", Some(json!({"name": "Moor"}))).await;
    let (vale, moor) = (vale["id"].as_str().unwrap(), moor["id"].as_str().unwrap());
    let session = |campaign: &str, number| json!({"title": "Night", "campaign_id": campaign, "number": number});
    let (_, ours) = call(&router, Method::POST, "/api/sessions", Some(session(vale, 1))).await;
    let (_, theirs) = call(&router, Method::POST, "/api/sessions", Some(session(moor, 1))).await;
    let (_, npc) = call(&router, Method::POST, "/api/npcs", Some(json!({"name": "Hob", "campaign_id": vale}))).await;
    let sql = format!(
        "UPDATE sessions SET number = 2.5; \
         UPDATE npcs SET campaign_id = CAST('{vale}' AS BLOB) WHERE id = '{}';",
        npc["id"].as_str().unwrap()
    );
    questodian_core::db::load_sql(&dir.path().join(questodian_core::db::DB_FILENAME), &sql).await.unwrap();

    let export = |campaign: &str| format!("/api/export?campaign_id={campaign}");
    let (_, bundle) = call(&router, Method::GET, &export(vale), None).await;
    let found: Vec<(&Value, &Value)> =
        bundle["unreadable"].as_array().unwrap().iter().map(|r| (&r["kind"], &r["id"])).collect();
    // Its own session, not the other campaign's.
    assert_eq!(found, [(&json!("sessions"), &ours["id"])], "{bundle}");
    // The NPC whose campaign link is stored as a blob reads as text, and is in its campaign's export.
    assert_eq!(bundle["collections"]["npcs"][0]["id"], npc["id"], "{bundle}");
    let (_, other) = call(&router, Method::GET, &export(moor), None).await;
    let found: Vec<&Value> = other["unreadable"].as_array().unwrap().iter().map(|r| &r["id"]).collect();
    assert_eq!(found, [&theirs["id"]], "{other}");
    assert_eq!(other["collections"]["npcs"], json!([]));
}
