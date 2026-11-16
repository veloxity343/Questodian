//! Table-side game logic: the initiative tracker, rewards, the session log and rests.

use serde_json::{Map, Value};

use crate::dice;
use crate::error::{Error, Result, invalid, not_found};
use crate::kinds::Kind;
use crate::repo::{Repo, new_id, now};
use crate::rules;
use crate::schemas::{
    ActionKind, Award, Character, Combatant, CombatantKind, Edition, Encounter, EncounterAction, Item, Lenient,
    LogEntry, LogIn, Monster, Npc, RestKind, Session, Which,
};
use crate::srd;

fn log(entries: &mut Vec<LogEntry>, text: impl Into<String>, kind: &str) {
    entries.push(LogEntry { id: new_id(), time: now(), kind: kind.into(), text: text.into() });
}

/// The rules edition of a record's campaign (2014 without one).
async fn edition_of(repo: &mut Repo<'_>, campaign_id: &str) -> Result<Edition> {
    if campaign_id.is_empty() {
        return Ok(Edition::E2014);
    }
    let campaign: crate::schemas::Campaign = repo.get_as(Kind::Campaigns, campaign_id).await?;
    Ok(campaign.edition)
}

// --------------------------------------------------------------------------- encounter helpers

/// Highest initiative first; ties go to the higher bonus, then by name.
pub fn sort_initiative(combatants: &mut [Combatant]) {
    combatants.sort_by(|a, b| {
        b.initiative.cmp(&a.initiative).then(b.init_bonus.cmp(&a.init_bonus)).then_with(|| a.name.cmp(&b.name))
    });
}

/// Move the turn forward (`step` 1) or back (-1). The first step forward starts combat.
pub fn advance_turn(encounter: &mut Encounter, step: i64) {
    let count = encounter.combatants.len() as i64;
    if count == 0 {
        (encounter.round, encounter.turn, encounter.active) = (0, 0, false);
        return;
    }
    if !encounter.active {
        sort_initiative(&mut encounter.combatants);
        (encounter.active, encounter.round, encounter.turn) = (true, 1, 0);
        return;
    }
    let mut turn = encounter.turn + step;
    let mut round = encounter.round;
    while turn >= count {
        turn -= count;
        round += 1;
    }
    while turn < 0 {
        if round <= 1 {
            turn = 0;
            break;
        }
        turn += count;
        round -= 1;
    }
    (encounter.turn, encounter.round) = (turn, round);
}

/// Damage (negative) is absorbed by temporary HP first; healing (positive) stops at max HP.
pub fn apply_hp_change(combatant: &mut Combatant, amount: i64) {
    if amount < 0 {
        let damage = -amount;
        let absorbed = combatant.temp_hp.min(damage);
        combatant.temp_hp -= absorbed;
        combatant.hp = (combatant.hp - (damage - absorbed)).max(0);
    } else {
        let cap = if combatant.max_hp != 0 { combatant.max_hp } else { combatant.hp + amount };
        combatant.hp = cap.min(combatant.hp + amount);
    }
}

/// Average HP, or a roll of the hit dice (at least 1) when asked and possible.
fn roll_hp(average_hp: i64, hp_roll: &str, average: bool) -> i64 {
    if !average && !hp_roll.is_empty() {
        if let Ok(result) = dice::roll(hp_roll) {
            return (result.total_f64().trunc() as i64).max(1);
        }
    }
    average_hp
}

/// A monster to add, from the SRD or a homebrew stat block, in one shape.
struct StatSource {
    name: String,
    ac: i64,
    hp: i64,
    hp_roll: String,
    init_bonus: i64,
    xp: i64,
    notes: String,
    srd_id: String,
    monster_id: String,
}

async fn stat_source(repo: &mut Repo<'_>, srd_id: &str, monster_id: &str, edition: Edition) -> Result<StatSource> {
    if !monster_id.is_empty() {
        let m: Monster = repo.get_as(Kind::Monsters, monster_id).await?;
        return Ok(StatSource {
            name: m.name,
            ac: m.ac,
            hp: m.hp,
            hp_roll: m.hp_roll,
            init_bonus: m.init_bonus,
            xp: m.xp,
            notes: m.kind,
            srd_id: String::new(),
            monster_id: m.id,
        });
    }
    let entry = srd::srd(edition)
        .get(srd_id)
        .filter(|e| e.category == "monsters")
        .ok_or_else(|| not_found("monster not found"))?;
    let number =
        |key: &str, default: i64| entry.data.get(key).and_then(|v| i64::from_json(v.clone()).ok()).unwrap_or(default);
    Ok(StatSource {
        name: entry.name.clone(),
        ac: number("ac", 10),
        hp: number("hp", 1),
        hp_roll: entry.data.get("hp_roll").and_then(Value::as_str).unwrap_or("").to_string(),
        init_bonus: number("init_bonus", 0),
        xp: number("xp", 0),
        notes: entry.summary.clone(),
        srd_id: entry.id.clone(),
        monster_id: String::new(),
    })
}

impl StatSource {
    fn combatant(&self, name: String, kind: CombatantKind, hp: i64) -> Combatant {
        Combatant {
            id: new_id(),
            name,
            kind,
            ac: self.ac,
            init_bonus: self.init_bonus,
            xp: self.xp,
            notes: self.notes.clone(),
            srd_id: self.srd_id.clone(),
            monster_id: self.monster_id.clone(),
            hp,
            max_hp: hp,
            ..Combatant::default()
        }
    }
}

/// Keep a linked character sheet's HP (and death saves) in step with the tracker.
/// `failures` of 3 marks the character dead (massive damage).
async fn sync_pc(repo: &mut Repo<'_>, combatant: &Combatant, failures: i64, healed_from_zero: bool) -> Result<()> {
    if combatant.kind != CombatantKind::Pc || combatant.character_id.is_empty() {
        return Ok(());
    }
    let mut ch: Character = match repo.get_as(Kind::Characters, &combatant.character_id).await {
        Ok(ch) => ch,
        Err(Error::NotFound(_)) => return Ok(()),
        Err(e) => return Err(e),
    };
    ch.hp.current = combatant.hp;
    ch.hp.temp = combatant.temp_hp;
    // Healing from 0 resets death saves, unless the character is already dead (3 failures):
    // that takes revival magic, which the DM records by hand.
    if healed_from_zero && ch.death_saves.failures < 3 {
        ch.death_saves.successes = 0;
        ch.death_saves.failures = 0;
    }
    ch.death_saves.failures = (ch.death_saves.failures + failures).min(3);
    repo.save_as(Kind::Characters, &ch).await?;
    Ok(())
}

fn find(combatants: &mut [Combatant], id: &str) -> Result<usize> {
    combatants.iter().position(|c| c.id == id).ok_or_else(|| not_found("combatant not found"))
}

fn combatant_from(fields: Map<String, Value>) -> Result<Combatant> {
    Combatant::from_json(Value::Object(fields)).map_err(Error::Invalid)
}

// --------------------------------------------------------------------------- encounter actions

/// Apply one change to an encounter and save it.
pub async fn encounter_action(repo: &mut Repo<'_>, encounter_id: &str, body: EncounterAction) -> Result<Value> {
    let mut enc: Encounter = repo.get_as(Kind::Encounters, encounter_id).await?;
    match body.action {
        ActionKind::Add => {
            let mut c = combatant_from(body.combatant.into_iter().collect())?;
            c.id = new_id();
            if c.max_hp == 0 {
                c.max_hp = c.hp;
            }
            enc.combatants.push(c);
        }
        ActionKind::AddMonster => add_monsters(repo, &mut enc, &body).await?,
        ActionKind::AddNpc => add_npc(repo, &mut enc, &body.npc_id).await?,
        ActionKind::AddCharacter => {
            let ids = if body.character_ids.is_empty() {
                vec![body.character_id.clone()]
            } else {
                body.character_ids.clone()
            };
            for id in ids {
                let ch: Character = repo.get_as(Kind::Characters, &id).await?;
                let edition = edition_of(repo, &ch.campaign_id).await?;
                enc.combatants.push(Combatant {
                    id: new_id(),
                    name: ch.name.clone(),
                    kind: CombatantKind::Pc,
                    ac: ch.ac,
                    hp: ch.hp.current,
                    max_hp: ch.hp.max,
                    temp_hp: ch.hp.temp,
                    init_bonus: rules::derive(&ch, edition).initiative,
                    character_id: ch.id.clone(),
                    conditions: ch.conditions.clone(),
                    ..Combatant::default()
                });
            }
        }
        ActionKind::Update => {
            let i = find(&mut enc.combatants, &body.id)?;
            let Value::Object(mut fields) = serde_json::to_value(&enc.combatants[i]).expect("combatants serialize")
            else {
                unreachable!("a combatant is an object")
            };
            for (key, value) in body.changes {
                if key != "id" && fields.contains_key(&key) {
                    fields.insert(key, value);
                }
            }
            enc.combatants[i] = combatant_from(fields)?;
        }
        ActionKind::Remove => {
            let i = find(&mut enc.combatants, &body.id)?;
            enc.combatants.remove(i);
            if enc.active && (i as i64) < enc.turn {
                enc.turn -= 1;
            }
            if enc.turn >= enc.combatants.len() as i64 {
                enc.turn = 0;
            }
        }
        ActionKind::Hp => change_hp(repo, &mut enc, &body.id, body.amount).await?,
        ActionKind::RollInitiative => {
            for c in &mut enc.combatants {
                let skip = match body.which {
                    Which::Monsters => c.kind == CombatantKind::Pc,
                    Which::Unset => c.initiative != 0,
                    Which::All => false,
                };
                if !skip {
                    c.initiative = dice::roll(&format!("d20{:+}", c.init_bonus))?.total_f64() as i64;
                }
            }
            sort_initiative(&mut enc.combatants);
        }
        ActionKind::RollHp => {
            let edition = edition_of(repo, &enc.campaign_id).await?;
            for i in 0..enc.combatants.len() {
                let c = &enc.combatants[i];
                if c.kind != CombatantKind::Monster || (c.srd_id.is_empty() && c.monster_id.is_empty()) {
                    continue;
                }
                let (srd_id, monster_id) = (c.srd_id.clone(), c.monster_id.clone());
                let source = match stat_source(repo, &srd_id, &monster_id, edition).await {
                    Ok(source) => source,
                    Err(Error::NotFound(_)) => continue,
                    Err(e) => return Err(e),
                };
                let hp = roll_hp(source.hp, &source.hp_roll, false);
                (enc.combatants[i].hp, enc.combatants[i].max_hp) = (hp, hp);
            }
            log(&mut enc.log, "Rolled hit points for the monsters", "combat");
        }
        ActionKind::Sort => sort_initiative(&mut enc.combatants),
        ActionKind::Next => {
            let (was_active, round_before) = (enc.active, enc.round);
            advance_turn(&mut enc, 1);
            if !was_active && enc.active {
                log(&mut enc.log, "Combat begins — round 1", "combat");
            } else if enc.round != round_before {
                let text = format!("Round {} begins", enc.round);
                log(&mut enc.log, text, "combat");
            }
        }
        ActionKind::Prev => advance_turn(&mut enc, -1),
        ActionKind::End => {
            if enc.active {
                let text = format!("Combat ends after {} round(s)", enc.round);
                log(&mut enc.log, text, "combat");
            }
            for c in &enc.combatants {
                sync_pc(repo, c, 0, false).await?;
            }
            (enc.active, enc.round, enc.turn) = (false, 0, 0);
        }
        ActionKind::Clear => {
            if !body.all {
                enc.combatants.retain(|c| c.kind == CombatantKind::Pc);
            } else {
                enc.combatants.clear();
            }
            (enc.active, enc.round, enc.turn) = (false, 0, 0);
        }
    }
    repo.save_as(Kind::Encounters, &enc).await
}

/// Add `count` copies of a monster. Names are numbered once there's more than one.
async fn add_monsters(repo: &mut Repo<'_>, enc: &mut Encounter, body: &EncounterAction) -> Result<()> {
    let edition = edition_of(repo, &enc.campaign_id).await?;
    let source = stat_source(repo, &body.srd_id, &body.monster_id, edition).await?;
    let is_same = |c: &Combatant| {
        (!source.srd_id.is_empty() && c.srd_id == source.srd_id)
            || (!source.monster_id.is_empty() && c.monster_id == source.monster_id)
    };
    let same: Vec<usize> = (0..enc.combatants.len()).filter(|&i| is_same(&enc.combatants[i])).collect();
    if let [only] = same[..]
        && enc.combatants[only].name == source.name
    {
        enc.combatants[only].name = format!("{} 1", source.name);
    }
    for i in 0..body.count {
        let hp = roll_hp(source.hp, &source.hp_roll, body.average_hp);
        let name = if body.count == 1 && same.is_empty() {
            source.name.clone()
        } else {
            format!("{} {}", source.name, same.len() as i64 + i + 1)
        };
        enc.combatants.push(source.combatant(name, CombatantKind::Monster, hp));
    }
    Ok(())
}

/// Add an NPC, with its linked stat block's numbers if it has one.
async fn add_npc(repo: &mut Repo<'_>, enc: &mut Encounter, npc_id: &str) -> Result<()> {
    let npc: Npc = repo.get_as(Kind::Npcs, npc_id).await?;
    let mut source = StatSource {
        name: String::new(),
        ac: 10,
        hp: 4,
        hp_roll: String::new(),
        init_bonus: 0,
        xp: 0,
        notes: String::new(),
        srd_id: String::new(),
        monster_id: String::new(),
    };
    if let Some((kind, id)) = npc.stat_ref.split_once(':')
        && matches!(kind, "srd" | "monster")
    {
        let edition = edition_of(repo, &enc.campaign_id).await?;
        let (srd_id, monster_id) = if kind == "srd" { (id, "") } else { ("", id) };
        source = stat_source(repo, srd_id, monster_id, edition).await?;
    }
    source.notes = if npc.role.is_empty() { npc.stats.clone() } else { npc.role.clone() };
    let mut c = source.combatant(npc.name.clone(), CombatantKind::Npc, source.hp);
    c.npc_id = npc.id.clone();
    enc.combatants.push(c);
    Ok(())
}

/// Damage or heal a combatant, with the 0 HP rules for player characters
/// (rules/damage-and-healing in both SRDs).
async fn change_hp(repo: &mut Repo<'_>, enc: &mut Encounter, id: &str, amount: i64) -> Result<()> {
    let i = find(&mut enc.combatants, id)?;
    let (before, temp_before) = (enc.combatants[i].hp, enc.combatants[i].temp_hp);
    apply_hp_change(&mut enc.combatants[i], amount);
    let c = enc.combatants[i].clone();
    let verb = if amount < 0 { "takes" } else { "heals" };
    log(&mut enc.log, format!("{} {verb} {} ({before} → {} HP)", c.name, amount.abs(), c.hp), "combat");
    let mut failures = 0;
    if amount < 0 && c.kind == CombatantKind::Pc {
        // Damage left over after reaching 0 HP; at least the hit point maximum kills outright.
        // Damage while already at 0 is a death save failure.
        let overflow = -amount - temp_before - before;
        if c.hp == 0 && c.max_hp != 0 && overflow >= c.max_hp {
            failures = 3;
            log(&mut enc.log, format!("{} dies from massive damage", c.name), "combat");
        } else if before == 0 && overflow > 0 {
            failures = 1;
            let text = format!("{} takes damage at 0 HP: a death save failure (two if it was a critical hit)", c.name);
            log(&mut enc.log, text, "combat");
        }
    }
    if c.hp == 0 && before > 0 {
        log(&mut enc.log, format!("{} drops to 0 HP", c.name), "combat");
    }
    sync_pc(repo, &c, failures, before == 0 && c.hp > 0).await
}

// --------------------------------------------------------------------------- rewards, log, rests

/// Give XP to characters, or hand a loot item to a character's inventory.
pub async fn award(repo: &mut Repo<'_>, session_id: &str, body: Award) -> Result<Value> {
    let mut session: Session = repo.get_as(Kind::Sessions, session_id).await?;
    if let Some(xp) = body.xp {
        if body.character_ids.is_empty() {
            return Err(invalid("choose at least one character"));
        }
        let mut names = Vec::new();
        for id in &body.character_ids {
            let mut ch: Character = repo.get_as(Kind::Characters, id).await?;
            ch.xp = (ch.xp + xp).max(0);
            repo.save_as(Kind::Characters, &ch).await?;
            names.push(ch.name);
        }
        session.xp_awarded += xp;
        log(&mut session.log, format!("{xp} XP awarded to {}", names.join(", ")), "loot");
    } else if let Some(index) = body.loot_index {
        let item = usize::try_from(index)
            .ok()
            .filter(|&i| i < session.loot.len())
            .ok_or_else(|| not_found("loot item not found"))?;
        let mut ch: Character = repo.get_as(Kind::Characters, &body.character_id).await?;
        let loot = &mut session.loot[item];
        ch.inventory.push(Item {
            name: loot.name.clone(),
            qty: if loot.qty != 0 { loot.qty } else { 1 },
            weight: 0.0,
            equipped: false,
            notes: loot.value.clone(),
        });
        repo.save_as(Kind::Characters, &ch).await?;
        loot.assigned_to = ch.name.clone();
        let text = format!("{} receives {}", ch.name, loot.name);
        log(&mut session.log, text, "loot");
    } else {
        return Err(invalid("nothing to award"));
    }
    repo.save_as(Kind::Sessions, &session).await
}

pub async fn add_log(repo: &mut Repo<'_>, session_id: &str, body: LogIn) -> Result<Value> {
    let mut session: Session = repo.get_as(Kind::Sessions, session_id).await?;
    let text = body.text.trim();
    if text.is_empty() {
        return Err(invalid("empty log entry"));
    }
    let kind = if body.kind.is_empty() { "note" } else { &body.kind };
    log(&mut session.log, text, kind);
    repo.save_as(Kind::Sessions, &session).await
}

/// A short or long rest, by the character's campaign edition.
pub async fn rest(repo: &mut Repo<'_>, character_id: &str, kind: RestKind) -> Result<Value> {
    let mut ch: Character = repo.get_as(Kind::Characters, character_id).await?;
    let edition = edition_of(repo, &ch.campaign_id).await?;
    let long = kind == RestKind::Long;
    // 2014: a long rest needs 1 HP at the start (rules/resting). 2024: both rests do.
    if ch.hp.current < 1 && (long || edition == Edition::E2024) {
        let name = if long { "long" } else { "short" };
        return Err(invalid(format!("a character needs at least 1 hit point to take a {name} rest")));
    }
    if long {
        ch.hp.current = ch.hp.max;
        ch.hp.temp = 0;
        let level = rules::total_level(&ch.classes);
        // 2014: regain up to half your Hit Dice (minimum one). 2024: regain all of them.
        let regained = if edition == Edition::E2024 { level } else { (level.div_euclid(2)).max(1) };
        ch.hit_dice.remaining = level.min(ch.hit_dice.remaining + regained);
        ch.death_saves.successes = 0;
        ch.death_saves.failures = 0;
        ch.exhaustion = (ch.exhaustion - 1).max(0);
    }
    // Every caster's slots return on a long rest; a warlock's Pact Magic slots on a short one too.
    if long || rules::is_warlock_only(&ch.classes) {
        for slot in ch.spellcasting.slots.values_mut() {
            slot.used = 0;
        }
    }
    for resource in &mut ch.resources {
        let resets = match resource.reset {
            crate::schemas::Reset::Short => true,
            crate::schemas::Reset::Long => long,
            crate::schemas::Reset::Never => false,
        };
        if resets {
            resource.current = resource.max;
        }
    }
    repo.save_as(Kind::Characters, &ch).await
}
