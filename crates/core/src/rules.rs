//! 5e rules numbers and calculations, for the 2014 (SRD 5.1) and 2024 (SRD 5.2.1) rules.
//!
//! Where the editions differ, functions take an [`Edition`]. Every number here was
//! checked against the bundled SRD data (see `docs/rules-audit.md`); tables the SRD
//! doesn't include (XP by level, point buy, the standard array) are marked as such.

use indexmap::IndexMap;
use serde::Serialize;
use serde_json::{Value, json};

use crate::schemas::{Character, ClassLevel, Edition, SkillProficiency};

pub const ABILITIES: [&str; 6] = ["str", "dex", "con", "int", "wis", "cha"];

pub const ABILITY_NAMES: [(&str, &str); 6] = [
    ("str", "Strength"),
    ("dex", "Dexterity"),
    ("con", "Constitution"),
    ("int", "Intelligence"),
    ("wis", "Wisdom"),
    ("cha", "Charisma"),
];

/// Each skill and the ability it uses.
pub const SKILLS: [(&str, &str); 18] = [
    ("acrobatics", "dex"),
    ("animal-handling", "wis"),
    ("arcana", "int"),
    ("athletics", "str"),
    ("deception", "cha"),
    ("history", "int"),
    ("insight", "wis"),
    ("intimidation", "cha"),
    ("investigation", "int"),
    ("medicine", "wis"),
    ("nature", "int"),
    ("perception", "wis"),
    ("performance", "cha"),
    ("persuasion", "cha"),
    ("religion", "int"),
    ("sleight-of-hand", "dex"),
    ("stealth", "dex"),
    ("survival", "wis"),
];

pub const CONDITIONS: [&str; 15] = [
    "blinded",
    "charmed",
    "deafened",
    "exhaustion",
    "frightened",
    "grappled",
    "incapacitated",
    "invisible",
    "paralyzed",
    "petrified",
    "poisoned",
    "prone",
    "restrained",
    "stunned",
    "unconscious",
];

/// XP needed to reach each level, 1 to 20. Not in either SRD.
pub const XP_BY_LEVEL: [i64; 20] = [
    0, 300, 900, 2700, 6500, 14000, 23000, 34000, 48000, 64000, 85000, 100000, 120000, 140000, 165000, 195000, 225000,
    265000, 305000, 355000,
];

/// XP for each challenge rating, as printed on the SRD stat blocks.
pub const CR_XP: [(&str, i64); 34] = [
    ("0", 10),
    ("1/8", 25),
    ("1/4", 50),
    ("1/2", 100),
    ("1", 200),
    ("2", 450),
    ("3", 700),
    ("4", 1100),
    ("5", 1800),
    ("6", 2300),
    ("7", 2900),
    ("8", 3900),
    ("9", 5000),
    ("10", 5900),
    ("11", 7200),
    ("12", 8400),
    ("13", 10000),
    ("14", 11500),
    ("15", 13000),
    ("16", 15000),
    ("17", 18000),
    ("18", 20000),
    ("19", 22000),
    ("20", 25000),
    ("21", 33000),
    ("22", 41000),
    ("23", 50000),
    ("24", 62000),
    ("25", 75000),
    ("26", 90000),
    ("27", 105000),
    ("28", 120000),
    ("29", 135000),
    ("30", 155000),
];

/// XP budget per character by party level: (low, moderate, high). From SRD 5.2.1
/// (rules/gameplay-toolbox-combat-encounters). SRD 5.1 has no encounter-building
/// table, so 2014 campaigns use this one too.
pub const XP_BUDGET: [(i64, i64, i64); 20] = [
    (50, 75, 100),
    (100, 150, 200),
    (150, 225, 400),
    (250, 375, 500),
    (500, 750, 1100),
    (600, 1000, 1400),
    (750, 1300, 1700),
    (1000, 1700, 2100),
    (1300, 2000, 2600),
    (1600, 2300, 3100),
    (1900, 2900, 4100),
    (2200, 3700, 4700),
    (2600, 4200, 5400),
    (2900, 4900, 6200),
    (3300, 5400, 7800),
    (3800, 6100, 9800),
    (4500, 7200, 11700),
    (5000, 8700, 14200),
    (5500, 10700, 17200),
    (6400, 13200, 22000),
];

/// Point-buy cost of each score, and the budget. Not in either SRD.
pub const POINT_BUY_COST: [(i64, i64); 8] = [(8, 0), (9, 1), (10, 2), (11, 3), (12, 4), (13, 5), (14, 7), (15, 9)];
pub const POINT_BUY_BUDGET: i64 = 27;
/// Not in either SRD.
pub const STANDARD_ARRAY: [i64; 6] = [15, 14, 13, 12, 10, 8];

pub const HIT_DICE: [(&str, i64); 12] = [
    ("barbarian", 12),
    ("bard", 8),
    ("cleric", 8),
    ("druid", 8),
    ("fighter", 10),
    ("monk", 8),
    ("paladin", 10),
    ("ranger", 10),
    ("rogue", 8),
    ("sorcerer", 6),
    ("warlock", 8),
    ("wizard", 6),
];

/// Spell slots by caster level (index 0 is "no slots"): the full-caster table.
pub const SPELL_SLOTS: [&[i64]; 21] = [
    &[],
    &[2],
    &[3],
    &[4, 2],
    &[4, 3],
    &[4, 3, 2],
    &[4, 3, 3],
    &[4, 3, 3, 1],
    &[4, 3, 3, 2],
    &[4, 3, 3, 3, 1],
    &[4, 3, 3, 3, 2],
    &[4, 3, 3, 3, 2, 1],
    &[4, 3, 3, 3, 2, 1],
    &[4, 3, 3, 3, 2, 1, 1],
    &[4, 3, 3, 3, 2, 1, 1],
    &[4, 3, 3, 3, 2, 1, 1, 1],
    &[4, 3, 3, 3, 2, 1, 1, 1],
    &[4, 3, 3, 3, 2, 1, 1, 1, 1],
    &[4, 3, 3, 3, 3, 1, 1, 1, 1],
    &[4, 3, 3, 3, 3, 2, 1, 1, 1],
    &[4, 3, 3, 3, 3, 2, 2, 1, 1],
];

/// How a class counts toward caster level. The warlock's Pact Magic is separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Caster {
    Full,
    Half,
}

fn caster_kind(class: &str) -> Option<Caster> {
    match class.to_lowercase().as_str() {
        "bard" | "cleric" | "druid" | "sorcerer" | "wizard" => Some(Caster::Full),
        "paladin" | "ranger" => Some(Caster::Half),
        _ => None,
    }
}

/// The six-step 2014 exhaustion table (conditions/exhaustion, SRD 5.1). Cumulative.
const EXHAUSTION_2014: [&str; 6] = [
    "Disadvantage on ability checks",
    "Speed halved",
    "Disadvantage on attack rolls and saving throws",
    "Hit point maximum halved",
    "Speed reduced to 0",
    "Death",
];

// --------------------------------------------------------------------------- small calculations

pub fn ability_modifier(score: i64) -> i64 {
    (score - 10).div_euclid(2)
}

pub fn proficiency_bonus(level: i64) -> i64 {
    2 + (level.clamp(1, 20) - 1) / 4
}

pub fn level_for_xp(xp: i64) -> i64 {
    XP_BY_LEVEL.iter().filter(|&&needed| xp >= needed).count().max(1) as i64
}

pub fn xp_for_next_level(level: i64) -> Option<i64> {
    (1..20).contains(&level).then(|| XP_BY_LEVEL[level as usize])
}

pub fn cr_xp(cr: &str) -> i64 {
    CR_XP.iter().find(|(c, _)| *c == cr).map_or(0, |(_, xp)| *xp)
}

pub fn hit_die(class: &str) -> Option<i64> {
    let class = class.to_lowercase();
    HIT_DICE.iter().find(|(c, _)| *c == class).map(|(_, die)| *die)
}

/// Total of all class levels; a total of 0 counts as level 1.
pub fn total_level(classes: &[ClassLevel]) -> i64 {
    match classes.iter().map(|c| c.level).sum::<i64>() {
        0 => 1,
        total => total,
    }
}

/// Combined caster level for the slot table.
///
/// Single-class half casters round up: from 2nd level in 2014 (classes/paladin), from
/// 1st level in 2024, where a level 1 Paladin or Ranger has two 1st-level slots.
/// Multiclass characters add full levels and half of each half-caster level, rounded down.
pub fn caster_level(classes: &[ClassLevel], edition: Edition) -> i64 {
    let single_class = classes.len() == 1;
    classes
        .iter()
        .map(|c| match caster_kind(&c.name) {
            Some(Caster::Full) => c.level,
            Some(Caster::Half) if single_class => {
                let casts = c.level >= 2 || edition == Edition::E2024;
                if casts { (c.level + 1) / 2 } else { 0 }
            }
            Some(Caster::Half) => c.level / 2,
            None => 0,
        })
        .sum()
}

pub fn spell_slots_for(classes: &[ClassLevel], edition: Edition) -> Vec<i64> {
    let level = caster_level(classes, edition);
    if level <= 0 { Vec::new() } else { SPELL_SLOTS[level.min(20) as usize].to_vec() }
}

/// Pact Magic slots for a single-class warlock of `level`, from the SRD's Warlock table
/// (index 0 is 1st-level slots; trailing levels without slots are left off).
pub fn pact_slots(level: i64, edition: Edition) -> Vec<i64> {
    let level = level.clamp(1, 20).to_string();
    let entry = crate::srd::srd(edition).get("classes/warlock");
    let row = entry.and_then(|e| e.data.get("spell_slots")).and_then(|t| t.get(&level)).and_then(Value::as_array);
    let mut slots: Vec<i64> = row.into_iter().flatten().map(|n| n.as_i64().unwrap_or(0)).collect();
    while slots.last() == Some(&0) {
        slots.pop();
    }
    slots
}

/// A single-class warlock, whose slots (Pact Magic) all come back on a short rest.
pub fn is_warlock_only(classes: &[ClassLevel]) -> bool {
    !classes.is_empty() && classes.iter().all(|c| c.name.eq_ignore_ascii_case("warlock"))
}

// --------------------------------------------------------------------------- exhaustion

/// What an exhaustion level does (conditions/exhaustion in each SRD).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exhaustion {
    /// Taken off every D20 Test (2024 only).
    pub d20_penalty: i64,
    /// The effects in words, e.g. `["D20 Tests -4", "Speed -10 ft."]`.
    pub effects: Vec<String>,
}

pub fn exhaustion_effects(level: i64, edition: Edition) -> Exhaustion {
    let level = level.clamp(0, 6);
    match edition {
        Edition::E2024 => {
            let mut effects = Vec::new();
            if level > 0 {
                effects.push(format!("D20 Tests -{}", 2 * level));
                effects.push(format!("Speed -{} ft.", 5 * level));
            }
            if level == 6 {
                effects.push("Death".into());
            }
            Exhaustion { d20_penalty: 2 * level, effects }
        }
        Edition::E2014 => Exhaustion {
            d20_penalty: 0,
            effects: EXHAUSTION_2014[..level as usize].iter().map(|e| e.to_string()).collect(),
        },
    }
}

pub fn exhausted_speed(speed: i64, level: i64, edition: Edition) -> i64 {
    let level = level.clamp(0, 6);
    match edition {
        Edition::E2024 => (speed - 5 * level).max(0),
        Edition::E2014 if level >= 5 => 0,
        Edition::E2014 if level >= 2 => speed.div_euclid(2),
        Edition::E2014 => speed,
    }
}

// --------------------------------------------------------------------------- character sheet

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpellNumbers {
    pub save_dc: i64,
    pub attack_bonus: i64,
}

/// Numbers worked out from a character sheet.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Derived {
    pub level: i64,
    pub proficiency_bonus: i64,
    pub modifiers: IndexMap<String, i64>,
    /// Plain ability check bonuses (2014 Jack of All Trades and 2024 exhaustion included).
    pub checks: IndexMap<String, i64>,
    pub saves: IndexMap<String, i64>,
    pub skills: IndexMap<String, i64>,
    pub initiative: i64,
    pub passive_perception: i64,
    pub passive_investigation: i64,
    pub passive_insight: i64,
    /// Empty (`{}`) without a spellcasting ability.
    #[serde(serialize_with = "spell_or_empty")]
    pub spell: Option<SpellNumbers>,
    /// Speed after exhaustion.
    pub speed: i64,
    /// What the current exhaustion level does.
    pub exhaustion: Vec<String>,
    pub carrying_capacity: i64,
    pub next_level_xp: Option<i64>,
}

fn spell_or_empty<S: serde::Serializer>(spell: &Option<SpellNumbers>, s: S) -> Result<S::Ok, S::Error> {
    match spell {
        Some(numbers) => numbers.serialize(s),
        None => json!({}).serialize(s),
    }
}

/// Modifiers, bonuses and passives for a character sheet.
///
/// The 2024 exhaustion penalty applies to every D20 Test (ability checks, saves, skills,
/// initiative, spell attacks). The spell save DC isn't a roll. Passive scores aren't rolls
/// either, so they're left alone in 2024 (our reading; the SRD doesn't say). In 2014,
/// exhaustion 1+ gives Disadvantage on ability checks, which is -5 to passive scores
/// (rules/ability-checks).
pub fn derive(ch: &Character, edition: Edition) -> Derived {
    let score = |ability: &str| ch.abilities.get(ability).copied().unwrap_or(10);
    let modifiers: IndexMap<String, i64> =
        ABILITIES.iter().map(|&a| (a.to_string(), ability_modifier(score(a)))).collect();
    let modifier = |ability: &str| modifiers[ability];
    let level = total_level(&ch.classes);
    let pb = proficiency_bonus(level);
    let penalty = exhaustion_effects(ch.exhaustion, edition).d20_penalty;

    let saves = ABILITIES
        .iter()
        .map(|&a| {
            let proficient = ch.save_proficiencies.iter().any(|s| s == a);
            (a.to_string(), modifier(a) + if proficient { pb } else { 0 })
        })
        .collect::<IndexMap<_, _>>();

    let half = pb / 2;
    let skills = SKILLS
        .iter()
        .map(|&(skill, ability)| {
            let bonus = match ch.skills.get(skill).copied().unwrap_or_default() {
                SkillProficiency::Expertise => pb * 2,
                SkillProficiency::Proficient => pb,
                SkillProficiency::Half => half,
                SkillProficiency::None if ch.jack_of_all_trades => half,
                SkillProficiency::None => 0,
            };
            (skill.to_string(), modifier(ability) + bonus)
        })
        .collect::<IndexMap<_, _>>();

    // Plain checks and initiative (a Dexterity check) have no proficiency. 2014 Jack of All
    // Trades covers any such check (classes/bard); the 2024 feature only covers skills.
    let jack = if ch.jack_of_all_trades && edition == Edition::E2014 { half } else { 0 };
    let checks = ABILITIES.iter().map(|&a| (a.to_string(), modifier(a) + jack - penalty)).collect();
    let initiative = modifier("dex") + ch.initiative_misc + jack;
    let passive_minus = if edition == Edition::E2014 && ch.exhaustion >= 1 { 5 } else { 0 };

    let spell = ABILITIES.contains(&ch.spellcasting.ability.as_str()).then(|| {
        let m = modifier(&ch.spellcasting.ability);
        SpellNumbers { save_dc: 8 + pb + m, attack_bonus: pb + m - penalty }
    });

    let minus = |map: &IndexMap<String, i64>| map.iter().map(|(k, v)| (k.clone(), v - penalty)).collect();
    Derived {
        level,
        proficiency_bonus: pb,
        passive_perception: 10 + skills["perception"] - passive_minus,
        passive_investigation: 10 + skills["investigation"] - passive_minus,
        passive_insight: 10 + skills["insight"] - passive_minus,
        checks,
        saves: minus(&saves),
        skills: minus(&skills),
        modifiers: modifiers.clone(),
        initiative: initiative - penalty,
        spell,
        speed: exhausted_speed(ch.speed, ch.exhaustion, edition),
        exhaustion: exhaustion_effects(ch.exhaustion, edition).effects,
        // Every SRD race and species is Small or Medium, which carry the same (Str x 15).
        carrying_capacity: score("str") * 15,
        next_level_xp: xp_for_next_level(level),
    }
}

// --------------------------------------------------------------------------- encounters

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Budget {
    pub low: i64,
    pub moderate: i64,
    pub high: i64,
}

/// Total XP budgets for a party.
pub fn encounter_budget(party_size: i64, party_level: i64) -> Budget {
    let (low, moderate, high) = XP_BUDGET[(party_level.clamp(1, 20) - 1) as usize];
    let size = party_size.max(1);
    Budget { low: low * size, moderate: moderate * size, high: high * size }
}

/// "low", "moderate" or "high": the smallest budget the XP fits. Around those, the app's
/// own bands: "trivial" under half of low, "deadly" above high (not SRD terms).
pub fn encounter_difficulty(monster_xp: i64, party_size: i64, party_level: i64) -> &'static str {
    let budget = encounter_budget(party_size, party_level);
    if monster_xp <= 0 || (monster_xp as f64) < budget.low as f64 * 0.5 {
        "trivial"
    } else if monster_xp <= budget.low {
        "low"
    } else if monster_xp <= budget.moderate {
        "moderate"
    } else if monster_xp <= budget.high {
        "high"
    } else {
        "deadly"
    }
}

/// The rules tables the UI needs, as sent by `GET /api/meta`.
pub fn tables_json() -> Value {
    let pairs = |items: &[(&str, &str)]| {
        items.iter().map(|(k, v)| (k.to_string(), json!(v))).collect::<serde_json::Map<_, _>>()
    };
    json!({
        "xp_budget": XP_BUDGET.iter().map(|&(l, m, h)| json!([l, m, h])).collect::<Vec<_>>(),
        "abilities": pairs(&ABILITY_NAMES),
        "skills": pairs(&SKILLS),
        "conditions": CONDITIONS,
        "hit_dice": HIT_DICE.iter().map(|(k, v)| (k.to_string(), json!(v))).collect::<serde_json::Map<_, _>>(),
        "xp_by_level": XP_BY_LEVEL,
        "cr_xp": CR_XP.iter().map(|(k, v)| (k.to_string(), json!(v))).collect::<serde_json::Map<_, _>>(),
        "spell_slots": SPELL_SLOTS,
        "point_buy": {
            "cost": POINT_BUY_COST.iter().map(|(k, v)| (k.to_string(), json!(v))).collect::<serde_json::Map<_, _>>(),
            "budget": POINT_BUY_BUDGET,
        },
        "standard_array": STANDARD_ARRAY,
    })
}

// --------------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schemas::validate;

    fn classes(list: &[(&str, i64)]) -> Vec<ClassLevel> {
        list.iter()
            .map(|(name, level)| ClassLevel { name: name.to_string(), level: *level, ..Default::default() })
            .collect()
    }

    fn character(value: Value) -> Character {
        validate(value).unwrap()
    }

    #[test]
    fn modifiers_and_proficiency() {
        let mods: Vec<i64> = [1, 8, 9, 10, 11, 12, 20, 30].map(ability_modifier).to_vec();
        assert_eq!(mods, [-5, -1, -1, 0, 0, 1, 5, 10]);
        let pbs: Vec<i64> = [1, 4, 5, 8, 9, 13, 17, 20].map(proficiency_bonus).to_vec();
        assert_eq!(pbs, [2, 2, 3, 3, 4, 5, 6, 6]);
    }

    #[test]
    fn levels_from_xp() {
        assert_eq!(level_for_xp(0), 1);
        assert_eq!(level_for_xp(299), 1);
        assert_eq!(level_for_xp(300), 2);
        assert_eq!(level_for_xp(355_000), 20);
        assert_eq!(xp_for_next_level(1), Some(300));
        assert_eq!(xp_for_next_level(20), None);
    }

    #[test]
    fn spell_slots() {
        let e14 = Edition::E2014;
        assert_eq!(spell_slots_for(&classes(&[("Wizard", 5)]), e14), [4, 3, 2]);
        assert_eq!(spell_slots_for(&classes(&[("Paladin", 1)]), e14), Vec::<i64>::new());
        assert_eq!(spell_slots_for(&classes(&[("Paladin", 2)]), e14), [2]);
        assert_eq!(spell_slots_for(&classes(&[("Paladin", 20)]), e14), [4, 3, 3, 3, 2]);
        // Multiclass: wizard 3 + paladin 4 is caster level 3 + 2 = 5.
        assert_eq!(spell_slots_for(&classes(&[("Wizard", 3), ("Paladin", 4)]), e14), [4, 3, 2]);
        // 2024 Paladins and Rangers cast from level 1.
        assert_eq!(spell_slots_for(&classes(&[("Paladin", 1)]), Edition::E2024), [2]);
        assert_eq!(spell_slots_for(&classes(&[("Ranger", 3)]), Edition::E2024), [3]);
        assert_eq!(spell_slots_for(&classes(&[("Fighter", 5)]), e14), Vec::<i64>::new());
        // Warlock 5: two 3rd-level Pact Magic slots, in both editions.
        assert_eq!(pact_slots(5, e14), [0, 0, 2]);
        assert_eq!(pact_slots(1, Edition::E2024), [1]);
    }

    #[test]
    fn derive_basics() {
        let ch = character(json!({
            "classes": [{"name": "Rogue", "level": 5}],
            "abilities": {"str": 8, "dex": 16, "wis": 13},
            "save_proficiencies": ["dex"],
            "skills": {"stealth": "expertise", "perception": "proficient", "insight": "half"},
            "spellcasting": {"ability": "wis"},
        }));
        let d = derive(&ch, Edition::E2014);
        assert_eq!((d.level, d.proficiency_bonus, d.initiative), (5, 3, 3));
        assert_eq!(d.saves["dex"], 6);
        assert_eq!((d.skills["stealth"], d.skills["perception"], d.skills["insight"]), (9, 4, 2));
        assert_eq!(d.passive_perception, 14);
        assert_eq!(d.spell, Some(SpellNumbers { save_dc: 12, attack_bonus: 4 }));
        assert_eq!(d.carrying_capacity, 120);
        assert_eq!(d.next_level_xp, Some(14000));
        let no_spells = serde_json::to_value(derive(&character(json!({})), Edition::E2014)).unwrap();
        assert_eq!(no_spells["spell"], json!({}));
    }

    #[test]
    fn derive_exhaustion_by_edition() {
        let ch = character(
            json!({"abilities": {"dex": 14, "wis": 12}, "exhaustion": 2, "speed": 30, "spellcasting": {"ability": "wis"}}),
        );
        let old = derive(&ch, Edition::E2014);
        let new = derive(&ch, Edition::E2024);
        assert_eq!((old.speed, old.initiative), (15, 2));
        assert_eq!(old.exhaustion[1], "Speed halved");
        assert_eq!((new.speed, new.initiative, new.saves["dex"]), (20, 2 - 4, 2 - 4));
        assert_eq!((new.skills["perception"], new.passive_perception), (1 - 4, 11));
        assert_eq!(new.spell, Some(SpellNumbers { save_dc: 11, attack_bonus: 3 - 4 }));
        assert_eq!(derive(&character(json!({"exhaustion": 5})), Edition::E2014).speed, 0);
        assert_eq!((old.passive_perception, new.checks["dex"]), (11 - 5, 2 - 4));
    }

    #[test]
    fn jack_of_all_trades_initiative_is_2014_only() {
        let ch = character(
            json!({"classes": [{"name": "Bard", "level": 5}], "abilities": {"dex": 14}, "jack_of_all_trades": true}),
        );
        assert_eq!(derive(&ch, Edition::E2014).initiative, 3);
        assert_eq!(derive(&ch, Edition::E2014).checks["str"], 1);
        assert_eq!(derive(&ch, Edition::E2024).initiative, 2);
    }

    #[test]
    fn encounter_maths() {
        assert_eq!(encounter_budget(4, 1), Budget { low: 200, moderate: 300, high: 400 });
        assert_eq!(encounter_difficulty(50, 4, 1), "trivial");
        assert_eq!(encounter_difficulty(150, 4, 1), "low");
        assert_eq!(encounter_difficulty(300, 4, 1), "moderate");
        assert_eq!(encounter_difficulty(401, 4, 1), "deadly");
        assert_eq!(cr_xp("1/4"), 50);
        assert_eq!(cr_xp("31"), 0);
    }
}
