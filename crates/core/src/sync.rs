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
/// The latest version of each record in the files being applied (its stamp, and the record or
/// None if deleted), by kind and id.
type Batch = HashMap<(String, String), (i64, Option<Value>)>;
/// In `sync_outbox`, for a record waiting only to be sent again (to a new folder) as it is, under
/// the stamp it has: not a change made here.
const RESEND: i64 = 0;

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

/// Where an image fetched from the folder waits to be used (by [`apply`], or a restore's import).
/// Only names sync gives are accepted.
pub fn fetched_image(files_dir: &Path, name: &str) -> Result<std::path::PathBuf> {
    let valid = name.strip_prefix("image-").and_then(|rest| rest.split_once('.')).is_some_and(|(hash, extension)| {
        hash.len() == 64
            && hash.bytes().all(|b| b.is_ascii_hexdigit())
            && !extension.is_empty()
            && extension.bytes().all(|b| b.is_ascii_alphanumeric())
    });
    if !valid {
        return Err(invalid(format!("not a sync image name: {name}")));
    }
    Ok(files_dir.join("sync-fetched").join(name))
}

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
                // Sent on by another device (to a new folder), it keeps the device that made it.
                stamp: (
                    c["stamp"].as_i64().unwrap_or(0),
                    c["device"].as_str().map_or_else(|| device.clone(), str::to_string),
                ),
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
    let batch: Batch = latest.iter().map(|(key, change)| (key.clone(), (change.stamp.0, change.doc.clone()))).collect();
    let mut changes: Vec<Change> = latest.into_values().collect();
    changes.sort_by(|a, b| (order(&a.kind), &a.id).cmp(&(order(&b.kind), &b.id)));

    let mut applied = Applied::default();
    let mut again = std::collections::HashSet::new();
    set_state(repo, "applying", "1")?; // the triggers don't note these writes as local changes
    for change in &changes {
        repo.conn().execute_batch("SAVEPOINT change")?;
        match apply_one(repo, &me, change, images, &batch).await {
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
    batch: &Batch,
) -> Result<bool, NotYet> {
    let Change { kind, id, stamp, .. } = change;
    let held = stamp_of(repo, kind, id)?;
    if held.as_ref().is_some_and(|held| stamp <= held) {
        return Ok(false);
    }
    let current = load(repo, kind, id).await?;
    if let Some(changed) = waiting_since(repo, kind, id)? {
        // Changed here too, and not sent yet: the later change wins.
        if changed != RESEND {
            let ours: Stamp = (stamp_to_send(repo, kind, id, changed)?, me.to_string());
            if ours > *stamp {
                // Another device's version, kept with the name of its image in the folder.
                let mut theirs = change.doc.clone();
                if let (Some(doc), Some(image)) = (theirs.as_mut(), &change.image) {
                    doc["sync_image"] = json!(image);
                }
                keep_replaced(repo, kind, id, theirs.as_ref(), current.as_ref())?;
                return Ok(false);
            }
        }
        repo.conn().execute("DELETE FROM sync_outbox WHERE kind = ? AND id = ?", [kind, id])?;
    }
    let written = write(repo, change, current.as_ref(), images, batch).await?;
    let rest = keep_replaced(repo, kind, id, current.as_ref(), change.doc.as_ref())
        .and_then(|()| set_stamp(repo, kind, id, stamp));
    if let Err(e) = rest {
        if let Some(path) = &written.new_file {
            crate::files::remove(path);
        }
        return Err(e.into());
    }
    // Only now, with everything for this change done: an image the record no longer uses.
    if let Some(path) = written.old_file {
        repo.remove_on_commit(path);
    }
    Ok(true)
}

/// The image files a write made and left unused.
#[derive(Default)]
struct Written {
    /// Made for the new version (removed if the change is undone).
    new_file: Option<std::path::PathBuf>,
    /// No longer used (removed when the sync is saved).
    old_file: Option<std::path::PathBuf>,
}

/// Keep `lost` to restore, unless it's no different from the version that won. A deletion
/// that lost isn't kept: there's nothing in it to restore.
fn keep_replaced(repo: &Repo<'_>, kind: &str, id: &str, lost: Option<&Value>, won: Option<&Value>) -> Result<()> {
    let Some(lost) = lost.filter(|&lost| Some(lost) != won) else {
        return Ok(());
    };
    // A handout's image file here goes with the version: note its name in the folder, to fetch
    // it from there if this version is restored.
    let mut lost = lost.clone();
    if kind == Kind::Handouts.plural()
        && lost.get("sync_image").is_none()
        && let Some(image) = local_image(repo, &lost)
    {
        lost["sync_image"] = json!(image);
    }
    // Once: a change put off and tried again would keep it again.
    repo.conn().execute(
        "INSERT INTO sync_replaced (kind, id, data, replaced) SELECT ?1, ?2, ?3, ?4
         WHERE NOT EXISTS (SELECT 1 FROM sync_replaced WHERE kind = ?1 AND id = ?2 AND data = ?3)",
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
    batch: &Batch,
) -> Result<Written, NotYet> {
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
        return Ok(Written::default());
    }
    let kind = record_kind(&change.kind)?;
    let old_file = current
        .filter(|_| kind == Kind::Handouts)
        .and_then(|c| handout_file(&repo.files_dir, c["filename"].as_str().unwrap_or("")));
    let Some(doc) = &change.doc else {
        if current.is_none() {
            return Ok(Written::default());
        }
        keep_what_was_changed_since(repo, kind, &change.id, change.stamp.0, batch).await?;
        repo.delete(kind, &change.id).await?;
        return Ok(Written { new_file: None, old_file });
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
    let saved = repo.save(kind, doc.clone(), true).await;
    let queued = match saved {
        Ok(_) if unlinked => queue(repo, &change.kind, &change.id, change.stamp.0 + 1),
        Ok(_) => Ok(()),
        Err(e) => Err(e),
    };
    if let Err(e) = queued {
        if let Some(path) = &new_file {
            crate::files::remove(path);
        }
        return Err(e.into());
    }
    let filename = doc["filename"].as_str().unwrap_or("");
    let old_file = old_file.filter(|old| filename.is_empty() || !old.ends_with(filename));
    Ok(Written { new_file, old_file })
}

/// Before deleting a record another device deleted: the records that would go with it (a
/// campaign's sessions, say) that were changed after the deletion (here, or by another device)
/// are kept, unlinked, and sent on, so every device keeps them. The others go with it; a change
/// to one made here before the deletion is kept to restore.
async fn keep_what_was_changed_since(
    repo: &mut Repo<'_>,
    kind: Kind,
    id: &str,
    deleted: i64,
    batch: &Batch,
) -> Result<()> {
    for (field, target) in LINKS {
        if target != kind {
            continue;
        }
        for child_kind in Kind::ALL {
            // Only what deleting it takes along: other links are just cleared, by every device.
            if !repo.has_column(child_kind, field) || !cascades(repo, child_kind, field)? {
                continue;
            }
            let children: Vec<String> = repo
                .conn()
                .prepare(&format!("SELECT id FROM {} WHERE {field} = ?", child_kind.table()))?
                .query_map([id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            for child in children {
                let plural = child_kind.plural();
                let incoming = batch.get(&(plural.to_string(), child.clone()));
                // Moved out of it in a version about to be applied: that version is the one to keep.
                if incoming.is_some_and(|(_, doc)| doc.as_ref().is_some_and(|d| d[field].as_str() != Some(id))) {
                    let mut doc = repo.get(child_kind, &child).await?;
                    doc[field] = json!("");
                    repo.save(child_kind, doc, true).await?;
                    continue;
                }
                let waiting = waiting_since(repo, plural, &child)?.filter(|&c| c != RESEND);
                let later = waiting.is_some_and(|c| c > deleted)
                    || stamp_of(repo, plural, &child)?.is_some_and(|(stamp, _)| stamp > deleted)
                    || incoming.is_some_and(|(stamp, _)| *stamp > deleted);
                if later {
                    let mut doc = repo.get(child_kind, &child).await?;
                    doc[field] = json!("");
                    repo.save(child_kind, doc, true).await?;
                    queue(repo, plural, &child, deleted + 1)?;
                    continue;
                }
                if waiting.is_some() {
                    let doc = repo.get(child_kind, &child).await?;
                    keep_replaced(repo, plural, &child, Some(&doc), None)?;
                }
                Box::pin(keep_what_was_changed_since(repo, child_kind, &child, deleted, batch)).await?;
            }
        }
    }
    Ok(())
}

/// Whether deleting what `field` links to deletes records of this kind too (rather than
/// clearing their link).
fn cascades(repo: &Repo<'_>, kind: Kind, field: &str) -> Result<bool> {
    let mut statement = repo.conn().prepare(&format!("PRAGMA foreign_key_list({})", kind.table()))?;
    let keys = statement
        .query_map([], |r| Ok((r.get::<_, String>("from")?, r.get::<_, String>("on_delete")?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(keys.iter().any(|(from, on_delete)| from == field && on_delete.eq_ignore_ascii_case("CASCADE")))
}

// ------------------------------------------------------------------ versions kept to restore

/// The versions kept to restore, newest first: what each is, when it was replaced, and whether
/// the record still exists here.
pub async fn replaced(repo: &mut Repo<'_>) -> Result<Vec<Value>> {
    let rows: Vec<(i64, String, String, String, String)> = repo
        .conn()
        .prepare("SELECT n, kind, id, data, replaced FROM sync_replaced ORDER BY n DESC")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut out = Vec::new();
    for (n, kind, id, data, replaced) in rows {
        let data: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
        let title = if kind == RULES {
            rule_key(&id).map(|(_, key)| key).unwrap_or_default()
        } else {
            crate::repo::title_of(&data)
        };
        let exists = load(repo, &kind, &id).await?.is_some();
        out.push(json!({"n": n, "kind": kind, "id": id, "title": title, "replaced": replaced, "exists": exists,
            "image": data.get("sync_image")}));
    }
    Ok(out)
}

/// Bring back a kept version, as a change made here now (so sync sends it, and it wins). What
/// it replaces is kept in its turn. A handout's image comes from here if its file is still here,
/// else from one fetched from the folder (`PUT /api/sync/images/<name>`); without either it comes
/// back without its image. Says whether its image came back.
pub async fn restore_replaced(repo: &mut Repo<'_>, n: i64) -> Result<Value> {
    let row: Option<(String, String, String)> = repo
        .conn()
        .query_row("SELECT kind, id, data FROM sync_replaced WHERE n = ?", [n], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .optional()?;
    let (kind, id, data) = row.ok_or_else(|| crate::error::not_found("that version is no longer kept"))?;
    let mut doc: Value =
        serde_json::from_str(&data).map_err(|e| invalid(format!("a kept version can't be read: {e}")))?;
    let current = load(repo, &kind, &id).await?;
    let mut image_back = true;
    if kind == RULES {
        let (rule_kind, key) = rule_key(&id)?;
        let data = Some(&doc["data"]).filter(|d| !d.is_null());
        repo.save_rule_override(&rule_kind, &key, data, None).await?;
    } else {
        let record = record_kind(&kind)?;
        let image = doc.as_object_mut().and_then(|d| d.remove("sync_image"));
        doc["id"] = json!(id);
        for (field, target) in LINKS {
            let link = doc.get(field).and_then(Value::as_str).unwrap_or("").to_string();
            if !link.is_empty() && !repo.exists(target, &link).await? {
                doc[field] = json!(""); // what it was in is gone
            }
        }
        if record == Kind::Handouts {
            let filename = doc["filename"].as_str().unwrap_or("").to_string();
            let here = handout_file(&repo.files_dir, &filename).is_some_and(|f| crate::files::read(&f).is_some());
            if !here {
                let fetched = match image.as_ref().and_then(Value::as_str) {
                    Some(name) => {
                        let path = fetched_image(&repo.files_dir, name)?;
                        crate::files::read(&path).map(|bytes| (path, bytes, name.to_string()))
                    }
                    None => None,
                };
                doc["filename"] = match fetched {
                    Some((path, bytes, name)) => {
                        doc["filename"] = json!(name); // for its extension
                        let stored = store_image(&repo.files_dir, &doc, &bytes)?;
                        repo.remove_on_commit(path);
                        json!(stored)
                    }
                    None => {
                        image_back = filename.is_empty();
                        json!("")
                    }
                };
            }
            if let Some(path) =
                current.as_ref().and_then(|c| handout_file(&repo.files_dir, c["filename"].as_str().unwrap_or("")))
                && Some(path.file_name()) != Some(Path::new(doc["filename"].as_str().unwrap_or("")).file_name())
            {
                repo.remove_on_commit(path);
            }
        }
        repo.save(record, doc.clone(), false).await?;
    }
    let restored = load(repo, &kind, &id).await?;
    keep_replaced(repo, &kind, &id, current.as_ref(), restored.as_ref())?;
    repo.conn().execute("DELETE FROM sync_replaced WHERE n = ?", [n])?;
    Ok(json!({"restored": restored, "image": image_back}))
}

/// Stop keeping a version.
pub async fn forget_replaced(repo: &mut Repo<'_>, n: i64) -> Result<()> {
    repo.conn().execute("DELETE FROM sync_replaced WHERE n = ?", [n])?;
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
    let in_folder: std::collections::HashSet<&str> =
        listing.iter().filter_map(|n| parse_change_file(n)).map(|(d, _)| d).collect();
    if !in_folder.contains(me.as_str()) {
        queue_everything(repo, &in_folder)?;
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
        let (stamp, made_by) = match stamp_of(repo, &kind, &id)? {
            Some(held) if changed == RESEND => held,
            _ => (stamp_to_send(repo, &kind, &id, changed)?, me.clone()),
        };
        let doc = load(repo, &kind, &id).await?;
        let mut change = json!({"kind": kind, "id": id, "stamp": stamp, "doc": doc});
        if made_by != me {
            change["device"] = json!(made_by);
        }
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
        plan.push(json!([kind, id, changed, stamp, made_by]));
        if size > MAX_BYTES {
            break;
        }
    }
    // After the last this device sent, here or (for a database restored from a copy) in the folder.
    let in_listing =
        listing.iter().filter_map(|n| parse_change_file(n)).filter(|(d, _)| *d == me).map(|(_, seq)| seq).max();
    let seq = state(repo, "seq")?.and_then(|s| s.parse::<u64>().ok()).unwrap_or(0).max(in_listing.unwrap_or(0)) + 1;
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
        let made_by = entry[4].as_str().unwrap_or(&me).to_string();
        let done = repo
            .conn()
            .execute("DELETE FROM sync_outbox WHERE kind = ? AND id = ? AND changed = ?", params![kind, id, changed])?;
        // Unless it changed again meanwhile, or another device's later version was applied.
        if done > 0 {
            set_stamp(repo, kind, id, &(stamp, made_by))?;
        }
    }
    clear_state(repo, "sending")
}

/// The folder has none of this device's files (the first sync, or a new folder): queue every
/// record to send. Records never synced go as changes made when they were last updated; the
/// others go again as they are, under the stamp they have, unless the device that made that
/// version has files in the folder (it sent them).
fn queue_everything(repo: &Repo<'_>, in_folder: &std::collections::HashSet<&str>) -> Result<()> {
    let devices = json!(in_folder.iter().collect::<Vec<_>>()).to_string();
    let millis = |column: &str| format!("COALESCE(CAST((julianday({column}) - 2440587.5) * 86400000 AS INTEGER), 0)");
    let mut sources: Vec<(String, String, String, String)> = Kind::ALL
        .iter()
        .map(|k| (format!("'{}'", k.plural()), k.table().to_string(), "t.id".to_string(), millis("t.updated")))
        .collect();
    sources.push((
        format!("'{RULES}'"),
        "rule_overrides".into(),
        r#"json_array(t.kind, t."key")"#.into(),
        millis("t.updated"),
    ));
    for (kind, table, id, time) in sources {
        repo.conn().execute(
            &format!(
                "INSERT INTO sync_outbox (kind, id, changed)
                 SELECT {kind}, {id}, CASE WHEN s.stamp IS NULL THEN {time} ELSE {RESEND} END
                 FROM {table} t LEFT JOIN sync_stamps s ON s.kind = {kind} AND s.id = {id}
                 WHERE s.stamp IS NULL OR s.device NOT IN (SELECT value FROM json_each(?))
                 ON CONFLICT (kind, id) DO NOTHING"
            ),
            [&devices],
        )?;
    }
    Ok(())
}
