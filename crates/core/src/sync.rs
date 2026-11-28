//! Sync through a cloud folder the user owns (Google Drive; WebDAV next), with no server of ours.
//!
//! No device ever changes a file in the folder once it's written, so no device can overwrite
//! another's work, whatever the cloud allows. Each device adds numbered files of its changes,
//! `changes-<device>-<seq>.json`, and images named by their content, `image-<sha256>.<ext>`
//! (sent once, whichever records use them). Syncing is applying the files other devices added
//! since last time, then sending this device's own changes.
//!
//! When two devices changed the same record, the later change wins: by the time it was made,
//! then by device, so every device picks the same one. A change made after applying another is
//! always later than it, whatever the clocks say. The version that lost is kept for 30 days
//! (`sync_replaced`), to restore.
//!
//! The core does no networking: the page moves the files, since it holds the sign-in to the
//! cloud. It asks which files to fetch ([`wanted`], [`images_needed`]), hands them over
//! ([`apply`]), asks what to send ([`outgoing`]), and says when it's sent ([`sent`]).
//! What changed locally is noted by triggers (migrations/0005_to_0006.sql).

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::error::{Result, invalid};
use crate::kinds::{Kind, LINKS};
use crate::repo::{Repo, new_id, now};
use crate::transfer::{handout_file, store_image};

/// What a change file says it is, and the newest format this app reads.
pub const FORMAT: &str = "questodian-sync";
pub const VERSION: u64 = 1;
/// The kind name for the rules-data edits, whose id is `[kind, key]` as JSON.
const RULES: &str = "rule_overrides";
/// A change file holds at most this many changes, and stops growing past this size (Google
/// Drive's simple uploads take up to 5 MB); what's left goes in the next.
const MAX_CHANGES: usize = 500;
const MAX_BYTES: usize = 4_000_000;
/// How long a version replaced by another device's is kept.
const KEEP_REPLACED: chrono::TimeDelta = chrono::TimeDelta::days(30);

/// When a version of a record was made, and on which device; later wins.
type Stamp = (i64, String);

// ------------------------------------------------------------------ state

fn state(repo: &Repo<'_>, key: &str) -> Result<Option<String>> {
    Ok(repo.conn().query_row(r#"SELECT value FROM sync_state WHERE "key" = ?"#, [key], |r| r.get(0)).optional()?)
}

fn set_state(repo: &Repo<'_>, key: &str, value: &str) -> Result<()> {
    repo.conn().execute(
        r#"INSERT INTO sync_state ("key", value) VALUES (?, ?) ON CONFLICT ("key") DO UPDATE SET value = excluded.value"#,
        [key, value],
    )?;
    Ok(())
}

fn clear_state(repo: &Repo<'_>, key: &str) -> Result<()> {
    repo.conn().execute(r#"DELETE FROM sync_state WHERE "key" = ?"#, [key])?;
    Ok(())
}

/// This device's name in the folder, made the first time it syncs. It lives in the database, so
/// a database copied to another device keeps it: restore backups by importing them instead.
fn device(repo: &Repo<'_>) -> Result<String> {
    if let Some(device) = state(repo, "device")? {
        return Ok(device);
    }
    let device = new_id();
    set_state(repo, "device", &device)?;
    Ok(device)
}

fn stamp_of(repo: &Repo<'_>, kind: &str, id: &str) -> Result<Option<Stamp>> {
    Ok(repo
        .conn()
        .query_row("SELECT stamp, device FROM sync_stamps WHERE kind = ? AND id = ?", [kind, id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?)
}

fn set_stamp(repo: &Repo<'_>, kind: &str, id: &str, stamp: &Stamp) -> Result<()> {
    repo.conn().execute(
        "INSERT INTO sync_stamps (kind, id, stamp, device) VALUES (?, ?, ?, ?)
         ON CONFLICT (kind, id) DO UPDATE SET stamp = excluded.stamp, device = excluded.device",
        params![kind, id, stamp.0, stamp.1],
    )?;
    Ok(())
}

/// When this device last changed the record, if that change hasn't been sent yet.
fn waiting_since(repo: &Repo<'_>, kind: &str, id: &str) -> Result<Option<i64>> {
    Ok(repo
        .conn()
        .query_row("SELECT changed FROM sync_outbox WHERE kind = ? AND id = ?", [kind, id], |r| r.get(0))
        .optional()?)
}

/// The time to send this device's change to a record with: when it was made, but always later
/// than the version it was made to, whatever the clocks say (a clock put back, two edits in one
/// millisecond). Sending the same change again (after a sync cut off) gives the same time.
fn stamp_to_send(repo: &Repo<'_>, kind: &str, id: &str, changed: i64) -> Result<i64> {
    Ok(match stamp_of(repo, kind, id)? {
        Some((held, _)) => changed.max(held + 1),
        None => changed,
    })
}

/// Note a record as changed here, to send: for a version this device made while applying others'
/// changes (a record kept and unlinked when what it was in was deleted). `at_least` makes it
/// later than the change that caused it.
fn queue(repo: &Repo<'_>, kind: &str, id: &str, at_least: i64) -> Result<()> {
    let now = chrono::Utc::now().timestamp_millis().max(at_least);
    repo.conn().execute(
        "INSERT INTO sync_outbox (kind, id, changed) VALUES (?, ?, ?)
         ON CONFLICT (kind, id) DO UPDATE SET changed = max(changed, excluded.changed)",
        params![kind, id, now],
    )?;
    Ok(())
}

/// Where this device's changes stand.
#[derive(Debug, Serialize)]
pub struct Status {
    /// Synced at least once from this database.
    pub started: bool,
    /// Changes not yet sent.
    pub waiting: i64,
    /// Versions replaced by another device's, kept to restore.
    pub replaced: i64,
}

pub async fn status(repo: &mut Repo<'_>) -> Result<Status> {
    let count = |sql: &str| repo.conn().query_row(sql, [], |r| r.get::<_, i64>(0));
    Ok(Status {
        started: state(repo, "device")?.is_some(),
        waiting: count("SELECT count(*) FROM sync_outbox")?,
        replaced: count("SELECT count(*) FROM sync_replaced")?,
    })
}

// ------------------------------------------------------------------ records

/// A record as sync sends it: the whole record, or for a rules-data edit its data and time.
/// None if it doesn't exist (deleted).
async fn load(repo: &mut Repo<'_>, kind: &str, id: &str) -> Result<Option<Value>> {
    if kind == RULES {
        let (rule_kind, key) = rule_key(id)?;
        let row: Option<(Option<String>, String)> = repo
            .conn()
            .query_row(
                r#"SELECT data, updated FROM rule_overrides WHERE kind = ? AND "key" = ?"#,
                [&rule_kind, &key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        return Ok(row.map(|(data, updated)| {
            let data = data.and_then(|d| serde_json::from_str::<Value>(&d).ok()).unwrap_or(Value::Null);
            json!({"data": data, "updated": updated})
        }));
    }
    let kind = record_kind(kind)?;
    repo.find(kind, id).await
}

fn record_kind(kind: &str) -> Result<Kind> {
    Kind::from_plural(kind).ok_or_else(|| invalid(format!("unknown kind '{kind}'")))
}

fn rule_key(id: &str) -> Result<(String, String)> {
    serde_json::from_str(id).map_err(|_| invalid(format!("invalid rules-data edit '{id}'")))
}

/// The name an image has in the folder: its content's SHA-256, and its extension.
fn image_name(filename: &str, bytes: &[u8]) -> String {
    let extension = Path::new(filename)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .filter(|e| !e.is_empty() && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or_else(|| "png".into());
    let hash: String = Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
    format!("image-{hash}.{extension}")
}

/// A handout's image as named in the folder, if it has one here.
fn local_image(repo: &Repo<'_>, handout: &Value) -> Option<String> {
    let filename = handout.get("filename").and_then(Value::as_str).unwrap_or("");
    let bytes = crate::files::read(&handout_file(&repo.files_dir, filename)?)?;
    Some(image_name(filename, &bytes))
}

// ------------------------------------------------------------------ fetching

/// A change file's name: `changes-<device>-<seq>.json`.
fn change_file(device: &str, seq: u64) -> String {
    format!("changes-{device}-{seq:09}.json")
}

fn parse_change_file(name: &str) -> Option<(&str, u64)> {
    let rest = name.strip_prefix("changes-")?.strip_suffix(".json")?;
    let (device, seq) = rest.rsplit_once('-')?;
    Some((device, seq.parse().ok()?))
}

/// Which of the folder's files to fetch and [`apply`], in order: other devices' change files
/// this device hasn't applied yet (whatever order they appear in, and whatever numbers are
/// missing: a device's file that was never sent leaves a gap).
pub async fn wanted(repo: &mut Repo<'_>, listing: &[String]) -> Result<Vec<String>> {
    let me = state(repo, "device")?;
    let mut out = Vec::new();
    for name in listing {
        let Some((device, seq)) = parse_change_file(name) else { continue };
        if me.as_deref() == Some(device) || applied_already(repo, name)? {
            continue;
        }
        out.push((device.to_string(), seq, name.clone()));
    }
    out.sort();
    Ok(out.into_iter().map(|(_, _, name)| name).collect())
}

fn applied_already(repo: &Repo<'_>, name: &str) -> Result<bool> {
    Ok(repo.conn().query_row("SELECT 1 FROM sync_applied WHERE name = ?", [name], |_| Ok(())).optional()?.is_some())
}

/// The images these change files use that this device doesn't have: fetch them for [`apply`].
pub async fn images_needed(repo: &mut Repo<'_>, files: &[Value]) -> Result<Vec<String>> {
    let mut needed = Vec::new();
    for change in files.iter().flat_map(|f| f["changes"].as_array().into_iter().flatten()) {
        let Some(image) = change.get("image").and_then(Value::as_str) else { continue };
        if needed.iter().any(|n| n == image) {
            continue;
        }
        let id = change["id"].as_str().unwrap_or("");
        let have = match repo.find(Kind::Handouts, id).await? {
            Some(handout) => local_image(repo, &handout),
            None => None,
        };
        if have.as_deref() != Some(image) {
            needed.push(image.to_string());
        }
    }
    Ok(needed)
}

/// What applying some change files did.
#[derive(Debug, Default, Serialize)]
pub struct Applied {
    /// Records written or deleted here.
    pub changed: usize,
    /// Changes older than what this device has, or than its own change waiting to be sent.
    pub older: usize,
    /// Records that couldn't be written yet, and why (an image not fetched, a campaign not
    /// sent yet). Their files are applied again next time.
    pub skipped: Vec<String>,
}

/// Why a change wasn't written this time: it's tried again next sync.
struct NotYet(String);

impl From<crate::error::Error> for NotYet {
    fn from(e: crate::error::Error) -> Self {
        NotYet(e.to_string())
    }
}

impl From<rusqlite::Error> for NotYet {
    fn from(e: rusqlite::Error) -> Self {
        NotYet(crate::error::Error::from(e).to_string())
    }
}

/// One change from a file.
struct Change {
    kind: String,
    id: String,
    stamp: Stamp,
    doc: Option<Value>,
    image: Option<String>,
    /// The sender has the handout's image but couldn't read it: keep the one here.
    keep_image: bool,
    /// The file it came from.
    file: String,
}

/// Apply change files from other devices, all or nothing, with the images [`images_needed`]
/// asked for. Each record ends up as its latest version; a version replaced is kept to restore.
pub async fn apply(repo: &mut Repo<'_>, files: &[Value], images: &HashMap<String, Vec<u8>>) -> Result<Applied> {
    let me = device(repo)?;
    let mut latest: HashMap<(String, String), Change> = HashMap::new();
    let mut names = Vec::new();
    for file in files {
        if file["format"] != FORMAT {
            return Err(invalid("not a Questodian sync file"));
        }
        if file["version"].as_u64().is_none_or(|v| v > VERSION) {
            return Err(invalid("another device syncs with a newer version of Questodian: update this one"));
        }
        let device = file["device"].as_str().unwrap_or("").to_string();
        let seq = file["seq"].as_u64().unwrap_or(0);
        if device.is_empty() || device == me {
            continue;
        }
        let name = change_file(&device, seq);
        names.push(name.clone());
        for c in file["changes"].as_array().into_iter().flatten() {
            let change = Change {
                kind: c["kind"].as_str().unwrap_or("").to_string(),
                id: c["id"].as_str().unwrap_or("").to_string(),
                stamp: (c["stamp"].as_i64().unwrap_or(0), device.clone()),
                doc: Some(c["doc"].clone()).filter(|d| !d.is_null()),
                image: c.get("image").and_then(Value::as_str).map(str::to_string),
                keep_image: c["keep_image"] == true,
                file: name.clone(),
            };
            let key = (change.kind.clone(), change.id.clone());
            if latest.get(&key).is_none_or(|known| known.stamp < change.stamp) {
                latest.insert(key, change);
            }
        }
    }

    // Linked records first (campaigns before what's in them), so links resolve.
    let order = |kind: &str| Kind::ALL.iter().position(|k| k.plural() == kind).unwrap_or(Kind::ALL.len());
    let mut changes: Vec<Change> = latest.into_values().collect();
    changes.sort_by(|a, b| (order(&a.kind), &a.id).cmp(&(order(&b.kind), &b.id)));

    let mut applied = Applied::default();
    let mut again = std::collections::HashSet::new();
    set_state(repo, "applying", "1")?; // the triggers don't note these writes as local changes
    for change in &changes {
        repo.conn().execute_batch("SAVEPOINT change")?;
        match apply_one(repo, &me, change, images).await {
            Ok(true) => applied.changed += 1,
            Ok(false) => applied.older += 1,
            Err(NotYet(why)) => {
                repo.conn().execute_batch("ROLLBACK TO change")?;
                applied.skipped.push(format!("{} {}: {why}", change.kind, change.id));
                again.insert(change.file.clone());
            }
        }
        repo.conn().execute_batch("RELEASE change")?;
    }
    clear_state(repo, "applying")?;
    for name in names.iter().filter(|n| !again.contains(*n)) {
        repo.conn().execute("INSERT INTO sync_applied (name) VALUES (?) ON CONFLICT (name) DO NOTHING", [name])?;
    }
    let cutoff = (chrono::Utc::now() - KEEP_REPLACED).format("%Y-%m-%dT%H:%M:%S+00:00").to_string();
    repo.conn().execute("DELETE FROM sync_replaced WHERE replaced < ?", [cutoff])?;
    Ok(applied)
}

/// Apply one change: true if it was written, false if this device's version is later.
async fn apply_one(
    repo: &mut Repo<'_>,
    me: &str,
    change: &Change,
    images: &HashMap<String, Vec<u8>>,
) -> Result<bool, NotYet> {
    let Change { kind, id, stamp, .. } = change;
    let held = stamp_of(repo, kind, id)?;
    if held.as_ref().is_some_and(|held| stamp <= held) {
        return Ok(false);
    }
    let current = load(repo, kind, id).await?;
    if let Some(changed) = waiting_since(repo, kind, id)? {
        // Changed here too, and not sent yet: the later change wins.
        let ours: Stamp = (stamp_to_send(repo, kind, id, changed)?, me.to_string());
        if ours > *stamp {
            keep_replaced(repo, kind, id, change.doc.as_ref(), current.as_ref())?;
            return Ok(false);
        }
        repo.conn().execute("DELETE FROM sync_outbox WHERE kind = ? AND id = ?", [kind, id])?;
    }
    write(repo, change, current.as_ref(), images).await?;
    keep_replaced(repo, kind, id, current.as_ref(), change.doc.as_ref())?;
    set_stamp(repo, kind, id, stamp)?;
    Ok(true)
}

/// Keep `lost` to restore, unless it's no different from the version that won. A deletion
/// that lost isn't kept: there's nothing in it to restore.
fn keep_replaced(repo: &Repo<'_>, kind: &str, id: &str, lost: Option<&Value>, won: Option<&Value>) -> Result<()> {
    let Some(lost) = lost.filter(|&lost| Some(lost) != won) else {
        return Ok(());
    };
    repo.conn().execute(
        "INSERT INTO sync_replaced (kind, id, data, replaced) VALUES (?, ?, ?, ?)",
        params![kind, id, lost.to_string(), now()],
    )?;
    Ok(())
}

/// Write another device's version of a record here (or delete it).
async fn write(
    repo: &mut Repo<'_>,
    change: &Change,
    current: Option<&Value>,
    images: &HashMap<String, Vec<u8>>,
) -> Result<(), NotYet> {
    if change.kind == RULES {
        let (rule_kind, key) = rule_key(&change.id)?;
        match &change.doc {
            Some(doc) => {
                let data = Some(&doc["data"]).filter(|d| !d.is_null());
                repo.save_rule_override(&rule_kind, &key, data, doc["updated"].as_str()).await?;
            }
            None => {
                repo.delete_rule_override(&rule_kind, &key).await?;
            }
        }
        return Ok(());
    }
    let kind = record_kind(&change.kind)?;
    let old_file = current
        .filter(|_| kind == Kind::Handouts)
        .and_then(|c| handout_file(&repo.files_dir, c["filename"].as_str().unwrap_or("")));
    let Some(doc) = &change.doc else {
        if current.is_some() {
            keep_what_was_changed_since(repo, kind, &change.id, change.stamp.0).await?;
            repo.delete(kind, &change.id).await?;
            if let Some(path) = old_file {
                repo.remove_on_commit(path);
            }
        }
        return Ok(());
    };
    let mut doc = doc.clone();
    doc["id"] = json!(change.id);
    let mut unlinked = false;
    for (field, target) in LINKS {
        let link = doc.get(field).and_then(Value::as_str).unwrap_or("").to_string();
        if link.is_empty() || repo.exists(target, &link).await? {
            continue;
        }
        if stamp_of(repo, target.plural(), &link)?.is_none() {
            // Not here yet (it's in a file still to come): try again next time.
            return Err(NotYet(format!("its {} hasn't arrived yet", target.singular())));
        }
        // Deleted: keep this record without the link, and send that on, so every device agrees.
        doc[field] = json!("");
        unlinked = true;
    }
    let mut new_file = None;
    if kind == Kind::Handouts {
        let same =
            current.filter(|c| change.keep_image || change.image.is_some() && local_image(repo, c) == change.image);
        doc["filename"] = match (same, &change.image) {
            (Some(current), _) => current["filename"].clone(),
            (None, Some(image)) => {
                let bytes = images.get(image).ok_or_else(|| NotYet(format!("its image {image} wasn't fetched")))?;
                doc["filename"] = json!(image); // for its extension
                let name = store_image(&repo.files_dir, &doc, bytes)?;
                new_file = Some(repo.files_dir.join(&name));
                json!(name)
            }
            (None, None) => json!(""),
        };
    }
    if let Err(e) = repo.save(kind, doc.clone(), true).await {
        if let Some(path) = new_file {
            crate::files::remove(&path);
        }
        return Err(e.into());
    }
    if let Some(path) =
        old_file.filter(|old| doc["filename"].as_str().is_none_or(|f| !old.ends_with(f) || f.is_empty()))
    {
        repo.remove_on_commit(path);
    }
    if unlinked {
        queue(repo, &change.kind, &change.id, change.stamp.0 + 1)?;
    }
    Ok(())
}

/// Before deleting a record another device deleted: the records in it (a campaign's sessions,
/// say) that were changed here since, or by another device after the deletion, are kept,
/// unlinked, and sent on, so every device keeps them. The rest go with it, as deleting it here
/// would take them.
async fn keep_what_was_changed_since(repo: &mut Repo<'_>, kind: Kind, id: &str, deleted: i64) -> Result<()> {
    for (field, target) in LINKS {
        if target != kind {
            continue;
        }
        for child_kind in Kind::ALL {
            if !repo.has_column(child_kind, field) {
                continue;
            }
            let children: Vec<String> = repo
                .conn()
                .prepare(&format!("SELECT id FROM {} WHERE {field} = ?", child_kind.table()))?
                .query_map([id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            for child in children {
                let plural = child_kind.plural();
                let changed_here = waiting_since(repo, plural, &child)?.is_some();
                let changed_after = stamp_of(repo, plural, &child)?.is_some_and(|(stamp, _)| stamp > deleted);
                if changed_here || changed_after {
                    let mut doc = repo.get(child_kind, &child).await?;
                    doc[field] = json!("");
                    repo.save(child_kind, doc, true).await?;
                    queue(repo, plural, &child, deleted + 1)?;
                } else {
                    Box::pin(keep_what_was_changed_since(repo, child_kind, &child, deleted)).await?;
                }
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ sending

/// A change file to send, and the images it uses (send any the folder doesn't have first).
#[derive(Debug, Serialize)]
pub struct Outgoing {
    pub name: String,
    pub file: Value,
    pub images: Vec<OutgoingImage>,
}

#[derive(Debug, Serialize)]
pub struct OutgoingImage {
    /// Its name in the folder.
    pub name: String,
    /// The handout whose image it is (its file is at `/api/handouts/<id>/file`).
    pub handout: String,
}

/// The next change file to send, if anything's waiting. `listing` is the folder's files: if none
/// is this device's, everything here is sent (the first sync, or a new folder). Call [`sent`]
/// once it's in the folder; until then, the same changes stay waiting.
pub async fn outgoing(repo: &mut Repo<'_>, listing: &[String]) -> Result<Option<Outgoing>> {
    let me = device(repo)?;
    if !listing.iter().any(|name| parse_change_file(name).is_some_and(|(device, _)| device == me)) {
        queue_everything(repo)?;
    }
    let rows: Vec<(String, String, i64)> = repo
        .conn()
        .prepare_cached("SELECT kind, id, changed FROM sync_outbox ORDER BY changed, kind, id LIMIT ?")?
        .query_map([MAX_CHANGES as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    if rows.is_empty() {
        return Ok(None);
    }
    let mut changes = Vec::new();
    let mut plan = Vec::new();
    let mut images: Vec<OutgoingImage> = Vec::new();
    let mut size = 0;
    for (kind, id, changed) in rows {
        let stamp = stamp_to_send(repo, &kind, &id, changed)?;
        let doc = load(repo, &kind, &id).await?;
        let mut change = json!({"kind": kind, "id": id, "stamp": stamp, "doc": doc});
        if kind == Kind::Handouts.plural()
            && let Some(doc) = &doc
            && !doc["filename"].as_str().unwrap_or("").is_empty()
        {
            match local_image(repo, doc) {
                Some(image) => {
                    if !images.iter().any(|i| i.name == image) {
                        images.push(OutgoingImage { name: image.clone(), handout: id.clone() });
                    }
                    change["image"] = json!(image);
                }
                // It has one, but its file can't be read here: other devices keep theirs.
                None => change["keep_image"] = json!(true),
            }
        }
        size += change.to_string().len();
        changes.push(change);
        plan.push(json!([kind, id, changed, stamp]));
        if size > MAX_BYTES {
            break;
        }
    }
    let seq = state(repo, "seq")?.and_then(|s| s.parse::<u64>().ok()).unwrap_or(0) + 1;
    set_state(repo, "seq", &seq.to_string())?;
    let name = change_file(&me, seq);
    set_state(repo, "sending", &json!({"name": name, "changes": plan}).to_string())?;
    let file =
        json!({"format": FORMAT, "version": VERSION, "device": me, "seq": seq, "made": now(), "changes": changes});
    Ok(Some(Outgoing { name, file, images }))
}

/// The change file [`outgoing`] gave is in the folder: its changes are no longer waiting,
/// unless they changed again meanwhile.
pub async fn sent(repo: &mut Repo<'_>, name: &str) -> Result<()> {
    let me = device(repo)?;
    let plan: Value = state(repo, "sending")?.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null);
    if plan["name"] != name {
        return Err(invalid(format!("{name} isn't the change file being sent")));
    }
    for entry in plan["changes"].as_array().into_iter().flatten() {
        let (kind, id) = (entry[0].as_str().unwrap_or(""), entry[1].as_str().unwrap_or(""));
        let (changed, stamp) = (entry[2].as_i64().unwrap_or(0), entry[3].as_i64().unwrap_or(0));
        let done = repo
            .conn()
            .execute("DELETE FROM sync_outbox WHERE kind = ? AND id = ? AND changed = ?", params![kind, id, changed])?;
        // Unless it changed again meanwhile, or another device's later version was applied.
        if done > 0 {
            set_stamp(repo, kind, id, &(stamp, me.clone()))?;
        }
    }
    clear_state(repo, "sending")
}

/// Note every record this device made or last changed as changed (when it was last updated),
/// to send them all: the folder doesn't have this device's files. Records last changed on
/// another device are that device's to send.
fn queue_everything(repo: &Repo<'_>) -> Result<()> {
    let me = device(repo)?;
    // Never synced, or last changed here.
    let own = |kind: &str, id: &str| {
        format!("NOT EXISTS (SELECT 1 FROM sync_stamps s WHERE s.kind = {kind} AND s.id = {id} AND s.device <> ?1)")
    };
    let millis = |column: &str| format!("COALESCE(CAST((julianday({column}) - 2440587.5) * 86400000 AS INTEGER), 0)");
    for kind in Kind::ALL {
        repo.conn().execute(
            &format!(
                "INSERT INTO sync_outbox (kind, id, changed) SELECT '{plural}', id, {time} FROM {table}
                 WHERE {own} ON CONFLICT (kind, id) DO NOTHING",
                plural = kind.plural(),
                table = kind.table(),
                time = millis("updated"),
                own = own(&format!("'{}'", kind.plural()), "id"),
            ),
            [&me],
        )?;
    }
    repo.conn().execute(
        &format!(
            r#"INSERT INTO sync_outbox (kind, id, changed) SELECT '{RULES}', json_array(kind, "key"), {time} FROM rule_overrides
               WHERE {own} ON CONFLICT (kind, id) DO NOTHING"#,
            time = millis("updated"),
            own = own(&format!("'{RULES}'"), r#"json_array(kind, "key")"#),
        ),
        [&me],
    )?;
    Ok(())
}
