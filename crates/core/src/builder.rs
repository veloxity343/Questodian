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
                Ok(Some(Value::Object(entry)))
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
    let bundled_value = &bundled()[key];
    let same_shape = matches!(
        (bundled_value, &value),
        (Value::Object(_), Value::Object(_))
            | (Value::Array(_), Value::Array(_))
            | (Value::Number(_), Value::Number(_))
            | (Value::String(_), Value::String(_))
    );
    if !same_shape || (bundled_value.is_i64() && !value.is_i64()) {
        return Err(invalid(format!("{key} must be the same kind of value as the bundled one")));
    }
    Ok(Some(value))
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
        assert_eq!(check("feats", "Lucky", Some(json!({"name": "x"}))).unwrap(), Some(json!({"name": "Lucky"})));
        assert_eq!(check("classes", "Wizard", None).unwrap(), None);
        let sent_back = json!({"name": "Lucky", "source": "edited"});
        assert_eq!(check("feats", "Lucky", Some(sent_back)).unwrap(), Some(json!({"name": "Lucky"})));
    }
}
