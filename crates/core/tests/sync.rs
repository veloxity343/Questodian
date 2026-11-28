//! Sync between databases through a folder (sync.rs): a map of file names to bytes stands in
//! for the cloud, and `sync` below does what the page does with a real one.

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use questodian_core::db::Database;
use questodian_core::kinds::Kind;
use questodian_core::sync::{self, Applied};
use questodian_core::{files, transfer};
use serde_json::{Value, json};

/// A cloud folder: names to bytes. Nothing in it is ever changed, only added.
type Folder = BTreeMap<String, Vec<u8>>;

struct Device {
    _dir: tempfile::TempDir,
    db: Database,
}

async fn device() -> Device {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).await.unwrap();
    Device { _dir: dir, db }
}

/// One sync, as the page does it: fetch and apply what's new, then send what's waiting.
async fn sync(device: &Device, folder: &mut Folder) -> Applied {
    let listing: Vec<String> = folder.keys().cloned().collect();
    let mut repo = device.db.write().await.unwrap();
    let names = sync::wanted(&mut repo, &listing).await.unwrap();
    let files: Vec<Value> = names.iter().map(|n| serde_json::from_slice(&folder[n]).unwrap()).collect();
    let needed = sync::images_needed(&mut repo, &files).await.unwrap();
    let images: HashMap<String, Vec<u8>> = needed.iter().map(|n| (n.clone(), folder[n].clone())).collect();
    let applied = sync::apply(&mut repo, &files, &images).await.unwrap();
    repo.commit().await.unwrap();
    loop {
        let listing: Vec<String> = folder.keys().cloned().collect();
        let mut repo = device.db.write().await.unwrap();
        let Some(out) = sync::outgoing(&mut repo, &listing).await.unwrap() else { break };
        repo.commit().await.unwrap();
        for image in &out.images {
            if !folder.contains_key(&image.name) {
                let bytes = image_of(device, &image.handout).await.expect("the handout's image");
                folder.insert(image.name.clone(), bytes);
            }
        }
        assert!(!folder.contains_key(&out.name), "a change file is never written twice");
        folder.insert(out.name.clone(), serde_json::to_vec(&out.file).unwrap());
        let mut repo = device.db.write().await.unwrap();
        sync::sent(&mut repo, &out.name).await.unwrap();
        repo.commit().await.unwrap();
    }
    applied
}

async fn image_of(device: &Device, handout: &str) -> Option<Vec<u8>> {
    let doc = device.db.read().await.unwrap().get(Kind::Handouts, handout).await.ok()?;
    files::read(&transfer::handout_file(&device.db.files_dir(), doc["filename"].as_str()?)?)
}

async fn create(device: &Device, kind: Kind, data: Value) -> Value {
    let mut repo = device.db.write().await.unwrap();
    let doc = repo.create(kind, Some(data)).await.unwrap();
    repo.commit().await.unwrap();
    doc
}

async fn update(device: &Device, kind: Kind, id: &str, data: Value) {
    let mut repo = device.db.write().await.unwrap();
    repo.update(kind, id, data).await.unwrap();
    repo.commit().await.unwrap();
}

async fn delete(device: &Device, kind: Kind, id: &str) {
    let mut repo = device.db.write().await.unwrap();
    repo.delete(kind, id).await.unwrap();
    repo.commit().await.unwrap();
}

/// Give a handout an image, as an upload does.
async fn set_image(device: &Device, id: &str, bytes: &[u8]) {
    let name = format!("{id}-{}.png", bytes.len());
    files::write(&device.db.files_dir().join(&name), bytes).unwrap();
    update(device, Kind::Handouts, id, json!({"filename": name, "mime": "image/png"})).await;
}

async fn get(device: &Device, kind: Kind, id: &str) -> Option<Value> {
    device.db.read().await.unwrap().get(kind, id).await.ok()
}

async fn status(device: &Device) -> sync::Status {
    sync::status(&mut device.db.read().await.unwrap()).await.unwrap()
}

/// Every record, without handouts' local file names (each device names its own files).
async fn everything(device: &Device) -> Value {
    let mut repo = device.db.read().await.unwrap();
    let mut out = serde_json::Map::new();
    for kind in Kind::ALL {
        let mut docs = repo.list(kind, &[]).await.unwrap();
        docs.sort_by(|x, y| x["id"].as_str().cmp(&y["id"].as_str())); // records made in the same second list either way
        for doc in &mut docs {
            if let Some(filename) = doc.get_mut("filename") {
                *filename = json!(!filename.as_str().unwrap_or("").is_empty());
            }
        }
        out.insert(kind.plural().into(), Value::Array(docs));
    }
    let rules: Vec<Value> =
        repo.rule_overrides().await.unwrap().into_iter().map(|o| json!([o.kind, o.key, o.data, o.updated])).collect();
    out.insert("rule_overrides".into(), Value::Array(rules));
    Value::Object(out)
}

/// The triggers note changes to the millisecond: keep two edits apart.
async fn later() {
    tokio::time::sleep(Duration::from_millis(5)).await;
}

#[tokio::test]
async fn a_first_sync_sends_everything_and_another_device_gets_it_all() {
    let (a, b) = (device().await, device().await);
    let mut folder = Folder::new();
    let campaign = create(&a, Kind::Campaigns, json!({"name": "The Sunken Keep"})).await;
    let cid = campaign["id"].as_str().unwrap();
    create(&a, Kind::Sessions, json!({"campaign_id": cid, "title": "Arrival"})).await;
    create(&a, Kind::Characters, json!({"campaign_id": cid, "name": "Wren"})).await;
    let handout = create(&a, Kind::Handouts, json!({"campaign_id": cid, "title": "Map"})).await;
    set_image(&a, handout["id"].as_str().unwrap(), b"\x89PNG one").await;
    {
        let mut repo = a.db.write().await.unwrap();
        repo.save_rule_override("species", "elf", Some(&json!({"speed": 35})), None).await.unwrap();
        repo.commit().await.unwrap();
    }

    let sent = sync(&a, &mut folder).await;
    assert_eq!(sent.changed, 0);
    assert_eq!(status(&a).await.waiting, 0, "everything was sent");
    assert_eq!(folder.keys().filter(|n| n.starts_with("image-")).count(), 1);

    let got = sync(&b, &mut folder).await;
    assert_eq!(got.changed, 5, "{got:?}");
    assert!(got.skipped.is_empty(), "{:?}", got.skipped);
    assert_eq!(everything(&b).await, everything(&a).await);
    assert_eq!(image_of(&b, handout["id"].as_str().unwrap()).await.unwrap(), b"\x89PNG one");
    assert_eq!(status(&b).await.waiting, 0, "what sync wrote isn't sent back");

    // Nothing new: nothing more is written to the folder.
    let files = folder.len();
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;
    assert_eq!(folder.len(), files);
}

#[tokio::test]
async fn edits_and_deletions_travel_both_ways_and_campaigns_take_their_records_along() {
    let (a, b) = (device().await, device().await);
    let mut folder = Folder::new();
    let campaign = create(&a, Kind::Campaigns, json!({"name": "Keep"})).await;
    let cid = campaign["id"].as_str().unwrap().to_string();
    let session = create(&a, Kind::Sessions, json!({"campaign_id": cid, "title": "One"})).await;
    let sid = session["id"].as_str().unwrap().to_string();
    let entry = create(&a, Kind::Handbook, json!({"campaign_id": cid, "title": "Lore"})).await;
    let hid = entry["id"].as_str().unwrap().to_string();
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;

    update(&b, Kind::Sessions, &sid, json!({"title": "One, renamed on B"})).await;
    sync(&b, &mut folder).await;
    sync(&a, &mut folder).await;
    assert_eq!(get(&a, Kind::Sessions, &sid).await.unwrap()["title"], "One, renamed on B");

    delete(&a, Kind::Campaigns, &cid).await;
    sync(&a, &mut folder).await;
    let got = sync(&b, &mut folder).await;
    assert!(got.skipped.is_empty(), "{:?}", got.skipped);
    assert!(get(&b, Kind::Campaigns, &cid).await.is_none());
    assert!(get(&b, Kind::Sessions, &sid).await.is_none(), "the campaign's session went with it");
    assert_eq!(get(&b, Kind::Handbook, &hid).await.unwrap()["campaign_id"], "", "handbook entries are unlinked");
    assert_eq!(everything(&b).await, everything(&a).await);
}

#[tokio::test]
async fn when_both_change_a_record_the_later_change_wins_everywhere_and_the_other_is_kept() {
    let (a, b) = (device().await, device().await);
    let mut folder = Folder::new();
    let npc = create(&a, Kind::Npcs, json!({"name": "Mara"})).await;
    let id = npc["id"].as_str().unwrap().to_string();
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;

    // Both edit while apart; B's edit is the later one, but A syncs first.
    update(&a, Kind::Npcs, &id, json!({"name": "Mara (A)"})).await;
    later().await;
    update(&b, Kind::Npcs, &id, json!({"name": "Mara (B)"})).await;
    sync(&a, &mut folder).await;
    let at_b = sync(&b, &mut folder).await;
    assert_eq!(at_b.older, 1, "B keeps its later edit");
    let at_a = sync(&a, &mut folder).await;
    assert_eq!(at_a.changed, 1);
    assert_eq!(get(&a, Kind::Npcs, &id).await.unwrap()["name"], "Mara (B)");
    assert_eq!(get(&b, Kind::Npcs, &id).await.unwrap()["name"], "Mara (B)");
    assert_eq!(status(&a).await.replaced, 1, "A's own edit is kept to restore");
    assert_eq!(status(&b).await.replaced, 1, "so is A's edit on B");
    assert_eq!(status(&a).await.waiting + status(&b).await.waiting, 0);
}

#[tokio::test]
async fn an_edit_made_after_a_deletion_brings_the_record_back_and_a_later_deletion_wins() {
    let (a, b) = (device().await, device().await);
    let mut folder = Folder::new();
    let note = create(&a, Kind::Notes, json!({"title": "Clue"})).await;
    let id = note["id"].as_str().unwrap().to_string();
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;

    delete(&a, Kind::Notes, &id).await;
    later().await;
    update(&b, Kind::Notes, &id, json!({"title": "Clue, still needed"})).await;
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;
    sync(&a, &mut folder).await;
    assert_eq!(get(&a, Kind::Notes, &id).await.unwrap()["title"], "Clue, still needed");
    assert_eq!(get(&b, Kind::Notes, &id).await.unwrap()["title"], "Clue, still needed");

    update(&a, Kind::Notes, &id, json!({"title": "Clue, edited"})).await;
    later().await;
    delete(&b, Kind::Notes, &id).await;
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;
    sync(&a, &mut folder).await;
    assert!(get(&a, Kind::Notes, &id).await.is_none());
    assert!(get(&b, Kind::Notes, &id).await.is_none());

    // A change seen once isn't applied again, even if its file is fetched again.
    let c = device().await;
    sync(&c, &mut folder).await;
    assert!(get(&c, Kind::Notes, &id).await.is_none(), "a newer deletion keeps it deleted on a new device");
}

#[tokio::test]
async fn images_are_sent_once_and_follow_their_handout() {
    let (a, b) = (device().await, device().await);
    let mut folder = Folder::new();
    let handout = create(&a, Kind::Handouts, json!({"title": "Map"})).await;
    let id = handout["id"].as_str().unwrap().to_string();
    set_image(&a, &id, b"first image").await;
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;

    update(&a, Kind::Handouts, &id, json!({"title": "Map, renamed"})).await;
    sync(&a, &mut folder).await;
    let got = sync(&b, &mut folder).await;
    assert_eq!(got.changed, 1);
    assert_eq!(folder.keys().filter(|n| n.starts_with("image-")).count(), 1, "the same image isn't sent again");
    assert_eq!(image_of(&b, &id).await.unwrap(), b"first image");

    set_image(&b, &id, b"second image").await;
    sync(&b, &mut folder).await;
    sync(&a, &mut folder).await;
    assert_eq!(image_of(&a, &id).await.unwrap(), b"second image");
    assert_eq!(folder.keys().filter(|n| n.starts_with("image-")).count(), 2);

    // The replaced image's file is gone from A's folder of handout images.
    let stored = std::fs::read_dir(a.db.files_dir()).unwrap().count();
    assert_eq!(stored, 1);
}

#[tokio::test]
async fn a_change_made_while_sending_stays_waiting() {
    let a = device().await;
    let mut folder = Folder::new();
    let scene = create(&a, Kind::Scenes, json!({"title": "Gate"})).await;
    let id = scene["id"].as_str().unwrap().to_string();

    let listing: Vec<String> = folder.keys().cloned().collect();
    let mut repo = a.db.write().await.unwrap();
    let out = sync::outgoing(&mut repo, &listing).await.unwrap().unwrap();
    repo.commit().await.unwrap();
    later().await;
    update(&a, Kind::Scenes, &id, json!({"title": "Gate, edited while sending"})).await;
    folder.insert(out.name.clone(), serde_json::to_vec(&out.file).unwrap());
    let mut repo = a.db.write().await.unwrap();
    sync::sent(&mut repo, &out.name).await.unwrap();
    repo.commit().await.unwrap();
    assert_eq!(status(&a).await.waiting, 1);

    sync(&a, &mut folder).await;
    let b = device().await;
    sync(&b, &mut folder).await;
    assert_eq!(get(&b, Kind::Scenes, &id).await.unwrap()["title"], "Gate, edited while sending");
}

#[tokio::test]
async fn a_new_empty_folder_gets_everything_again() {
    let a = device().await;
    let mut first = Folder::new();
    create(&a, Kind::Monsters, json!({"name": "Ogre"})).await;
    sync(&a, &mut first).await;

    let mut second = Folder::new();
    sync(&a, &mut second).await;
    let b = device().await;
    sync(&b, &mut second).await;
    assert_eq!(everything(&b).await, everything(&a).await);
}

#[tokio::test]
async fn a_file_from_a_newer_version_is_refused_and_nothing_changes() {
    let a = device().await;
    let file = json!({"format": "questodian-sync", "version": sync::VERSION + 1, "device": "elsewhere", "seq": 1,
        "changes": [{"kind": "campaigns", "id": "c1", "stamp": 1, "doc": {"name": "From the future"}}]});
    let mut repo = a.db.write().await.unwrap();
    let error = sync::apply(&mut repo, &[file], &HashMap::new()).await.unwrap_err().to_string();
    assert!(error.contains("newer version"), "{error}");
    drop(repo);
    assert!(get(&a, Kind::Campaigns, "c1").await.is_none());
}

// ---------------------------------------------------------------------- harder cases

/// Run SQL on a device's database directly (to set a clock wrong, say).
fn sql(device: &Device, statement: &str) {
    rusqlite::Connection::open(&device.db.path).unwrap().execute_batch(statement).unwrap();
}

/// Apply the folder's new files on a device, with no images fetched.
async fn apply_without_images(device: &Device, folder: &Folder) -> Applied {
    let listing: Vec<String> = folder.keys().cloned().collect();
    let mut repo = device.db.write().await.unwrap();
    let names = sync::wanted(&mut repo, &listing).await.unwrap();
    let files: Vec<Value> = names.iter().map(|n| serde_json::from_slice(&folder[n]).unwrap()).collect();
    let applied = sync::apply(&mut repo, &files, &HashMap::new()).await.unwrap();
    repo.commit().await.unwrap();
    applied
}

#[tokio::test]
async fn an_edit_gets_through_after_the_clock_is_put_back() {
    let (a, b) = (device().await, device().await);
    let mut folder = Folder::new();
    let note = create(&a, Kind::Notes, json!({"title": "v1"})).await;
    let id = note["id"].as_str().unwrap().to_string();
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;

    update(&a, Kind::Notes, &id, json!({"title": "v2"})).await;
    sql(&a, "UPDATE sync_outbox SET changed = changed + 3600000"); // the clock an hour fast
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;
    update(&a, Kind::Notes, &id, json!({"title": "v3"})).await; // the clock corrected
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;
    assert_eq!(get(&b, Kind::Notes, &id).await.unwrap()["title"], "v3");
}

#[tokio::test]
async fn a_session_edited_after_its_campaign_was_deleted_elsewhere_is_kept_everywhere() {
    for a_first in [false, true] {
        let (a, b) = (device().await, device().await);
        let mut folder = Folder::new();
        let campaign = create(&a, Kind::Campaigns, json!({"name": "Keep"})).await;
        let cid = campaign["id"].as_str().unwrap().to_string();
        let session = create(&a, Kind::Sessions, json!({"campaign_id": cid, "title": "One"})).await;
        let sid = session["id"].as_str().unwrap().to_string();
        sync(&a, &mut folder).await;
        sync(&b, &mut folder).await;

        delete(&a, Kind::Campaigns, &cid).await;
        later().await;
        update(&b, Kind::Sessions, &sid, json!({"title": "One, edited on B"})).await;
        let order = if a_first { [&a, &b, &a, &b] } else { [&b, &a, &b, &a] };
        for device in order {
            let applied = sync(device, &mut folder).await;
            assert!(applied.skipped.is_empty(), "{:?}", applied.skipped);
        }
        for device in [&a, &b] {
            assert!(get(device, Kind::Campaigns, &cid).await.is_none());
            let kept = get(device, Kind::Sessions, &sid).await.expect("the later edit keeps the session");
            assert_eq!((kept["title"].as_str(), kept["campaign_id"].as_str()), (Some("One, edited on B"), Some("")));
        }
        assert_eq!(everything(&a).await, everything(&b).await, "A synced first: {a_first}");
    }
}

#[tokio::test]
async fn a_record_waits_for_its_campaign_when_that_comes_in_a_later_file() {
    let (a, b) = (device().await, device().await);
    let mut folder = Folder::new();
    let campaign = create(&a, Kind::Campaigns, json!({"name": "Big"})).await;
    let cid = campaign["id"].as_str().unwrap().to_string();
    let session = create(&a, Kind::Sessions, json!({"campaign_id": cid, "title": "One"})).await;
    for n in 0..499 {
        create(&a, Kind::Monsters, json!({"name": format!("Monster {n}")})).await;
    }
    later().await;
    update(&a, Kind::Campaigns, &cid, json!({"name": "Big, renamed"})).await; // sent last

    // A sends its first file only; B syncs in between.
    let listing: Vec<String> = folder.keys().cloned().collect();
    let mut repo = a.db.write().await.unwrap();
    let out = sync::outgoing(&mut repo, &listing).await.unwrap().unwrap();
    sync::sent(&mut repo, &out.name).await.unwrap();
    repo.commit().await.unwrap();
    folder.insert(out.name.clone(), serde_json::to_vec(&out.file).unwrap());
    let early = sync(&b, &mut folder).await;
    assert_eq!(early.skipped.len(), 1, "the session waits for its campaign: {:?}", early.skipped);

    sync(&a, &mut folder).await;
    let late = sync(&b, &mut folder).await;
    assert!(late.skipped.is_empty(), "{:?}", late.skipped);
    assert_eq!(get(&b, Kind::Sessions, session["id"].as_str().unwrap()).await.unwrap()["campaign_id"], cid.as_str());
    assert_eq!(everything(&a).await, everything(&b).await);
}

#[tokio::test]
async fn a_change_skipped_for_want_of_its_image_is_applied_next_time() {
    let (a, b) = (device().await, device().await);
    let mut folder = Folder::new();
    let handout = create(&a, Kind::Handouts, json!({"title": "Map"})).await;
    let id = handout["id"].as_str().unwrap().to_string();
    set_image(&a, &id, b"the image").await;
    sync(&a, &mut folder).await;

    let first = apply_without_images(&b, &folder).await;
    assert_eq!(first.skipped.len(), 1, "{first:?}");
    sync(&b, &mut folder).await;
    assert_eq!(image_of(&b, &id).await.unwrap(), b"the image");
}

#[tokio::test]
async fn a_handout_that_cant_be_saved_keeps_its_image() {
    let (a, b) = (device().await, device().await);
    let mut folder = Folder::new();
    let handout = create(&a, Kind::Handouts, json!({"title": "Map"})).await;
    let id = handout["id"].as_str().unwrap().to_string();
    set_image(&a, &id, b"old image").await;
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;

    let image = "image-0000000000000000000000000000000000000000000000000000000000000000.png".to_string();
    let file = json!({"format": "questodian-sync", "version": sync::VERSION, "device": "elsewhere", "seq": 1,
        "changes": [{"kind": "handouts", "id": id, "stamp": 9_999_999_999_999i64, "doc": {"title": "x", "map": 5}, "image": image}]});
    let mut repo = b.db.write().await.unwrap();
    let applied = sync::apply(&mut repo, &[file], &HashMap::from([(image, b"new image".to_vec())])).await.unwrap();
    repo.commit().await.unwrap();
    assert_eq!(applied.skipped.len(), 1, "{applied:?}");
    assert_eq!(image_of(&b, &id).await.unwrap(), b"old image");
    assert_eq!(std::fs::read_dir(b.db.files_dir()).unwrap().count(), 1, "no file left from the failed save");
}

#[tokio::test]
async fn a_device_missing_an_image_file_doesnt_take_it_from_the_others() {
    let (a, b) = (device().await, device().await);
    let mut folder = Folder::new();
    let handout = create(&a, Kind::Handouts, json!({"title": "Map"})).await;
    let id = handout["id"].as_str().unwrap().to_string();
    set_image(&a, &id, b"the image").await;
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;

    let filename = get(&a, Kind::Handouts, &id).await.unwrap()["filename"].as_str().unwrap().to_string();
    std::fs::remove_file(a.db.files_dir().join(filename)).unwrap();
    update(&a, Kind::Handouts, &id, json!({"title": "Map, renamed"})).await;
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;
    assert_eq!(get(&b, Kind::Handouts, &id).await.unwrap()["title"], "Map, renamed");
    assert_eq!(image_of(&b, &id).await.unwrap(), b"the image");
}

#[tokio::test]
async fn applying_between_sending_and_sent_keeps_the_later_version_ahead() {
    let (a, b) = (device().await, device().await);
    let mut folder = Folder::new();
    let npc = create(&a, Kind::Npcs, json!({"name": "Mara"})).await;
    let id = npc["id"].as_str().unwrap().to_string();
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;

    update(&b, Kind::Npcs, &id, json!({"name": "B"})).await;
    sql(&b, "UPDATE sync_outbox SET changed = changed + 3600000"); // B's clock an hour fast
    update(&a, Kind::Npcs, &id, json!({"name": "A"})).await;
    sync(&b, &mut folder).await;

    // A starts sending, applies B's file (B's later edit wins), then finishes sending.
    let listing: Vec<String> = folder.keys().cloned().collect();
    let mut repo = a.db.write().await.unwrap();
    let out = sync::outgoing(&mut repo, &listing).await.unwrap().unwrap();
    repo.commit().await.unwrap();
    folder.insert(out.name.clone(), serde_json::to_vec(&out.file).unwrap());
    apply_without_images(&a, &folder).await;
    let mut repo = a.db.write().await.unwrap();
    sync::sent(&mut repo, &out.name).await.unwrap();
    repo.commit().await.unwrap();
    assert_eq!(get(&a, Kind::Npcs, &id).await.unwrap()["name"], "B");

    update(&a, Kind::Npcs, &id, json!({"name": "A2, made after seeing B"})).await;
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;
    assert_eq!(get(&b, Kind::Npcs, &id).await.unwrap()["name"], "A2, made after seeing B");
}

#[tokio::test]
async fn deleting_a_session_elsewhere_leaves_its_scenes_encounters_linked() {
    let (a, b) = (device().await, device().await);
    let mut folder = Folder::new();
    let cid = create(&a, Kind::Campaigns, json!({"name": "C"})).await["id"].as_str().unwrap().to_string();
    let sid = create(&a, Kind::Sessions, json!({"campaign_id": cid})).await["id"].as_str().unwrap().to_string();
    let scene = create(&a, Kind::Scenes, json!({"campaign_id": cid, "session_id": sid, "title": "X"})).await;
    let xid = scene["id"].as_str().unwrap().to_string();
    let eid = create(&a, Kind::Encounters, json!({"campaign_id": cid, "scene_id": xid})).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;

    update(&a, Kind::Encounters, &eid, json!({"name": "Edited on A"})).await;
    later().await;
    delete(&b, Kind::Sessions, &sid).await;
    for device in [&a, &b, &a, &b] {
        sync(device, &mut folder).await;
    }
    for device in [&a, &b] {
        let encounter = get(device, Kind::Encounters, &eid).await.unwrap();
        assert_eq!(encounter["scene_id"], xid.as_str(), "the scene is still there");
        assert_eq!(get(device, Kind::Scenes, &xid).await.unwrap()["session_id"], "");
    }
    assert_eq!(everything(&a).await, everything(&b).await);
}

#[tokio::test]
async fn a_new_folder_gets_records_another_device_made_too() {
    let (elsewhere, a, b) = (device().await, device().await, device().await);
    let mut first = Folder::new();
    let cid = create(&elsewhere, Kind::Campaigns, json!({"name": "Made elsewhere"})).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    sync(&elsewhere, &mut first).await;
    sync(&a, &mut first).await;
    create(&a, Kind::Sessions, json!({"campaign_id": cid, "title": "Made on A"})).await;

    let mut second = Folder::new();
    sync(&a, &mut second).await;
    let got = sync(&b, &mut second).await;
    assert!(got.skipped.is_empty(), "{:?}", got.skipped);
    assert_eq!(everything(&b).await, everything(&a).await);

    // In the first folder, A doesn't send the campaign again: its maker did.
    let before = first.len();
    let c = device().await;
    sync(&c, &mut first).await;
    assert_eq!(first.len(), before, "C has nothing of its own to send");
}

#[tokio::test]
async fn an_edit_made_before_its_campaign_was_deleted_goes_with_it_and_is_kept_to_restore() {
    let (a, b) = (device().await, device().await);
    let mut folder = Folder::new();
    let cid = create(&a, Kind::Campaigns, json!({"name": "C"})).await["id"].as_str().unwrap().to_string();
    let sid = create(&a, Kind::Sessions, json!({"campaign_id": cid, "title": "One"})).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;

    update(&a, Kind::Sessions, &sid, json!({"title": "One, edited first"})).await;
    later().await;
    delete(&b, Kind::Campaigns, &cid).await;
    for device in [&b, &a, &b, &a] {
        sync(device, &mut folder).await;
    }
    assert!(get(&a, Kind::Sessions, &sid).await.is_none());
    assert!(get(&b, Kind::Sessions, &sid).await.is_none());
    // The campaign, deleted on B, and A's edit to its session, are kept to restore.
    assert_eq!(status(&a).await.replaced, 2);
}

#[tokio::test]
async fn a_handout_moved_out_of_a_campaign_then_deleted_with_it_arrives_in_one_sync() {
    let (a, b) = (device().await, device().await);
    let mut folder = Folder::new();
    let cid = create(&a, Kind::Campaigns, json!({"name": "C"})).await["id"].as_str().unwrap().to_string();
    let hid = create(&a, Kind::Handouts, json!({"campaign_id": cid, "title": "Map"})).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    set_image(&a, &hid, b"map image").await;
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;

    update(&a, Kind::Handouts, &hid, json!({"campaign_id": ""})).await;
    later().await;
    delete(&a, Kind::Campaigns, &cid).await;
    sync(&a, &mut folder).await;
    let got = sync(&b, &mut folder).await;
    assert!(got.skipped.is_empty(), "{:?}", got.skipped);
    assert_eq!(get(&b, Kind::Handouts, &hid).await.unwrap()["campaign_id"], "");
    assert_eq!(image_of(&b, &hid).await.unwrap(), b"map image");
}

#[tokio::test]
async fn a_database_restored_from_a_copy_doesnt_reuse_its_file_names() {
    let a = device().await;
    let mut folder = Folder::new();
    create(&a, Kind::Notes, json!({"title": "one"})).await;
    sync(&a, &mut folder).await;
    sql(&a, r#"DELETE FROM sync_state WHERE "key" = 'seq'"#); // as it was before that sync
    create(&a, Kind::Notes, json!({"title": "two"})).await;
    sync(&a, &mut folder).await; // the helper fails if a name is written twice
    let b = device().await;
    sync(&b, &mut folder).await;
    assert_eq!(everything(&b).await, everything(&a).await);
}

#[tokio::test]
async fn restoring_a_backup_on_one_device_restores_it_on_the_others() {
    let (a, b) = (device().await, device().await);
    let mut folder = Folder::new();
    create(&a, Kind::Notes, json!({"title": "In the backup"})).await;
    let backup = transfer::export_bundle(&mut a.db.read().await.unwrap(), None).await.unwrap();
    create(&a, Kind::Notes, json!({"title": "Made after it"})).await;
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;

    let mut repo = a.db.write().await.unwrap();
    transfer::import_data(&mut repo, &backup, transfer::Mode::Replace).await.unwrap();
    repo.commit().await.unwrap();
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;
    let titles = |v: Value| v["notes"].as_array().unwrap().iter().map(|n| n["title"].clone()).collect::<Vec<_>>();
    assert_eq!(titles(everything(&b).await), vec![json!("In the backup")]);
}

#[tokio::test]
async fn a_restore_keeps_a_rules_edit_this_version_cant_use() {
    let a = device().await;
    // As a newer version's edit arrives by sync: not checked against this version's rules data.
    sql(
        &a,
        r#"INSERT INTO rule_overrides (kind, "key", data, updated) VALUES ('feats', 'From A Later Version', NULL, '2026-11-28T00:00:00+00:00')"#,
    );
    let backup = transfer::export_bundle(&mut a.db.read().await.unwrap(), None).await.unwrap();
    let mut repo = a.db.write().await.unwrap();
    transfer::import_data(&mut repo, &backup, transfer::Mode::Replace).await.unwrap();
    repo.commit().await.unwrap();
    let edits = a.db.read().await.unwrap().rule_overrides().await.unwrap();
    assert_eq!(edits.len(), 1, "{edits:?}");
}

// ---------------------------------------------------------------------- versions kept to restore

async fn kept(device: &Device) -> Vec<Value> {
    sync::replaced(&mut device.db.read().await.unwrap()).await.unwrap()
}

async fn restore_kept(device: &Device, n: &Value) -> Value {
    let mut repo = device.db.write().await.unwrap();
    let out = sync::restore_replaced(&mut repo, n.as_i64().unwrap()).await.unwrap();
    repo.commit().await.unwrap();
    out
}

#[tokio::test]
async fn an_edit_that_lost_can_be_restored_and_goes_everywhere() {
    let (a, b) = (device().await, device().await);
    let mut folder = Folder::new();
    let id = create(&a, Kind::Npcs, json!({"name": "Mara"})).await["id"].as_str().unwrap().to_string();
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;
    update(&a, Kind::Npcs, &id, json!({"name": "Mara (A)"})).await;
    later().await;
    update(&b, Kind::Npcs, &id, json!({"name": "Mara (B)"})).await;
    for device in [&a, &b, &a] {
        sync(device, &mut folder).await;
    }

    let list = kept(&a).await;
    assert_eq!((list.len(), &list[0]["title"], &list[0]["exists"]), (1, &json!("Mara (A)"), &json!(true)), "{list:?}");
    restore_kept(&a, &list[0]["n"]).await;
    assert_eq!(get(&a, Kind::Npcs, &id).await.unwrap()["name"], "Mara (A)");
    assert_eq!(kept(&a).await[0]["title"], "Mara (B)", "what the restore replaced is kept in turn");
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;
    assert_eq!(get(&b, Kind::Npcs, &id).await.unwrap()["name"], "Mara (A)");
}

#[tokio::test]
async fn a_record_deleted_on_another_device_can_be_brought_back() {
    let (a, b) = (device().await, device().await);
    let mut folder = Folder::new();
    let id = create(&a, Kind::Notes, json!({"title": "Clue"})).await["id"].as_str().unwrap().to_string();
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;
    delete(&a, Kind::Notes, &id).await;
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;

    let list = kept(&b).await;
    assert_eq!((&list[0]["title"], &list[0]["exists"]), (&json!("Clue"), &json!(false)));
    restore_kept(&b, &list[0]["n"]).await;
    sync(&b, &mut folder).await;
    sync(&a, &mut folder).await;
    assert_eq!(get(&a, Kind::Notes, &id).await.unwrap()["title"], "Clue");

    // Dismissed, it's gone from the list.
    let mut repo = b.db.write().await.unwrap();
    let n = sync::replaced(&mut repo).await.unwrap().len();
    if n > 0 {
        let first = sync::replaced(&mut repo).await.unwrap()[0]["n"].as_i64().unwrap();
        sync::forget_replaced(&mut repo, first).await.unwrap();
    }
    repo.commit().await.unwrap();
    assert_eq!(kept(&b).await.len(), n.saturating_sub(1));
}

#[tokio::test]
async fn a_handouts_old_image_comes_back_from_the_folder() {
    let (a, b) = (device().await, device().await);
    let mut folder = Folder::new();
    let id = create(&a, Kind::Handouts, json!({"title": "Map"})).await["id"].as_str().unwrap().to_string();
    set_image(&a, &id, b"first map").await;
    sync(&a, &mut folder).await;
    sync(&b, &mut folder).await;
    set_image(&b, &id, b"second map").await;
    sync(&b, &mut folder).await;
    sync(&a, &mut folder).await;
    assert_eq!(image_of(&a, &id).await.unwrap(), b"second map");

    let list = kept(&a).await;
    let image = list[0]["image"].as_str().expect("the kept version names its image").to_string();
    // The page fetches its image from the folder first.
    files::write(&sync::fetched_image(&a.db.files_dir(), &image).unwrap(), &folder[&image]).unwrap();
    let out = restore_kept(&a, &list[0]["n"]).await;
    assert_eq!(out["image"], true);
    assert_eq!(image_of(&a, &id).await.unwrap(), b"first map");

    // The version that replaced, kept in turn: without its image fetched, it comes back without one, and says so.
    let second = kept(&a).await;
    assert_eq!(second.len(), 1, "{second:?}");
    let out = restore_kept(&a, &second[0]["n"]).await;
    assert_eq!(out["image"], false);
    assert_eq!(get(&a, Kind::Handouts, &id).await.unwrap()["filename"], "");
}
