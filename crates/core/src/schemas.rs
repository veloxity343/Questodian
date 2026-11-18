//! The shape of every record and request body.
//!
//! Every field has a default, so partial input (a new record with just a name, or an
//! older export missing newer fields) becomes a complete record. Unknown keys are
//! ignored. Input is converted the way the first (Python) version converted it with
//! Pydantic, so its exports and stored records still load: `"300"` is a valid whole
//! number, `"yes"` a valid true, and `null` a valid empty string. See [`Lenient`].

use indexmap::IndexMap;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::rules::ABILITIES;

// --------------------------------------------------------------------------- lenient input

/// Conversion from any JSON value, with the same leniency as Pydantic's default mode.
pub trait Lenient: Sized {
    fn from_json(value: Value) -> Result<Self, String>;
}

/// `deserialize_with` helper: read a field leniently.
pub fn lenient<'de, D: Deserializer<'de>, T: Lenient>(d: D) -> Result<T, D::Error> {
    T::from_json(Value::deserialize(d)?).map_err(D::Error::custom)
}

fn describe(value: &Value) -> String {
    let text = value.to_string();
    if text.chars().count() > 40 { format!("{}…", text.chars().take(40).collect::<String>()) } else { text }
}

/// Text: a string, or `null` for an empty one.
impl Lenient for String {
    fn from_json(value: Value) -> Result<Self, String> {
        match value {
            Value::String(s) => Ok(s),
            Value::Null => Ok(String::new()),
            other => Err(format!("expected text, got {}", describe(&other))),
        }
    }
}

/// Whole numbers: integers, `true`/`false` (1/0), whole decimals (`3.0`) and
/// strings holding one of those (`"300"`, `" 7 "`, `"1.0"`).
impl Lenient for i64 {
    fn from_json(value: Value) -> Result<Self, String> {
        let whole = |f: f64| (f.fract() == 0.0 && f.abs() < 9.0e18).then_some(f as i64);
        let parsed = match &value {
            Value::Number(n) => n.as_i64().or_else(|| n.as_f64().and_then(whole)),
            Value::Bool(b) => Some(*b as i64),
            Value::String(s) => {
                let s = s.trim();
                s.parse::<i64>().ok().or_else(|| {
                    // "1.0" is accepted, "1e3" isn't (as in Pydantic).
                    let plain = s.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+'));
                    plain.then(|| s.parse::<f64>().ok().and_then(whole)).flatten()
                })
            }
            _ => None,
        };
        parsed.ok_or_else(|| format!("expected a whole number, got {}", describe(&value)))
    }
}

/// Numbers: anything numeric, `true`/`false`, and numeric strings (`"1.5"`, `"1e3"`).
impl Lenient for f64 {
    fn from_json(value: Value) -> Result<Self, String> {
        let parsed = match &value {
            Value::Number(n) => n.as_f64(),
            Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            Value::String(s) => s.trim().parse::<f64>().ok().filter(|f| f.is_finite()),
            _ => None,
        };
        parsed.ok_or_else(|| format!("expected a number, got {}", describe(&value)))
    }
}

/// True or false: booleans, 0 and 1, and the words Pydantic accepts
/// (`yes/no`, `true/false`, `on/off`, `y/n`, `t/f`, `1/0`, any case).
impl Lenient for bool {
    fn from_json(value: Value) -> Result<Self, String> {
        let parsed = match &value {
            Value::Bool(b) => Some(*b),
            Value::Number(n) => match n.as_f64() {
                Some(0.0) => Some(false),
                Some(1.0) => Some(true),
                _ => None,
            },
            Value::String(s) => match s.trim().to_lowercase().as_str() {
                "1" | "true" | "yes" | "on" | "y" | "t" => Some(true),
                "0" | "false" | "no" | "off" | "n" | "f" => Some(false),
                _ => None,
            },
            _ => None,
        };
        parsed.ok_or_else(|| format!("expected true or false, got {}", describe(&value)))
    }
}

/// Lists must be JSON arrays; each item is converted leniently.
impl<T: Lenient> Lenient for Vec<T> {
    fn from_json(value: Value) -> Result<Self, String> {
        match value {
            Value::Array(items) => items.into_iter().map(T::from_json).collect(),
            other => Err(format!("expected a list, got {}", describe(&other))),
        }
    }
}

/// Maps must be JSON objects; each value is converted leniently. Key order is kept.
impl<T: Lenient> Lenient for IndexMap<String, T> {
    fn from_json(value: Value) -> Result<Self, String> {
        match value {
            Value::Object(entries) => entries.into_iter().map(|(k, v)| Ok((k, T::from_json(v)?))).collect(),
            other => Err(format!("expected an object, got {}", describe(&other))),
        }
    }
}

/// Optional values: `null` is `None`.
impl<T: Lenient> Lenient for Option<T> {
    fn from_json(value: Value) -> Result<Self, String> {
        if value.is_null() { Ok(None) } else { T::from_json(value).map(Some) }
    }
}

/// Free-form JSON, kept as it is.
impl Lenient for Value {
    fn from_json(value: Value) -> Result<Self, String> {
        Ok(value)
    }
}

/// Tags: a list of strings, or one comma-separated string (`"undead, boss"`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Tags(pub Vec<String>);

impl Lenient for Tags {
    fn from_json(value: Value) -> Result<Self, String> {
        match value {
            Value::String(s) => {
                Ok(Tags(s.split(',').map(str::trim).filter(|t| !t.is_empty()).map(String::from).collect()))
            }
            other => Vec::<String>::from_json(other).map(Tags),
        }
    }
}

// --------------------------------------------------------------------------- declaration macros

/// A record or nested document: every field has a default and is read leniently.
macro_rules! schema {
    ($(#[$meta:meta])* pub struct $name:ident { $($(#[$field_meta:meta])* $field:ident : $ty:ty = $default:expr),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(default)]
        pub struct $name {
            $($(#[$field_meta])* #[serde(deserialize_with = "lenient")] pub $field: $ty,)*
        }

        impl Default for $name {
            fn default() -> Self {
                Self { $($field: $default,)* }
            }
        }

        impl Lenient for $name {
            fn from_json(value: Value) -> Result<Self, String> {
                if !value.is_object() {
                    return Err(format!("expected an object, got {}", describe(&value)));
                }
                serde_json::from_value(value).map_err(|e| e.to_string())
            }
        }
    };
}

/// A fixed set of words, e.g. a session status. The first one listed is the default.
macro_rules! choice {
    ($(#[$meta:meta])* pub enum $name:ident { $first:ident = $first_text:literal $(, $variant:ident = $text:literal)* $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
        pub enum $name {
            #[default]
            #[serde(rename = $first_text)]
            $first,
            $(#[serde(rename = $text)] $variant,)*
        }

        impl $name {
            pub const ALL: &[&str] = &[$first_text $(, $text)*];

            pub fn as_str(self) -> &'static str {
                match self {
                    Self::$first => $first_text,
                    $(Self::$variant => $text,)*
                }
            }
        }

        impl Lenient for $name {
            fn from_json(value: Value) -> Result<Self, String> {
                match &value {
                    Value::String(s) if s == $first_text => Ok(Self::$first),
                    $(Value::String(s) if s == $text => Ok(Self::$variant),)*
                    _ => Err(format!("expected one of {}, got {}", Self::ALL.join(", "), describe(&value))),
                }
            }
        }
    };
}

fn ability_scores() -> IndexMap<String, i64> {
    ABILITIES.iter().map(|a| (a.to_string(), 10)).collect()
}

fn text(s: &str) -> String {
    s.to_string()
}

// --------------------------------------------------------------------------- campaigns

choice! {
    /// Which rules a campaign uses: 2014 (SRD 5.1) or 2024 (SRD 5.2.1).
    pub enum Edition { E2014 = "2014", E2024 = "2024" }
}

schema! {
    pub struct Player {
        name: String = String::new(),
        contact: String = String::new(),
        character_id: String = String::new(),
        notes: String = String::new(),
    }
}

schema! {
    pub struct Campaign {
        id: String = String::new(),
        created: String = String::new(),
        updated: String = String::new(),
        name: String = text("New Campaign"),
        system: String = text("5e (SRD 5.1)"),
        edition: Edition = Edition::E2014,
        description: String = String::new(),
        setting: String = String::new(),
        players: Vec<Player> = Vec::new(),
        notes: String = String::new(),
    }
}

// --------------------------------------------------------------------------- sessions

choice! {
    pub enum CombatantKind { Monster = "monster", Pc = "pc", Npc = "npc" }
}

schema! {
    pub struct Combatant {
        id: String = String::new(),
        name: String = String::new(),
        kind: CombatantKind = CombatantKind::Monster,
        initiative: i64 = 0,
        init_bonus: i64 = 0,
        ac: i64 = 10,
        hp: i64 = 0,
        max_hp: i64 = 0,
        temp_hp: i64 = 0,
        conditions: Vec<String> = Vec::new(),
        notes: String = String::new(),
        character_id: String = String::new(),
        srd_id: String = String::new(),
        /// A homebrew stat block.
        monster_id: String = String::new(),
        npc_id: String = String::new(),
        xp: i64 = 0,
        /// Legendary monsters are worth more fought in their lair (0: no lair value).
        xp_in_lair: i64 = 0,
        in_lair: bool = false,
        /// Caught unawares when combat starts (cleared when it ends).
        surprised: bool = false,
        hidden: bool = false,
    }
}

schema! {
    pub struct LogEntry {
        id: String = String::new(),
        time: String = String::new(),
        kind: String = text("note"),
        text: String = String::new(),
    }
}

schema! {
    pub struct LootItem {
        name: String = String::new(),
        qty: i64 = 1,
        value: String = String::new(),
        assigned_to: String = String::new(),
    }
}

choice! {
    pub enum SessionStatus { Planned = "planned", Active = "active", Done = "done" }
}

schema! {
    pub struct Session {
        id: String = String::new(),
        created: String = String::new(),
        updated: String = String::new(),
        campaign_id: String = String::new(),
        number: i64 = 1,
        title: String = text("New Session"),
        date: String = String::new(),
        status: SessionStatus = SessionStatus::Planned,
        attendance: Vec<String> = Vec::new(),
        prep: String = String::new(),
        recap: String = String::new(),
        log: Vec<LogEntry> = Vec::new(),
        loot: Vec<LootItem> = Vec::new(),
        xp_awarded: i64 = 0,
    }
}

// --------------------------------------------------------------------------- characters

schema! {
    pub struct ClassLevel {
        name: String = String::new(),
        subclass: String = String::new(),
        level: i64 = 1,
    }
}

schema! {
    pub struct HitPoints {
        max: i64 = 10,
        current: i64 = 10,
        temp: i64 = 0,
    }
}

schema! {
    pub struct HitDice {
        total: String = text("1d10"),
        remaining: i64 = 1,
    }
}

schema! {
    pub struct DeathSaves {
        successes: i64 = 0,
        failures: i64 = 0,
    }
}

schema! {
    pub struct Proficiencies {
        armor: String = String::new(),
        weapons: String = String::new(),
        tools: String = String::new(),
        languages: String = text("Common"),
    }
}

schema! {
    pub struct Attack {
        name: String = String::new(),
        bonus: String = String::new(),
        damage: String = String::new(),
        notes: String = String::new(),
    }
}

schema! {
    pub struct SpellSlot {
        max: i64 = 0,
        used: i64 = 0,
    }
}

schema! {
    pub struct Spell {
        name: String = String::new(),
        level: i64 = 0,
        prepared: bool = false,
        notes: String = String::new(),
        srd_id: String = String::new(),
    }
}

schema! {
    pub struct Spellcasting {
        ability: String = String::new(),
        /// Keyed by spell level, "1" to "9".
        slots: IndexMap<String, SpellSlot> = IndexMap::new(),
        spells: Vec<Spell> = Vec::new(),
    }
}

choice! {
    /// When a class resource comes back. `ShortOne`: one use on a short rest, all on a long rest
    /// (Second Wind, Channel Divinity, Rage, Wild Shape in the 2024 rules).
    pub enum Reset { Long = "long", Short = "short", ShortOne = "short_one", Never = "none" }
}

schema! {
    pub struct Resource {
        name: String = String::new(),
        current: i64 = 0,
        max: i64 = 0,
        reset: Reset = Reset::Long,
    }
}

schema! {
    pub struct Item {
        name: String = String::new(),
        qty: i64 = 1,
        weight: f64 = 0.0,
        equipped: bool = false,
        notes: String = String::new(),
    }
}

schema! {
    pub struct Currency {
        cp: i64 = 0,
        sp: i64 = 0,
        ep: i64 = 0,
        gp: i64 = 0,
        pp: i64 = 0,
    }
}

schema! {
    pub struct Feature {
        name: String = String::new(),
        source: String = String::new(),
        description: String = String::new(),
    }
}

schema! {
    pub struct Personality {
        traits: String = String::new(),
        ideals: String = String::new(),
        bonds: String = String::new(),
        flaws: String = String::new(),
    }
}

choice! {
    pub enum SkillProficiency { None = "none", Half = "half", Proficient = "proficient", Expertise = "expertise" }
}

schema! {
    pub struct Character {
        id: String = String::new(),
        created: String = String::new(),
        updated: String = String::new(),
        name: String = text("New Character"),
        player: String = String::new(),
        campaign_id: String = String::new(),
        race: String = String::new(),
        background: String = String::new(),
        alignment: String = String::new(),
        /// At least one.
        classes: Vec<ClassLevel> = vec![ClassLevel { name: text("Fighter"), ..ClassLevel::default() }],
        xp: i64 = 0,
        abilities: IndexMap<String, i64> = ability_scores(),
        save_proficiencies: Vec<String> = Vec::new(),
        skills: IndexMap<String, SkillProficiency> = IndexMap::new(),
        jack_of_all_trades: bool = false,
        ac: i64 = 10,
        initiative_misc: i64 = 0,
        speed: i64 = 30,
        hp: HitPoints = HitPoints::default(),
        hit_dice: HitDice = HitDice::default(),
        death_saves: DeathSaves = DeathSaves::default(),
        inspiration: bool = false,
        conditions: Vec<String> = Vec::new(),
        /// 0 to 6.
        exhaustion: i64 = 0,
        proficiencies: Proficiencies = Proficiencies::default(),
        attacks: Vec<Attack> = Vec::new(),
        spellcasting: Spellcasting = Spellcasting::default(),
        resources: Vec<Resource> = Vec::new(),
        inventory: Vec<Item> = Vec::new(),
        currency: Currency = Currency::default(),
        features: Vec<Feature> = Vec::new(),
        personality: Personality = Personality::default(),
        appearance: String = String::new(),
        backstory: String = String::new(),
        notes: String = String::new(),
        /// The character builder's choices (ability method, base scores, background bonuses,
        /// species and lineage, skill picks...), kept as the UI writes them. Empty for
        /// characters made without the builder.
        build: IndexMap<String, Value> = IndexMap::new(),
    }
}

// --------------------------------------------------------------------------- handbook

schema! {
    pub struct HandbookEntry {
        id: String = String::new(),
        created: String = String::new(),
        updated: String = String::new(),
        title: String = text("Untitled"),
        category: String = text("House Rules"),
        tags: Tags = Tags::default(),
        campaign_id: String = String::new(),
        body: String = String::new(),
    }
}

// --------------------------------------------------------------------------- DM screen

choice! {
    pub enum SceneStatus { Planned = "planned", Active = "active", Completed = "completed", Skipped = "skipped" }
}

schema! {
    pub struct Scene {
        id: String = String::new(),
        created: String = String::new(),
        updated: String = String::new(),
        campaign_id: String = String::new(),
        session_id: String = String::new(),
        position: i64 = 0,
        title: String = text("New Scene"),
        location: String = String::new(),
        status: SceneStatus = SceneStatus::Planned,
        summary: String = String::new(),
        read_aloud: String = String::new(),
        body: String = String::new(),
    }
}

schema! {
    pub struct Npc {
        id: String = String::new(),
        created: String = String::new(),
        updated: String = String::new(),
        campaign_id: String = String::new(),
        name: String = text("New NPC"),
        role: String = String::new(),
        first_appears: String = String::new(),
        looks: String = String::new(),
        voice: String = String::new(),
        wants: String = String::new(),
        secret: String = String::new(),
        knows: String = String::new(),
        /// Free text, e.g. "Commoner".
        stats: String = String::new(),
        /// `srd:monsters/commoner` or `monster:<id>`, for the tracker and stat block.
        stat_ref: String = String::new(),
        notes: String = String::new(),
        tags: Tags = Tags::default(),
    }
}

schema! {
    /// A homebrew stat block. Campaign-less monsters are shared by every campaign.
    pub struct Monster {
        id: String = String::new(),
        created: String = String::new(),
        updated: String = String::new(),
        campaign_id: String = String::new(),
        name: String = text("New Monster"),
        kind: String = text("Medium humanoid, neutral"),
        cr: String = text("0"),
        xp: i64 = 0,
        ac: i64 = 10,
        hp: i64 = 1,
        hp_roll: String = String::new(),
        init_bonus: i64 = 0,
        speed: String = text("30 ft."),
        abilities: IndexMap<String, i64> = ability_scores(),
        body: String = String::new(),
        notes: String = String::new(),
    }
}

choice! {
    pub enum NoteKind { General = "general", Session = "session", Quest = "quest", Pin = "pin" }
}

schema! {
    pub struct Note {
        id: String = String::new(),
        created: String = String::new(),
        updated: String = String::new(),
        campaign_id: String = String::new(),
        session_id: String = String::new(),
        kind: NoteKind = NoteKind::General,
        title: String = text("New note"),
        body: String = String::new(),
        pinned: bool = false,
        done: bool = false,
    }
}

schema! {
    pub struct Encounter {
        id: String = String::new(),
        created: String = String::new(),
        updated: String = String::new(),
        campaign_id: String = String::new(),
        session_id: String = String::new(),
        scene_id: String = String::new(),
        name: String = text("New Encounter"),
        round: i64 = 0,
        turn: i64 = 0,
        active: bool = false,
        combatants: Vec<Combatant> = Vec::new(),
        /// 1 to 12.
        party_size: i64 = 4,
        /// 1 to 20.
        party_level: i64 = 1,
        log: Vec<LogEntry> = Vec::new(),
        notes: String = String::new(),
    }
}

schema! {
    pub struct TrackerStep {
        label: String = String::new(),
    }
}

schema! {
    /// A clock or counter, e.g. "Alarm 3 of 7" with a label for each step.
    pub struct Tracker {
        id: String = String::new(),
        created: String = String::new(),
        updated: String = String::new(),
        campaign_id: String = String::new(),
        session_id: String = String::new(),
        name: String = text("New tracker"),
        value: i64 = 0,
        /// 1 to 100.
        max: i64 = 6,
        steps: Vec<TrackerStep> = Vec::new(),
        notes: String = String::new(),
    }
}

schema! {
    /// An image (map, letter, portrait) stored as a file next to the database.
    pub struct Handout {
        id: String = String::new(),
        created: String = String::new(),
        updated: String = String::new(),
        campaign_id: String = String::new(),
        folder: String = String::new(),
        title: String = text("New handout"),
        /// The stored file's name, or "" before an image is uploaded.
        filename: String = String::new(),
        mime: String = String::new(),
        body: String = String::new(),
        /// A map drawn in the app (grid, legend, cells, walls, labels, markers, tokens, fog),
        /// as the map editor writes it; empty for image handouts.
        map: IndexMap<String, Value> = IndexMap::new(),
    }
}

// --------------------------------------------------------------------------- range checks

/// Limits that the type alone doesn't express. Returns the first problem found.
pub trait Check {
    fn check(&self) -> Result<(), String> {
        Ok(())
    }
}

fn within(field: &str, value: i64, low: i64, high: i64) -> Result<(), String> {
    if (low..=high).contains(&value) {
        Ok(())
    } else {
        Err(format!("{field} must be from {low} to {high}, got {value}"))
    }
}

impl Check for Campaign {}
impl Check for Session {}
impl Check for HandbookEntry {}
impl Check for Scene {}
impl Check for Npc {}
impl Check for Monster {}
impl Check for Note {}
impl Check for Handout {}

impl Check for Character {
    fn check(&self) -> Result<(), String> {
        if self.classes.is_empty() {
            return Err("classes must list at least one class".into());
        }
        within("exhaustion", self.exhaustion, 0, 6)
    }
}

impl Check for Encounter {
    fn check(&self) -> Result<(), String> {
        within("party_size", self.party_size, 1, 12)?;
        within("party_level", self.party_level, 1, 20)
    }
}

impl Check for Tracker {
    fn check(&self) -> Result<(), String> {
        within("max", self.max, 1, 100)
    }
}

/// Read a value into a schema leniently, then apply its range checks.
pub fn validate<T: Lenient + Check>(value: Value) -> Result<T, String> {
    let doc = T::from_json(value)?;
    doc.check()?;
    Ok(doc)
}

// --------------------------------------------------------------------------- request bodies

choice! {
    pub enum ActionKind {
        Add = "add", AddMonster = "add_monster", AddCharacter = "add_character", AddNpc = "add_npc",
        Update = "update", Remove = "remove", Hp = "hp", RollInitiative = "roll_initiative", RollHp = "roll_hp",
        Sort = "sort", Next = "next", Prev = "prev", End = "end", Clear = "clear",
    }
}

choice! {
    /// Which combatants `roll_initiative` rolls for.
    pub enum Which { All = "all", Monsters = "monsters", Unset = "unset" }
}

schema! {
    /// One change to an encounter. Which fields matter depends on `action`.
    pub struct EncounterAction {
        action: ActionKind = ActionKind::Add,
        id: String = String::new(),
        combatant: IndexMap<String, Value> = IndexMap::new(),
        changes: IndexMap<String, Value> = IndexMap::new(),
        srd_id: String = String::new(),
        monster_id: String = String::new(),
        npc_id: String = String::new(),
        /// 1 to 50.
        count: i64 = 1,
        average_hp: bool = true,
        character_id: String = String::new(),
        character_ids: Vec<String> = Vec::new(),
        amount: i64 = 0,
        which: Which = Which::All,
        all: bool = false,
        /// `roll_initiative`: one roll per group of identical monsters (the SRD's way).
        group: bool = false,
    }
}

impl Check for EncounterAction {
    fn check(&self) -> Result<(), String> {
        within("count", self.count, 1, 50)
    }
}

schema! {
    /// XP for some characters, or one loot item for one character.
    pub struct Award {
        xp: Option<i64> = None,
        character_ids: Vec<String> = Vec::new(),
        loot_index: Option<i64> = None,
        character_id: String = String::new(),
    }
}

impl Check for Award {}

/// A session log entry. `text` is required.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LogIn {
    pub text: String,
    #[serde(default = "note_kind", deserialize_with = "lenient")]
    pub kind: String,
}

fn note_kind() -> String {
    text("note")
}

choice! {
    pub enum RestKind { Long = "long", Short = "short" }
}

schema! {
    pub struct RestIn {
        r#type: RestKind = RestKind::Long,
    }
}

impl Check for RestIn {}

schema! {
    /// What the players' screen shows: a handout and the DM's zoom and pan.
    pub struct Projection {
        id: String = String::new(),
        /// Above 0, at most 20.
        zoom: f64 = 1.0,
        x: f64 = 0.0,
        y: f64 = 0.0,
        seq: i64 = 0,
    }
}

impl Check for Projection {
    fn check(&self) -> Result<(), String> {
        if self.zoom > 0.0 && self.zoom <= 20.0 {
            Ok(())
        } else {
            Err(format!("zoom must be above 0 and at most 20, got {}", self.zoom))
        }
    }
}

/// A dice or arithmetic expression. Required.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Expression {
    pub expression: String,
}

/// Bodies with required fields: read strictly, like Pydantic does for them.
macro_rules! strict_body {
    ($($name:ident),*) => {$(
        impl Lenient for $name {
            fn from_json(value: Value) -> Result<Self, String> {
                serde_json::from_value(value).map_err(|e| e.to_string())
            }
        }
        impl Check for $name {}
    )*};
}

strict_body!(Expression, LogIn);

schema! {
    pub struct MarkdownFile {
        filename: String = String::new(),
        text: String = String::new(),
    }
}

schema! {
    pub struct MarkdownImport {
        files: Vec<MarkdownFile> = Vec::new(),
        campaign_id: String = String::new(),
    }
}

impl Check for MarkdownImport {}

schema! {
    pub struct ClassesIn {
        classes: Vec<ClassLevel> = Vec::new(),
        edition: Edition = Edition::E2014,
    }
}

impl Check for ClassesIn {}

// --------------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn int(v: Value) -> Option<i64> {
        i64::from_json(v).ok()
    }

    fn boolean(v: Value) -> Option<bool> {
        bool::from_json(v).ok()
    }

    /// The same table the Python app's Pydantic models accept (checked 2026-11-16).
    #[test]
    fn whole_numbers_convert_like_pydantic() {
        assert_eq!(int(json!(true)), Some(1));
        assert_eq!(int(json!(1.0)), Some(1));
        assert_eq!(int(json!("300")), Some(300));
        assert_eq!(int(json!(" 7 ")), Some(7));
        assert_eq!(int(json!("1.0")), Some(1));
        for bad in
            [json!(null), json!(1.5), json!("1.5"), json!("yes"), json!(""), json!("1e3"), json!([]), json!(1e20)]
        {
            assert_eq!(int(bad.clone()), None, "{bad}");
        }
    }

    #[test]
    fn booleans_convert_like_pydantic() {
        for yes in [
            json!(true),
            json!(1),
            json!(1.0),
            json!("1"),
            json!("yes"),
            json!("true"),
            json!("on"),
            json!("t"),
            json!("y"),
        ] {
            assert_eq!(boolean(yes.clone()), Some(true), "{yes}");
        }
        for no in [json!(false), json!(0), json!("0"), json!("no"), json!("off"), json!("f"), json!("n")] {
            assert_eq!(boolean(no.clone()), Some(false), "{no}");
        }
        for bad in [json!(null), json!(2), json!(1.5), json!("1.0"), json!(""), json!("abc")] {
            assert_eq!(boolean(bad.clone()), None, "{bad}");
        }
    }

    #[test]
    fn numbers_and_text() {
        assert_eq!(f64::from_json(json!("1e3")).ok(), Some(1000.0));
        assert_eq!(f64::from_json(json!(" 7 ")).ok(), Some(7.0));
        assert!(f64::from_json(json!("abc")).is_err());
        assert_eq!(String::from_json(json!(null)).ok(), Some(String::new()));
        assert!(String::from_json(json!(1)).is_err());
        assert!(Vec::<String>::from_json(json!("a,b")).is_err());
    }

    #[test]
    fn defaults_fill_missing_fields_and_extra_keys_are_ignored() {
        let ch: Character =
            validate(json!({"name": "Pip", "xp": "300", "inspiration": "yes", "spaceship": 1})).unwrap();
        assert_eq!((ch.name.as_str(), ch.xp, ch.inspiration), ("Pip", 300, true));
        assert_eq!(ch.classes[0].name, "Fighter");
        assert_eq!(ch.abilities.keys().collect::<Vec<_>>(), ["str", "dex", "con", "int", "wis", "cha"]);
        assert_eq!(ch.hp, HitPoints { max: 10, current: 10, temp: 0 });
    }

    #[test]
    fn range_checks() {
        assert!(validate::<Character>(json!({"exhaustion": 7})).is_err());
        assert!(validate::<Character>(json!({"classes": []})).is_err());
        assert!(validate::<Encounter>(json!({"party_size": 0})).is_err());
        assert!(validate::<Tracker>(json!({"max": 101})).is_err());
        assert!(validate::<Projection>(json!({"zoom": 0})).is_err());
    }

    #[test]
    fn tags_and_choices() {
        let npc: Npc = validate(json!({"tags": "undead,  boss, "})).unwrap();
        assert_eq!(npc.tags.0, ["undead", "boss"]);
        let session: Session = validate(json!({"status": "done"})).unwrap();
        assert_eq!(session.status, SessionStatus::Done);
        assert!(validate::<Session>(json!({"status": "nope"})).is_err());
        assert_eq!(serde_json::to_value(Edition::E2024).unwrap(), json!("2024"));
    }

    #[test]
    fn nulls_in_text_fields_become_empty() {
        let scene: Scene = validate(json!({"session_id": null})).unwrap();
        assert_eq!(scene.session_id, "");
        assert!(validate::<Scene>(json!({"position": null})).is_err());
    }

    #[test]
    fn required_fields() {
        assert!(serde_json::from_value::<LogIn>(json!({})).is_err());
        let entry: LogIn = serde_json::from_value(json!({"text": "hi"})).unwrap();
        assert_eq!(entry.kind, "note");
        assert!(serde_json::from_value::<Expression>(json!({})).is_err());
    }
}
