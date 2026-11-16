//! The bundled System Reference Document, per rules edition: loading and search.
//!
//! `2014` is SRD 5.1 and `2024` is SRD 5.2.1. Both are compiled into the binary and
//! decompressed the first time they're used.

use std::collections::HashMap;
use std::io::Read;
use std::sync::OnceLock;

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::schemas::Edition;

const SRD_2014: &[u8] = include_bytes!("../../../src/questodian/data/srd.json.gz");
const SRD_2024: &[u8] = include_bytes!("../../../src/questodian/data/srd-2024.json.gz");

/// Categories in display order, with their labels.
const CATEGORY_LABELS: [(&str, &str); 20] = [
    ("rules", "Rules"),
    ("conditions", "Conditions"),
    ("abilities", "Ability Scores"),
    ("skills", "Skills"),
    ("classes", "Classes"),
    ("races", "Races"),
    ("species", "Species"),
    ("backgrounds", "Backgrounds"),
    ("feats", "Feats"),
    ("spells", "Spells"),
    ("monsters", "Monsters"),
    ("equipment", "Equipment"),
    ("magic-items", "Magic Items"),
    ("weapon-mastery", "Weapon Mastery"),
    ("weapon-properties", "Weapon Properties"),
    ("poisons", "Poisons"),
    ("damage-types", "Damage Types"),
    ("magic-schools", "Schools of Magic"),
    ("alignments", "Alignments"),
    ("languages", "Languages"),
];

pub fn edition_label(edition: Edition) -> &'static str {
    match edition {
        Edition::E2014 => "5e (2014)",
        Edition::E2024 => "5.5e (2024)",
    }
}

/// An edition named in a request: empty means the default (2014).
pub fn parse_edition(text: &str) -> Result<Edition, String> {
    match text {
        "" | "2014" => Ok(Edition::E2014),
        "2024" => Ok(Edition::E2024),
        other => Err(format!("unknown rules edition '{other}' (use 2014 or 2024)")),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub category: String,
    pub name: String,
    pub summary: String,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Structured fields, e.g. a spell's level or a monster's AC.
    pub data: Map<String, Value>,
}

/// One edition's reference, with what search needs worked out in advance.
pub struct Srd {
    pub meta: Map<String, Value>,
    pub entries: Vec<Entry>,
    by_id: HashMap<String, usize>,
    /// Lowercased name, and name + summary + tags + body, for each entry.
    search_text: Vec<(String, String)>,
}

#[derive(Deserialize)]
struct File {
    meta: Map<String, Value>,
    entries: Vec<Entry>,
}

impl Srd {
    fn load(compressed: &[u8]) -> Srd {
        let mut json = String::new();
        GzDecoder::new(compressed).read_to_string(&mut json).expect("the bundled SRD is valid gzip");
        let file: File = serde_json::from_str(&json).expect("the bundled SRD is valid JSON");
        let by_id = file.entries.iter().enumerate().map(|(i, e)| (e.id.clone(), i)).collect();
        let search_text = file
            .entries
            .iter()
            .map(|e| {
                let all = [e.name.as_str(), &e.summary, &e.tags.join(" "), e.body.as_deref().unwrap_or("")].join(" ");
                (e.name.to_lowercase(), all.to_lowercase())
            })
            .collect();
        Srd { meta: file.meta, entries: file.entries, by_id, search_text }
    }

    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.by_id.get(id).map(|&i| &self.entries[i])
    }
}

/// The reference for an edition, loaded on first use.
pub fn srd(edition: Edition) -> &'static Srd {
    static E2014: OnceLock<Srd> = OnceLock::new();
    static E2024: OnceLock<Srd> = OnceLock::new();
    match edition {
        Edition::E2014 => E2014.get_or_init(|| Srd::load(SRD_2014)),
        Edition::E2024 => E2024.get_or_init(|| Srd::load(SRD_2024)),
    }
}

/// The SRD's title and licence, plus which edition it is.
pub fn meta(edition: Edition) -> Value {
    let mut meta = srd(edition).meta.clone();
    meta.insert("edition".into(), json!(edition.as_str()));
    meta.insert("edition_label".into(), json!(edition_label(edition)));
    Value::Object(meta)
}

/// Categories present in this edition, in display order, with entry counts.
pub fn categories(edition: Edition) -> Vec<Value> {
    let entries = &srd(edition).entries;
    CATEGORY_LABELS
        .iter()
        .filter_map(|&(id, label)| {
            let count = entries.iter().filter(|e| e.category == id).count();
            (count > 0).then(|| json!({"id": id, "label": label, "count": count}))
        })
        .collect()
}

#[derive(Debug, Serialize)]
pub struct SearchResults {
    pub total: usize,
    pub results: Vec<Entry>,
}

/// Search one edition. Results rank by where the words match: exact name, name prefix,
/// name contains, all words in the name, then all words anywhere; ties sort by name.
///
/// `filters` match an entry's structured `data`, e.g. `level=3` for spells or
/// `cr_label=1/4` for monsters. Comparison ignores case, and a list field matches if
/// any item does.
pub fn search(
    edition: Edition,
    query: &str,
    category: &str,
    filters: &[(String, String)],
    limit: usize,
    offset: usize,
) -> SearchResults {
    let srd = srd(edition);
    let query = query.trim().to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    let mut ranked: Vec<(u8, &str, &Entry)> = Vec::new();
    for (entry, (name, text)) in srd.entries.iter().zip(&srd.search_text) {
        if !category.is_empty() && entry.category != category {
            continue;
        }
        if !matches_filters(&entry.data, filters) {
            continue;
        }
        let rank = if words.is_empty() {
            9
        } else if *name == query {
            0
        } else if name.starts_with(&query) {
            1
        } else if name.contains(&query) {
            2
        } else if words.iter().all(|w| name.contains(w)) {
            3
        } else if words.iter().all(|w| text.contains(w)) {
            5
        } else {
            continue;
        };
        ranked.push((rank, name, entry));
    }
    ranked.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1))); // stable: equal names keep file order
    let total = ranked.len();
    let results =
        ranked.into_iter().skip(offset).take(limit).map(|(_, _, e)| Entry { body: None, ..e.clone() }).collect();
    SearchResults { total, results }
}

fn matches_filters(data: &Map<String, Value>, filters: &[(String, String)]) -> bool {
    filters.iter().filter(|(_, want)| !want.is_empty()).all(|(key, want)| {
        let want = want.to_lowercase();
        match data.get(key) {
            Some(Value::Array(items)) => items.iter().any(|item| python_str(item).to_lowercase() == want),
            have => python_str(have.unwrap_or(&Value::Null)).to_lowercase() == want,
        }
    })
}

/// A value as Python's `str()` writes it, so filters compare the same as before
/// (`True`, `None`, `0.5`).
fn python_str(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Null => "None".into(),
        other => other.to_string(),
    }
}

/// The full entry, body included.
pub fn get(edition: Edition, id: &str) -> Option<Entry> {
    srd(edition).get(id).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(results: &SearchResults) -> Vec<&str> {
        results.results.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn loads_both_editions() {
        assert_eq!(srd(Edition::E2014).meta["title"], "System Reference Document 5.1");
        assert!(srd(Edition::E2024).entries.len() > 100);
        assert_eq!(meta(Edition::E2024)["edition_label"], "5.5e (2024)");
    }

    #[test]
    fn exact_name_ranks_first() {
        let found = search(Edition::E2014, "Fireball", "", &[], 5, 0);
        assert_eq!(names(&found)[0], "Fireball");
        assert!(found.results[0].body.is_none());
        assert!(get(Edition::E2014, &found.results[0].id).unwrap().body.is_some());
    }

    #[test]
    fn filters_match_structured_data() {
        let filters = [("level".to_string(), "3".to_string())];
        let found = search(Edition::E2014, "", "spells", &filters, 500, 0);
        assert!(found.total > 10);
        assert!(found.results.iter().all(|e| e.data["level"] == 3));
    }

    #[test]
    fn paging() {
        let all = search(Edition::E2014, "", "conditions", &[], 500, 0);
        let page = search(Edition::E2014, "", "conditions", &[], 2, 1);
        assert_eq!(page.total, all.total);
        assert_eq!(names(&page), names(&all)[1..3]);
    }

    #[test]
    fn editions() {
        assert_eq!(parse_edition(""), Ok(Edition::E2014));
        assert!(parse_edition("2015").unwrap_err().contains("'2015'"));
        let cats: Vec<String> =
            categories(Edition::E2024).iter().map(|c| c["id"].as_str().unwrap().to_string()).collect();
        assert!(cats.contains(&"species".to_string()) && !cats.contains(&"races".to_string()));
    }
}
