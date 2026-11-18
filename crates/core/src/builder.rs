//! The character builder's rules data (2024 rules): classes, species, backgrounds, feats,
//! equipment and the ability score methods, bundled in `data/rules-2024.json.gz`, with the
//! user's own edits laid over the top.
//!
//! An edit is stored in `rule_overrides` as (kind, key, data):
//! - a named entry (`classes`, `species`, `backgrounds`, `feats`, `armor`, `weapons`,
//!   `masteries`, `items`) keyed by its name. `data` replaces the bundled entry, or adds a
//!   new one; `null` hides the bundled entry.
//! - a setting (`settings`) keyed by its field (`point_buy`, `standard_array`, `skills`...).
//!   `data` replaces the bundled value.
//!
//! Removing an edit brings back the bundled entry.

use std::io::Read;
use std::sync::OnceLock;

use flate2::read::GzDecoder;
use serde_json::{Map, Value, json};

use crate::error::{Result, invalid};

const RULES_2024: &[u8] = include_bytes!("../../../data/rules-2024.json.gz");

/// The lists of named entries the user can edit, add to or hide.
pub const ENTRY_KINDS: [&str; 8] =
    ["classes", "species", "backgrounds", "feats", "armor", "weapons", "masteries", "items"];

/// The other values the user can change.
pub const SETTINGS: [&str; 8] =
    ["skills", "ability_uses", "point_buy", "standard_array", "rolled", "score_cap", "alignments", "fighting_styles"];

/// One stored edit.
#[derive(Debug, Clone, PartialEq)]
pub struct Override {
    pub kind: String,
    pub key: String,
    /// `None` hides a bundled entry.
    pub data: Option<Value>,
    pub updated: String,
}

/// The bundled data, loaded on first use.
pub fn bundled() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let mut json = String::new();
        GzDecoder::new(RULES_2024).read_to_string(&mut json).expect("the bundled rules are valid gzip");
        serde_json::from_str(&json).expect("the bundled rules are valid JSON")
    })
}

fn name_of(entry: &Value) -> &str {
    entry.get("name").and_then(Value::as_str).unwrap_or("")
}

/// Check an edit before it's stored, and return it tidied (an entry's name is its key).
pub fn check(kind: &str, key: &str, data: Option<Value>) -> Result<Option<Value>> {
    let key = key.trim();
    if key.is_empty() {
        return Err(invalid("a name is needed"));
    }
    if ENTRY_KINDS.contains(&kind) {
        return match data {
            None => {
                let bundled_entry = bundled()[kind].as_array().into_iter().flatten().any(|e| name_of(e) == key);
                if bundled_entry {
                    Ok(None)
                } else {
                    Err(invalid(format!("{key} isn't bundled, so it can't be hidden")))
                }
            }
            Some(Value::Object(mut entry)) => {
                entry.insert("name".into(), json!(key));
                entry.remove("source"); // added by `merged`, not part of the entry
                let entry = Value::Object(entry);
                let examples: Vec<&Value> = bundled()[kind].as_array().into_iter().flatten().collect();
                fits(&entry, &examples, kind).map_err(|e| invalid(format!("{key}: {e}")))?;
                Ok(Some(entry))
            }
            Some(_) => Err(invalid(format!("{kind} entries are objects"))),
        };
    }
    if kind != "settings" || !SETTINGS.contains(&key) {
        return Err(invalid(format!("unknown rules data {kind}/{key}")));
    }
    let Some(value) = data else {
        return Err(invalid("settings can't be hidden; reset them instead"));
    };
    fits(&value, &[&bundled()[key]], key).map_err(|e| invalid(format!("{key}: {e}")))?;
    Ok(Some(value))
}

fn type_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "nothing",
        Value::Bool(_) => "yes/no",
        Value::Number(n) if n.is_i64() || n.is_u64() => "a whole number",
        Value::Number(_) => "a number",
        Value::String(_) => "text",
        Value::Array(_) => "a list",
        Value::Object(_) => "a group of fields",
    }
}

/// Fields whose keys are data (point costs by score, uses by level, gear options, skill
/// names, Expertise by level): any keys will do, as long as the values have the right shape.
const LOOKUPS: [&str; 5] = ["costs", "by_level", "starting_gear", "skills", "expertise"];
/// Fields an entry may leave out.
const OPTIONAL: [&str; 2] = ["srd", "source"];

/// Whether an edited value has the shape of the bundled ones it stands in for, so the
/// builder can read it: the same kinds of value, and every field the bundled ones all have.
/// Extra fields are fine. A whole number can stand in for a number.
fn fits(value: &Value, examples: &[&Value], field: &str) -> std::result::Result<(), String> {
    if examples.is_empty() {
        return Ok(());
    }
    let mut allowed: Vec<&str> = examples.iter().map(|e| type_of(e)).collect();
    if allowed.contains(&"a number") {
        allowed.push("a whole number");
    }
    let found = type_of(value);
    if !allowed.contains(&found) {
        allowed.sort();
        allowed.dedup();
        return Err(format!("should be {}, not {found}", allowed.join(" or ")));
    }
    match value {
        Value::Object(fields) => {
            let objects: Vec<&Map<String, Value>> = examples.iter().filter_map(|e| e.as_object()).collect();
            if LOOKUPS.contains(&field) {
                let inner: Vec<&Value> = objects.iter().flat_map(|o| o.values()).collect();
                for (name, item) in fields {
                    fits(item, &inner, name).map_err(|e| format!("{name} {e}"))?;
                }
                return Ok(());
            }
            let Some(first) = objects.first() else { return Ok(()) };
            for name in first.keys().filter(|k| objects.iter().all(|o| o.contains_key(*k))) {
                let Some(item) = fields.get(name) else {
                    if OPTIONAL.contains(&name.as_str()) {
                        continue;
                    }
                    return Err(format!("{name} is missing"));
                };
                let inner: Vec<&Value> = objects.iter().map(|o| &o[name]).collect();
                fits(item, &inner, name).map_err(|e| format!("{name} {e}"))?;
            }
            Ok(())
        }
        Value::Array(items) => {
            let inner: Vec<&Value> = examples.iter().filter_map(|e| e.as_array()).flatten().collect();
            for (i, item) in items.iter().enumerate() {
                fits(item, &inner, field).map_err(|e| format!("item {}: {e}", i + 1))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Whether the character builder works out this character: it has a class from the rules data
/// (with the edits) and isn't edited by hand. Such a character follows the 2024 rules.
pub fn builds(build: &indexmap::IndexMap<String, Value>, overrides: &[Override]) -> bool {
    let Some(class) = build.get("class").and_then(Value::as_str).filter(|c| !c.is_empty()) else { return false };
    if build.get("manual").and_then(Value::as_bool) == Some(true) {
        return false;
    }
    merged(overrides)["classes"].as_array().into_iter().flatten().any(|c| name_of(c) == class)
}

/// The bundled data with the edits applied. Each named entry gets `"source"`:
/// `"bundled"`, `"edited"` or `"added"`; `"hidden"` lists hidden entries and
/// `"edited_settings"` the changed settings, so the UI can offer to restore them.
pub fn merged(overrides: &[Override]) -> Value {
    let mut out = bundled().as_object().cloned().unwrap_or_default();
    let mut hidden: Map<String, Value> = Map::new();
    let mut edited_settings = Vec::new();
    for kind in ENTRY_KINDS {
        let mut entries: Vec<Value> = out
            .get(kind)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|mut e| {
                e["source"] = json!("bundled");
                e
            })
            .collect();
        let mut hidden_here = Vec::new();
        for o in overrides.iter().filter(|o| o.kind == kind) {
            let at = entries.iter().position(|e| name_of(e) == o.key);
            match (&o.data, at) {
                (None, Some(i)) => {
                    entries.remove(i);
                    hidden_here.push(json!(o.key));
                }
                (None, None) => {}
                (Some(data), Some(i)) => {
                    entries[i] = data.clone();
                    entries[i]["source"] = json!("edited");
                }
                (Some(data), None) => {
                    let mut entry = data.clone();
                    entry["source"] = json!("added");
                    entries.push(entry);
                }
            }
        }
        entries.sort_by_key(|e| name_of(e).to_lowercase());
        out.insert(kind.into(), Value::Array(entries));
        if !hidden_here.is_empty() {
            hidden.insert(kind.into(), Value::Array(hidden_here));
        }
    }
    for o in overrides.iter().filter(|o| o.kind == "settings") {
        if let Some(data) = &o.data
            && SETTINGS.contains(&o.key.as_str())
        {
            out.insert(o.key.clone(), data.clone());
            edited_settings.push(json!(o.key));
        }
    }
    out.insert("hidden".into(), Value::Object(hidden));
    out.insert("edited_settings".into(), Value::Array(edited_settings));
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(kind: &str, key: &str, data: Option<Value>) -> Override {
        Override { kind: kind.into(), key: key.into(), data, updated: String::new() }
    }

    #[test]
    fn bundled_data_has_every_kind() {
        let data = bundled();
        for kind in ENTRY_KINDS {
            assert!(!data[kind].as_array().unwrap().is_empty(), "{kind}");
        }
        for key in SETTINGS {
            assert!(!data[key].is_null(), "{key}");
        }
    }

    #[test]
    fn every_bundled_entry_and_setting_passes_its_own_check() {
        for kind in ENTRY_KINDS {
            for entry in bundled()[kind].as_array().unwrap() {
                let name = entry["name"].as_str().unwrap();
                check(kind, name, Some(entry.clone())).unwrap_or_else(|e| panic!("{kind}/{name}: {e}"));
            }
        }
        for key in SETTINGS {
            check("settings", key, Some(bundled()[key].clone())).unwrap_or_else(|e| panic!("{key}: {e}"));
        }
    }

    #[test]
    fn edits_must_keep_the_shape_the_builder_reads() {
        let fighter = bundled()["classes"].as_array().unwrap().iter().find(|c| c["name"] == "Fighter").unwrap().clone();
        assert!(check("classes", "Fighter", Some(fighter.clone())).is_ok(), "the bundled entry itself");
        let with = |change: &dyn Fn(&mut Value)| {
            let mut entry = fighter.clone();
            change(&mut entry);
            check("classes", "Fighter", Some(entry))
        };
        assert!(with(&|e| e["hit_die"] = json!(12)).is_ok());
        assert!(with(&|e| e["spell_ability"] = json!("int")).is_ok(), "other classes have one");
        let error = with(&|e| e["levels"] = json!({})).unwrap_err().to_string();
        assert!(error.contains("levels should be a list"), "{error}");
        assert!(with(&|e| e.as_object_mut().unwrap().remove("levels").map(|_| ()).unwrap()).is_err());
        assert!(with(&|e| e["levels"][0]["slots"] = json!("none")).is_err());
        assert!(check("settings", "standard_array", Some(json!(["a"]))).is_err());
        assert!(
            check("settings", "point_buy", Some(json!({"budget": 30, "min": 8, "max": 15}))).is_err(),
            "costs missing"
        );
        assert!(
            check("settings", "point_buy", Some(json!({"budget": 30, "min": 8, "max": 15, "costs": {"8": 0}}))).is_ok()
        );
        let pack = json!({"weight": 30, "cost": "5 GP", "kind": "Gear", "note": ""});
        assert!(check("items", "Big Pack", Some(pack)).is_ok(), "a whole number for a weight");
    }

    #[test]
    fn edits_replace_add_and_hide() {
        let merged = merged(&[
            edit("classes", "Fighter", Some(json!({"name": "Fighter", "hit_die": 12}))),
            edit("classes", "Gunslinger", Some(json!({"name": "Gunslinger", "hit_die": 8}))),
            edit("classes", "Wizard", None),
            edit("settings", "score_cap", Some(json!(22))),
        ]);
        let classes = merged["classes"].as_array().unwrap();
        let find = |name: &str| classes.iter().find(|c| c["name"] == name).cloned();
        assert_eq!(find("Fighter").unwrap(), json!({"name": "Fighter", "hit_die": 12, "source": "edited"}));
        assert_eq!(find("Gunslinger").unwrap()["source"], "added");
        assert_eq!(find("Bard").unwrap()["source"], "bundled");
        assert!(find("Wizard").is_none());
        assert_eq!(merged["hidden"], json!({"classes": ["Wizard"]}));
        assert_eq!(merged["score_cap"], 22);
        assert_eq!(merged["edited_settings"], json!(["score_cap"]));
    }

    #[test]
    fn checks_refuse_nonsense() {
        assert!(check("classes", " ", Some(json!({}))).is_err());
        assert!(check("classes", "Fighter", Some(json!("text"))).is_err());
        assert!(check("classes", "Gunslinger", None).is_err(), "only bundled entries hide");
        assert!(check("settings", "score_cap", Some(json!("high"))).is_err());
        assert!(check("settings", "score_cap", Some(json!(20.5))).is_err(), "a whole number");
        assert!(check("settings", "nonsense", Some(json!(1))).is_err());
        assert!(check("spells", "Fireball", Some(json!({}))).is_err());
        let lucky = json!({"name": "x", "type": "Origin", "text": "Reroll.", "srd": false});
        assert_eq!(check("feats", "Lucky", Some(lucky)).unwrap().unwrap()["name"], "Lucky");
        assert_eq!(check("classes", "Wizard", None).unwrap(), None);
        let sent_back = json!({"name": "Lucky", "type": "Origin", "text": "Reroll.", "source": "edited"});
        assert!(check("feats", "Lucky", Some(sent_back)).unwrap().unwrap().get("source").is_none());
    }
}
