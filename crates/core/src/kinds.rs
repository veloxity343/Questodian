//! The eleven kinds of record, described in one place: table, names, ordering,
//! filters, links and schema.

use serde::Serialize;
use serde_json::Value;

use crate::schemas::{self, Check, Lenient};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Campaigns,
    Sessions,
    Characters,
    Handbook,
    Scenes,
    Npcs,
    Monsters,
    Notes,
    Encounters,
    Trackers,
    Handouts,
}

impl Kind {
    /// Every kind, ordered so that records come after the records they link to
    /// (imports rely on this).
    pub const ALL: [Kind; 11] = [
        Kind::Campaigns,
        Kind::Sessions,
        Kind::Characters,
        Kind::Handbook,
        Kind::Scenes,
        Kind::Npcs,
        Kind::Monsters,
        Kind::Notes,
        Kind::Encounters,
        Kind::Trackers,
        Kind::Handouts,
    ];

    /// The plural name used in URLs, exports and the table name.
    pub fn plural(self) -> &'static str {
        match self {
            Kind::Campaigns => "campaigns",
            Kind::Sessions => "sessions",
            Kind::Characters => "characters",
            Kind::Handbook => "handbook",
            Kind::Scenes => "scenes",
            Kind::Npcs => "npcs",
            Kind::Monsters => "monsters",
            Kind::Notes => "notes",
            Kind::Encounters => "encounters",
            Kind::Trackers => "trackers",
            Kind::Handouts => "handouts",
        }
    }

    pub fn table(self) -> &'static str {
        self.plural()
    }

    /// The singular name used in messages and single-record exports.
    pub fn singular(self) -> &'static str {
        match self {
            Kind::Campaigns => "campaign",
            Kind::Sessions => "session",
            Kind::Characters => "character",
            Kind::Handbook => "handbook",
            Kind::Scenes => "scene",
            Kind::Npcs => "npc",
            Kind::Monsters => "monster",
            Kind::Notes => "note",
            Kind::Encounters => "encounter",
            Kind::Trackers => "tracker",
            Kind::Handouts => "handout",
        }
    }

    pub fn from_plural(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.plural() == name)
    }

    pub fn from_singular(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.singular() == name)
    }

    /// `ORDER BY` for lists. `lower()` is SQLite's, which folds ASCII only, as before.
    pub fn order_by(self) -> &'static str {
        match self {
            Kind::Sessions => "campaign_id, number",
            Kind::Scenes => "session_id, position, lower(title)",
            Kind::Handbook => "lower(title)",
            Kind::Notes => "pinned DESC, updated DESC",
            Kind::Encounters => "created",
            Kind::Handouts => "lower(folder), lower(title)",
            _ => "lower(name)",
        }
    }

    /// Validate a record and fill in defaults, as the API returns it.
    pub fn normalize(self, value: Value) -> Result<Value, String> {
        fn via<T: Lenient + Check + Serialize>(value: Value) -> Result<Value, String> {
            let doc: T = schemas::validate(value)?;
            Ok(serde_json::to_value(doc).expect("schemas serialize"))
        }
        if !value.is_object() {
            return Err(format!("{} must be an object", self.singular()));
        }
        match self {
            Kind::Campaigns => via::<schemas::Campaign>(value),
            Kind::Sessions => via::<schemas::Session>(value),
            Kind::Characters => via::<schemas::Character>(value),
            Kind::Handbook => via::<schemas::HandbookEntry>(value),
            Kind::Scenes => via::<schemas::Scene>(value),
            Kind::Npcs => via::<schemas::Npc>(value),
            Kind::Monsters => via::<schemas::Monster>(value),
            Kind::Notes => via::<schemas::Note>(value),
            Kind::Encounters => via::<schemas::Encounter>(value),
            Kind::Trackers => via::<schemas::Tracker>(value),
            Kind::Handouts => via::<schemas::Handout>(value),
        }
    }
}

/// Link fields (stored as NULL when empty) and the kind each points to.
pub const LINKS: [(&str, Kind); 3] =
    [("campaign_id", Kind::Campaigns), ("session_id", Kind::Sessions), ("scene_id", Kind::Scenes)];

/// Fields a list can be filtered on (ignored for kinds that don't have them).
pub const FILTERS: [&str; 5] = ["campaign_id", "session_id", "scene_id", "kind", "folder"];

/// What a column holds, from its declared SQL type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Text,
    Integer,
    Boolean,
    /// JSON stored as text.
    Json,
    /// An id of another record, or NULL.
    Link,
}

impl ColumnKind {
    pub fn from_declared(name: &str, declared: &str) -> ColumnKind {
        if LINKS.iter().any(|(field, _)| *field == name) {
            return ColumnKind::Link;
        }
        match declared.to_uppercase().as_str() {
            "JSON" => ColumnKind::Json,
            "BOOLEAN" => ColumnKind::Boolean,
            "INTEGER" => ColumnKind::Integer,
            _ => ColumnKind::Text,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub kind: ColumnKind,
}
