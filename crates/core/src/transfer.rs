//! Import and export: JSON bundles, single records, and Markdown.
//!
//! The format is the one the first (Python) version wrote, so its backups and exports
//! import unchanged.

use std::collections::HashMap;
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{Map, Value, json};

use crate::builder;
use crate::error::{Error, Result, invalid};
use crate::files;
use crate::kinds::{Kind, LINKS};
use crate::repo::{Repo, new_id, now, valid_id};
use crate::rules::{self, ABILITIES, ABILITY_NAMES};
use crate::schemas::{Character, Edition, Session};

pub const FORMAT: &str = "questodian";
/// The bundle format. 1: a session holds its encounter. 2: DM screen records, encounters on
/// their own. 3: characters' builder choices, rules data edits and drawn maps (older versions
/// refuse these files rather than import them without those).
pub const VERSION: i64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Keep ids; replace existing records.
    Overwrite,
    /// Keep ids; leave existing records alone.
    Skip,
    /// Give every record a new id (rewriting links between them), so nothing is replaced.
    Copy,
    /// Make the data exactly what a full backup holds: overwrite, and remove every record (and
    /// rules-data edit) the backup doesn't have. Restoring a cloud backup.
    Replace,
}

impl Mode {
    pub fn parse(text: &str) -> Result<Mode> {
        match text {
            "overwrite" => Ok(Mode::Overwrite),
            "skip" => Ok(Mode::Skip),
            "copy" => Ok(Mode::Copy),
            "replace" => Ok(Mode::Replace),
            _ => Err(invalid("mode must be one of overwrite, skip, copy, replace")),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Mode::Overwrite => "overwrite",
            Mode::Skip => "skip",
            Mode::Copy => "copy",
            Mode::Replace => "replace",
        }
    }
}

fn text<'a>(doc: &'a Value, key: &str) -> &'a str {
    doc.get(key).and_then(Value::as_str).unwrap_or("")
}

// --------------------------------------------------------------------------- export

/// The collection holding edits to the character builder's rules data (full exports only).
const RULE_EDITS: &str = "rule_overrides";

/// Everything, or one campaign together with the records linked to it. Handout images
/// are embedded (base64) so the bundle is self-contained.
pub async fn export_bundle(repo: &mut Repo<'_>, campaign_id: Option<&str>) -> Result<Value> {
    let mut collections = Map::new();
    match campaign_id {
        Some(id) => {
            let campaign = repo.get(Kind::Campaigns, id).await?;
            let linked: Vec<String> = campaign["players"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|p| text(p, "character_id").to_string())
                .filter(|c| !c.is_empty())
                .collect();
            collections.insert("campaigns".into(), json!([campaign]));
            for kind in &Kind::ALL[1..] {
                let docs = if *kind == Kind::Characters {
                    let all = repo.list(Kind::Characters, &[]).await?;
                    all.into_iter()
                        .filter(|c| text(c, "campaign_id") == id || linked.iter().any(|l| l == text(c, "id")))
                        .collect()
                } else {
                    repo.list(*kind, &[("campaign_id", id)]).await?
                };
                collections.insert(kind.plural().into(), Value::Array(docs));
            }
        }
        None => {
            for kind in Kind::ALL {
                collections.insert(kind.plural().into(), Value::Array(repo.list(kind, &[]).await?));
            }
            let edits = repo.rule_overrides().await?;
            let edits = edits
                .into_iter()
                .map(|o| json!({"kind": o.kind, "key": o.key, "data": o.data, "updated": o.updated}))
                .collect();
            collections.insert(RULE_EDITS.into(), Value::Array(edits));
        }
    }
    if let Some(Value::Array(handouts)) = collections.get_mut("handouts") {
        for handout in handouts {
            let filename = text(handout, "filename").to_string();
            if let Some(path) = handout_file(&repo.files_dir, &filename)
                && let Some(bytes) = files::read(&path)
            {
                handout["file_b64"] = Value::String(BASE64.encode(bytes));
            }
        }
    }
    Ok(json!({"format": FORMAT, "version": VERSION, "exported": now(), "collections": collections}))
}

/// Remove every record and rules-data edit that isn't in `docs` and `edits` (for [`Mode::Replace`]).
/// What's in them goes later kinds first, so a campaign's own records go before it does.
async fn remove_all_but(repo: &mut Repo<'_>, docs: &Checked, edits: &[RuleEdit]) -> Result<()> {
    for kind in Kind::ALL.iter().rev() {
        let keep: std::collections::HashSet<&str> =
            docs.get(kind).into_iter().flatten().map(|(doc, _)| text(doc, "id")).collect();
        for doc in repo.list(*kind, &[]).await? {
            let id = text(&doc, "id");
            if keep.contains(id) || !repo.exists(*kind, id).await? {
                continue;
            }
            if *kind == Kind::Handouts
                && let Some(path) = handout_file(&repo.files_dir, text(&doc, "filename"))
            {
                repo.remove_on_commit(path);
            }
            repo.delete(*kind, id).await?;
        }
    }
    for edit in repo.rule_overrides().await? {
        if !edits.iter().any(|e| e.kind == edit.kind && e.key == edit.key) {
            repo.delete_rule_override(&edit.kind, &edit.key).await?;
        }
    }
    Ok(())
}

/// One record in an envelope that says what it is.
pub async fn export_one(repo: &mut Repo<'_>, kind: Kind, id: &str) -> Result<Value> {
    let data = repo.get(kind, id).await?;
    Ok(json!({"format": FORMAT, "version": VERSION, "exported": now(), "kind": kind.singular(), "data": data}))
}

/// A stored handout file name is only ever a plain name inside the handouts folder.
pub fn handout_file(dir: &Path, filename: &str) -> Option<std::path::PathBuf> {
    let plain = !filename.is_empty() && !filename.contains(['/', '\\']) && filename != "." && filename != "..";
    plain.then(|| dir.join(filename))
}

// --------------------------------------------------------------------------- import

/// The records in an import file, by kind, after upgrading older formats.
fn collections_from(payload: &Value) -> Result<HashMap<Kind, Vec<Value>>> {
    let Value::Object(payload) = payload else { return Err(invalid("expected a JSON object")) };
    match payload.get("format") {
        None | Some(Value::Null) => {}
        Some(Value::String(f)) if f == FORMAT => {}
        Some(other) => return Err(invalid(format!("not a Questodian file (format={})", python_repr(other)))),
    }
    let version = match payload.get("version") {
        None | Some(Value::Null) => VERSION,
        Some(v) => python_int(v).ok_or_else(|| invalid(format!("invalid version {}", python_repr(v))))?,
    };
    if version > VERSION {
        return Err(invalid("this file was made by a newer version of Questodian"));
    }
    let objects = |docs: &[Value]| docs.iter().filter(|d| d.is_object()).cloned().collect::<Vec<_>>();
    let mut out = HashMap::new();
    if let Some(collections) = payload.get("collections") {
        let Value::Object(collections) = collections else { return Err(invalid("'collections' must be an object")) };
        for (name, docs) in collections {
            let Some(kind) = Kind::from_plural(name) else { continue };
            let Value::Array(docs) = docs else { return Err(invalid(format!("'{name}' must be a list"))) };
            out.insert(kind, objects(docs));
        }
    } else if let (Some(kind), Some(data)) = (payload.get("kind"), payload.get("data")) {
        let name = kind.as_str().unwrap_or("");
        let kind = Kind::from_singular(name)
            .or_else(|| Kind::from_plural(name))
            .ok_or_else(|| invalid(format!("unknown record kind {}", python_repr(kind))))?;
        let docs = match data {
            Value::Array(list) => objects(list),
            single => objects(std::slice::from_ref(single)),
        };
        out.insert(kind, docs);
    } else {
        return Err(invalid("file has neither 'collections' nor 'kind'/'data'"));
    }
    upgrade_v1(&mut out);
    Ok(out)
}

/// One rules data edit from a full export.
struct RuleEdit {
    kind: String,
    key: String,
    data: Option<Value>,
    updated: Option<String>,
}

/// Edits to the rules data in a full export, checked like edits made in the app. An edit that
/// doesn't fit this version's rules data (hiding an entry that's no longer bundled, a kind it
/// doesn't know) is left out and counted, rather than refusing the whole file.
fn rule_edits_from(payload: &Value) -> Result<(Vec<RuleEdit>, usize)> {
    let Some(edits) = payload.get("collections").and_then(|c| c.get(RULE_EDITS)) else { return Ok((Vec::new(), 0)) };
    let Value::Array(edits) = edits else { return Err(invalid(format!("'{RULE_EDITS}' must be a list"))) };
    let mut out = Vec::new();
    let mut unusable = 0;
    for edit in edits {
        let (kind, key) = (text(edit, "kind"), text(edit, "key").trim());
        let data = edit.get("data").filter(|d| !d.is_null()).cloned();
        match builder::check(kind, key, data) {
            Ok(data) => out.push(RuleEdit {
                kind: kind.to_string(),
                key: key.to_string(),
                data,
                updated: Some(text(edit, "updated").to_string()).filter(|u| !u.is_empty()),
            }),
            Err(_) => unusable += 1,
        }
    }
    Ok((out, unusable))
}

/// Version 1 sessions carried one `encounter`; turn those into encounter records.
fn upgrade_v1(collections: &mut HashMap<Kind, Vec<Value>>) {
    let mut found = Vec::new();
    for session in collections.get(&Kind::Sessions).into_iter().flatten() {
        let Some(Value::Object(encounter)) = session.get("encounter") else { continue };
        if !encounter.get("combatants").is_some_and(truthy) {
            continue;
        }
        let mut doc: Map<String, Value> = ["round", "turn", "active", "combatants"]
            .iter()
            .filter_map(|k| encounter.get(*k).map(|v| (k.to_string(), v.clone())))
            .collect();
        let session_id = session.get("id").filter(|v| truthy(v)).map(python_str).unwrap_or_else(new_id);
        doc.insert("id".into(), Value::String(format!("{session_id}-enc").chars().take(64).collect()));
        doc.insert("campaign_id".into(), session.get("campaign_id").cloned().unwrap_or(json!("")));
        doc.insert("session_id".into(), session.get("id").cloned().unwrap_or(json!("")));
        let field = |key: &str| session.get(key).map(python_str).unwrap_or_default();
        doc.insert("name".into(), json!(format!("Session {}: {}", field("number"), field("title")).trim()));
        found.push(Value::Object(doc));
    }
    if !found.is_empty() {
        collections.entry(Kind::Encounters).or_default().extend(found);
    }
}

/// Validated records by kind, each with its embedded image (handouts only).
type Checked = HashMap<Kind, Vec<(Value, Option<Vec<u8>>)>>;

/// Import a bundle or single-record export. Everything is checked before anything is
/// written, and the caller's unit of work makes it all-or-nothing.
pub async fn import_data(repo: &mut Repo<'_>, payload: &Value, mode: Mode) -> Result<Value> {
    let asked = mode;
    // Only a full backup (which has the rules-data edits, even none) can stand for everything.
    if mode == Mode::Replace && payload.get("collections").and_then(|c| c.get(RULE_EDITS)).is_none() {
        return Err(invalid("only a full backup can replace everything"));
    }
    let mode = if mode == Mode::Replace { Mode::Overwrite } else { mode };
    let collections = collections_from(payload)?;
    let (rule_edits, unusable_edits) = rule_edits_from(payload)?;
    let mut docs: Checked = HashMap::new();
    for kind in Kind::ALL {
        let mut checked = Vec::new();
        for raw in collections.get(&kind).into_iter().flatten() {
            let image = match raw.get("file_b64") {
                Some(b64) if kind == Kind::Handouts && truthy(b64) => {
                    let decoded = b64.as_str().and_then(|s| BASE64.decode(s).ok());
                    let title = python_repr(raw.get("title").unwrap_or(&json!("")));
                    Some(decoded.ok_or_else(|| invalid(format!("handout {title} has a broken image")))?)
                }
                _ => None,
            };
            let doc = kind.normalize(raw.clone()).map_err(|e| invalid(format!("invalid record: {e}")))?;
            checked.push((doc, image));
        }
        docs.insert(kind, checked);
    }
    if mode == Mode::Copy {
        remap_ids(&mut docs);
    }

    let mut imported = Map::new();
    let mut skipped = Map::new();
    let mut written_files = Vec::new();
    let result = async {
        if asked == Mode::Replace {
            remove_all_but(repo, &docs, &rule_edits).await?;
        }
        for kind in Kind::ALL {
            let (mut done, mut passed) = (0, 0);
            for (mut doc, image) in docs.remove(&kind).unwrap_or_default() {
                if !valid_id(text(&doc, "id")) {
                    doc["id"] = json!(new_id());
                }
                let id = text(&doc, "id").to_string();
                if mode == Mode::Skip && repo.exists(kind, &id).await? {
                    passed += 1;
                    continue;
                }
                // Linked kinds are written first, so a link that still dangles points outside this import.
                for (field, target) in LINKS {
                    let link = doc.get(field).and_then(Value::as_str).unwrap_or("").to_string();
                    if !link.is_empty() && !repo.exists(target, &link).await? {
                        doc[field] = json!("");
                    }
                }
                if kind == Kind::Handouts {
                    // Overwriting a handout replaces its image; the old file goes once the import is saved.
                    if mode == Mode::Overwrite && repo.exists(kind, &id).await? {
                        let old = repo.get(kind, &id).await?;
                        if let Some(path) = handout_file(&repo.files_dir, text(&old, "filename")) {
                            repo.remove_on_commit(path);
                        }
                    }
                    let name = match image {
                        Some(bytes) => store_image(&repo.files_dir, &doc, &bytes)?,
                        None => String::new(),
                    };
                    if !name.is_empty() {
                        written_files.push(repo.files_dir.join(&name));
                    }
                    doc["filename"] = json!(name);
                }
                repo.save(kind, doc, true).await?;
                done += 1;
            }
            if done > 0 {
                imported.insert(kind.plural().into(), json!(done));
            }
            if passed > 0 {
                skipped.insert(kind.plural().into(), json!(passed));
            }
        }
        if !rule_edits.is_empty() || unusable_edits > 0 {
            let existing = repo.rule_overrides().await?;
            let (mut done, mut passed) = (0, unusable_edits);
            for edit in &rule_edits {
                let exists = existing.iter().any(|o| o.kind == edit.kind && o.key == edit.key);
                if exists && mode != Mode::Overwrite {
                    passed += 1;
                    continue;
                }
                repo.save_rule_override(&edit.kind, &edit.key, edit.data.as_ref(), edit.updated.as_deref()).await?;
                done += 1;
            }
            if done > 0 {
                imported.insert(RULE_EDITS.into(), json!(done));
            }
            if passed > 0 {
                skipped.insert(RULE_EDITS.into(), json!(passed));
            }
        }
        Ok::<(), Error>(())
    }
    .await;
    if let Err(e) = result {
        // The records roll back with the transaction; take the image files with them.
        for file in written_files {
            files::remove(&file);
        }
        return Err(e);
    }
    Ok(json!({"imported": imported, "skipped": skipped, "mode": asked.name()}))
}

pub(crate) fn store_image(dir: &Path, doc: &Value, bytes: &[u8]) -> Result<String> {
    // The old file's extension, like Python's `Path.suffix` (".png" alone has none).
    let extension = Path::new(text(doc, "filename"))
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .filter(|e| !e.is_empty() && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .map_or_else(|| ".png".to_string(), |e| format!(".{e}"));
    let name = format!("{}-{}{extension}", text(doc, "id"), &new_id()[..6]);
    files::write(&dir.join(&name), bytes)?;
    Ok(name)
}

/// Give every record a new id and rewrite the references between them.
fn remap_ids(docs: &mut Checked) {
    let mut remap: HashMap<String, String> = HashMap::new();
    for kind in Kind::ALL {
        for (doc, _) in docs.get_mut(&kind).into_iter().flatten() {
            let old = Some(text(doc, "id").to_string()).filter(|s| !s.is_empty()).unwrap_or_else(new_id);
            let fresh = new_id();
            remap.insert(old, fresh.clone());
            doc["id"] = json!(fresh);
        }
    }
    let swap = |value: &mut Value| {
        if let Some(new) = value.as_str().and_then(|old| remap.get(old)) {
            *value = json!(new);
        }
    };
    for (kind, list) in docs.iter_mut() {
        for (doc, _) in list {
            for (field, _) in LINKS {
                if let Some(link) = doc.get_mut(field) {
                    swap(link);
                }
            }
            match kind {
                Kind::Campaigns => {
                    doc["players"].as_array_mut().into_iter().flatten().for_each(|p| swap(&mut p["character_id"]))
                }
                Kind::Sessions => doc["attendance"].as_array_mut().into_iter().flatten().for_each(swap),
                Kind::Encounters => {
                    for c in doc["combatants"].as_array_mut().into_iter().flatten() {
                        for field in ["character_id", "monster_id", "npc_id"] {
                            swap(&mut c[field]);
                        }
                    }
                }
                Kind::Handouts => {
                    let tokens = doc.get_mut("map").and_then(|m| m.get_mut("tokens")).and_then(Value::as_array_mut);
                    for link in tokens.into_iter().flatten().filter_map(|t| t.get_mut("character_id")) {
                        swap(link);
                    }
                }
                Kind::Npcs => {
                    if let Some(id) = text(doc, "stat_ref").strip_prefix("monster:") {
                        let id = remap.get(id).cloned().unwrap_or_else(|| id.to_string());
                        doc["stat_ref"] = json!(format!("monster:{id}"));
                    }
                }
                _ => {}
            }
        }
    }
}

// --------------------------------------------------------------------------- Python-compatible helpers

/// Python's truthiness of a JSON value.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Python's `str()` of a JSON scalar.
fn python_str(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Null => "None".into(),
        other => other.to_string(),
    }
}

/// Python's `repr()` of a JSON scalar, for messages.
fn python_repr(value: &Value) -> String {
    match value {
        Value::String(s) => format!("'{s}'"),
        other => python_str(other),
    }
}

/// Python's `int()`: whole numbers, and strings of digits.
fn python_int(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f.trunc() as i64)),
        Value::Bool(b) => Some(*b as i64),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Python's `str.title()`: capitalise the first letter of every run of letters.
fn title_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut previous_is_letter = false;
    for ch in text.chars() {
        if previous_is_letter {
            out.extend(ch.to_lowercase())
        } else {
            out.extend(ch.to_uppercase())
        }
        previous_is_letter = ch.is_alphabetic();
    }
    out
}

fn signed(n: i64) -> String {
    if n >= 0 { format!("+{n}") } else { n.to_string() }
}

// --------------------------------------------------------------------------- Markdown

/// A Markdown document as a handbook entry. The title is the first `# ` heading if the
/// text starts with one, else the file name.
pub fn markdown_to_handbook(text: &str, filename: &str) -> Value {
    let body = text.replace("\r\n", "\n");
    let (title, rest) = match leading_heading(&body) {
        Some((title, rest)) => (title, rest),
        None => (String::new(), body.as_str()),
    };
    let title = if title.is_empty() { title_from_filename(filename) } else { title };
    json!({"title": title, "body": rest.trim(), "category": "Imported"})
}

/// `# Title` (after optional blank space) on a line ending in a newline. Returns the
/// title and the text after that line.
fn leading_heading(text: &str) -> Option<(String, &str)> {
    let rest = text.trim_start().strip_prefix('#')?;
    let after_hash = rest.trim_start();
    if after_hash.len() == rest.len() {
        return None; // "##" or "#word": not a top-level heading
    }
    let (line, after) = after_hash.split_once('\n')?;
    let title = line.trim();
    if title.is_empty() {
        return None;
    }
    Some((title.to_string(), after))
}

fn title_from_filename(filename: &str) -> String {
    let lower = filename.to_lowercase();
    let stem = [".markdown", ".md", ".txt"]
        .iter()
        .find(|ext| lower.ends_with(*ext))
        .map_or(filename, |ext| &filename[..filename.len() - ext.len()]);
    let title = stem.replace('_', " ").trim().to_string();
    if title.is_empty() { "Imported".into() } else { title }
}

/// A character sheet as Markdown.
pub fn character_markdown(c: &Character, edition: Edition) -> String {
    let d = rules::derive(c, edition);
    let classes: Vec<String> = c
        .classes
        .iter()
        .map(|k| {
            let sub = if k.subclass.is_empty() { String::new() } else { format!(" ({})", k.subclass) };
            format!("{}{sub} {}", k.name, k.level)
        })
        .collect();
    let classes = classes.join(" / ");
    let heading = [c.race.as_str(), &classes].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join(" ");
    let mut subtitle = format!("*{heading}*");
    if !c.background.is_empty() {
        subtitle += &format!(" — {}", c.background);
    }
    if !c.alignment.is_empty() {
        subtitle += &format!(", {}", c.alignment);
    }
    let player = if c.player.is_empty() { "—" } else { &c.player };
    let temp = if c.hp.temp != 0 { format!(" (+{} temp)", c.hp.temp) } else { String::new() };
    let score = |a: &str| c.abilities.get(a).copied().unwrap_or(10);
    let name_of = |a: &str| ABILITY_NAMES.iter().find(|(k, _)| *k == a).map_or(a, |(_, n)| *n).to_string();

    let mut out = vec![
        format!("# {}", c.name),
        String::new(),
        subtitle,
        String::new(),
        format!(
            "**Player:** {player} · **Level** {} · **XP** {} · **Proficiency Bonus** {}",
            d.level,
            c.xp,
            signed(d.proficiency_bonus)
        ),
        String::new(),
        format!(
            "**AC** {} · **Initiative** {} · **Speed** {} ft. · **HP** {}/{}{temp} · **Hit Dice** {} of {}",
            c.ac,
            signed(d.initiative),
            c.speed,
            c.hp.current,
            c.hp.max,
            c.hit_dice.remaining,
            c.hit_dice.total
        ),
        String::new(),
        format!("| {} |", ABILITIES.iter().map(|a| a.to_uppercase()).collect::<Vec<_>>().join(" | ")),
        format!("|{}", ":---:|".repeat(6)),
        format!(
            "| {} |",
            ABILITIES
                .iter()
                .map(|a| format!("{} ({})", score(a), signed(d.modifiers[*a])))
                .collect::<Vec<_>>()
                .join(" | ")
        ),
        String::new(),
        format!(
            "**Saving Throws:** {}",
            ABILITIES
                .iter()
                .map(|a| {
                    let star = if c.save_proficiencies.iter().any(|s| s == a) { "*" } else { "" };
                    format!("{} {}{star}", name_of(a), signed(d.saves[*a]))
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        String::new(),
        format!(
            "**Skills:** {}",
            d.skills
                .iter()
                .map(|(skill, bonus)| {
                    let mark = match c.skills.get(skill).map(|p| p.as_str()) {
                        Some("proficient") => "*",
                        Some("expertise") => "**",
                        Some("half") => "½",
                        _ => "",
                    };
                    format!("{} {}{mark}", title_case(&skill.replace('-', " ")), signed(*bonus))
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        String::new(),
        format!(
            "**Passive Perception** {} · **Passive Investigation** {} · **Passive Insight** {}",
            d.passive_perception, d.passive_investigation, d.passive_insight
        ),
    ];

    let p = &c.proficiencies;
    let profs: Vec<String> =
        [("armor", &p.armor), ("weapons", &p.weapons), ("tools", &p.tools), ("languages", &p.languages)]
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| format!("**{}:** {v}  ", title_case(k)))
            .collect();
    if !profs.is_empty() {
        out.extend(["".into(), "## Proficiencies".into(), "".into()]);
        out.extend(profs);
    }
    if !c.attacks.is_empty() {
        out.extend(["", "## Attacks", "", "| Name | Bonus | Damage | Notes |", "|---|---|---|---|"].map(String::from));
        out.extend(c.attacks.iter().map(|a| format!("| {} | {} | {} | {} |", a.name, a.bonus, a.damage, a.notes)));
    }
    let sc = &c.spellcasting;
    if !sc.ability.is_empty() || !sc.spells.is_empty() {
        out.extend(["", "## Spellcasting", ""].map(String::from));
        if let Some(spell) = &d.spell {
            out.push(format!(
                "**Ability** {} · **Save DC** {} · **Attack** {}",
                name_of(&sc.ability),
                spell.save_dc,
                signed(spell.attack_bonus)
            ));
        }
        let mut levels: Vec<(&String, &crate::schemas::SpellSlot)> = sc.slots.iter().collect();
        levels.sort_by(|a, b| a.0.cmp(b.0));
        let slots: Vec<String> = levels
            .iter()
            .filter(|(_, s)| s.max != 0)
            .map(|(lvl, s)| format!("{lvl}: {}/{}", s.max - s.used, s.max))
            .collect();
        if !slots.is_empty() {
            out.push(String::new());
            out.push(format!("**Slots** {}", slots.join(" · ")));
        }
        let mut by_level: std::collections::BTreeMap<i64, Vec<String>> = Default::default();
        for spell in &sc.spells {
            let mark = if spell.prepared { " ◆" } else { "" };
            by_level.entry(spell.level).or_default().push(format!("{}{mark}", spell.name));
        }
        for (level, names) in by_level {
            let label = if level == 0 { "Cantrips".to_string() } else { format!("Level {level}") };
            out.push(String::new());
            out.push(format!("**{label}:** {}", names.join(", ")));
        }
    }
    if !c.resources.is_empty() {
        out.extend(["", "## Resources", ""].map(String::from));
        out.extend(
            c.resources.iter().map(|r| format!("- **{}:** {}/{} ({})", r.name, r.current, r.max, r.reset.as_str())),
        );
    }
    if !c.features.is_empty() {
        out.push(String::new());
        out.push("## Features & Traits".into());
        for f in &c.features {
            let source = if f.source.is_empty() { String::new() } else { format!(" *({})*", f.source) };
            out.push(String::new());
            out.push(format!("**{}**{source}. {}", f.name, f.description));
        }
    }
    let cur = &c.currency;
    let coins: Vec<String> = [("cp", cur.cp), ("sp", cur.sp), ("ep", cur.ep), ("gp", cur.gp), ("pp", cur.pp)]
        .iter()
        .filter(|(_, v)| *v != 0)
        .map(|(k, v)| format!("{v} {k}"))
        .collect();
    if !c.inventory.is_empty() || !coins.is_empty() {
        out.extend(["", "## Equipment", ""].map(String::from));
        out.extend(c.inventory.iter().map(|i| {
            let qty = if i.qty != 1 { format!(" ×{}", i.qty) } else { String::new() };
            let equipped = if i.equipped { " (equipped)" } else { "" };
            format!("- {}{qty}{equipped}", i.name)
        }));
        if !coins.is_empty() {
            out.push(String::new());
            out.push(format!("**Coins:** {}", coins.join(", ")));
        }
    }
    let pers = &c.personality;
    let traits = [("traits", &pers.traits), ("ideals", &pers.ideals), ("bonds", &pers.bonds), ("flaws", &pers.flaws)];
    if traits.iter().any(|(_, v)| !v.is_empty()) {
        out.extend(["", "## Personality", ""].map(String::from));
        out.extend(traits.iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| format!("**{}:** {v}  ", title_case(k))));
    }
    for (value, label) in [(&c.appearance, "Appearance"), (&c.backstory, "Backstory"), (&c.notes, "Notes")] {
        if !value.is_empty() {
            out.extend([String::new(), format!("## {label}"), String::new(), value.clone()]);
        }
    }
    out.join("\n") + "\n"
}

/// A session's notes, log and loot as Markdown.
pub fn session_markdown(s: &Session, campaign_name: &str) -> String {
    let mut out = vec![format!("# Session {}: {}", s.number, s.title), String::new()];
    let meta: Vec<&str> = [campaign_name, &s.date, s.status.as_str()].into_iter().filter(|x| !x.is_empty()).collect();
    if !meta.is_empty() {
        out.extend([meta.join(" · "), String::new()]);
    }
    if !s.recap.is_empty() {
        out.extend(["## Recap".into(), String::new(), s.recap.clone(), String::new()]);
    }
    if !s.prep.is_empty() {
        out.extend(["## Prep".into(), String::new(), s.prep.clone(), String::new()]);
    }
    if !s.log.is_empty() {
        out.extend(["## Log".into(), String::new()]);
        out.extend(s.log.iter().map(|e| {
            let time: String = e.time.chars().skip(11).take(5).collect();
            format!("- `{time}` **{}** {}", e.kind, e.text)
        }));
        out.push(String::new());
    }
    if !s.loot.is_empty() {
        out.extend(["## Loot".into(), String::new()]);
        out.extend(s.loot.iter().map(|item| {
            let qty = if item.qty != 1 { format!(" ×{}", item.qty) } else { String::new() };
            let to = if item.assigned_to.is_empty() { String::new() } else { format!(" → {}", item.assigned_to) };
            format!("- {}{qty}{to}", item.name)
        }));
        out.push(String::new());
    }
    if s.xp_awarded != 0 {
        out.extend([format!("**XP awarded:** {}", s.xp_awarded), String::new()]);
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_and_titles() {
        let entry = markdown_to_handbook("# Hello\r\n\r\nworld", "x.md");
        assert_eq!((entry["title"].as_str(), entry["body"].as_str()), (Some("Hello"), Some("world")));
        assert_eq!(markdown_to_handbook("## Sub\nbody\n", "house_rules.MD")["title"], "house rules");
        assert_eq!(markdown_to_handbook("# Only a title", "")["title"], "Imported");
        assert_eq!(markdown_to_handbook("#\nNext line\nbody", "")["title"], "Next line");
        assert_eq!(title_case("sleight of hand"), "Sleight Of Hand");
        assert_eq!(title_case("o'brien"), "O'Brien");
    }
}
