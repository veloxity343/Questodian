"""Build the character-builder rules data: data/rules-2024.json.gz.

    python3 scripts/build_rules_2024.py path/to/dnd/tools/character-sheet

The data comes from the character sheet in the `dnd` repo: `origins.py` (backgrounds and
starting gear), `srd.json` (class level tables and weapons, from SRD 5.2.1) and `items.json`
(item weights and costs), plus the tables below, transcribed from that sheet's `build.py`
and its button macros. Most of it is from SRD 5.2.1 (CC BY 4.0). Backgrounds and origin
feats not in the SRD come from the 2024 Player's Handbook and are marked `"srd": false`.

Everything here can be edited inside the app (Library → Rules data); this file is only the
starting point.
"""

from __future__ import annotations

import gzip
import json
import sys
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "data" / "rules-2024.json.gz"
ABILITIES = ["str", "dex", "con", "int", "wis", "cha"]
FULL = {"Strength": "str", "Dexterity": "dex", "Constitution": "con", "Intelligence": "int", "Wisdom": "wis", "Charisma": "cha"}

# Backgrounds and origin feats in SRD 5.2.1; everything else is from the Player's Handbook.
SRD_BACKGROUNDS = {"Acolyte", "Criminal", "Sage", "Soldier"}
SRD_FEATS = {"Alert", "Magic Initiate (Cleric)", "Magic Initiate (Druid)", "Magic Initiate (Wizard)", "Savage Attacker", "Skilled"}

SKILLS = {
    "Acrobatics": "dex", "Animal Handling": "wis", "Arcana": "int", "Athletics": "str", "Deception": "cha",
    "History": "int", "Insight": "wis", "Intimidation": "cha", "Investigation": "int", "Medicine": "wis",
    "Nature": "int", "Perception": "wis", "Performance": "cha", "Persuasion": "cha", "Religion": "int",
    "Sleight of Hand": "dex", "Stealth": "dex", "Survival": "wis",
}

# name, hit die, saves, skill picks, skill options ("Any" = all), weapon rule, armor text, light, medium,
# heavy, shield, spellcasting ability, unarmored defense, level-table resource labels
CLASSES = [
    ("Barbarian", 12, "Strength", "Constitution", 2, "Animal Handling, Athletics, Intimidation, Nature, Perception, Survival", "All", "Light, Medium, Shields", 1, 1, 0, 1, None, "Barbarian", ("Rages", "Rage damage", "Weapon masteries")),
    ("Bard", 8, "Dexterity", "Charisma", 3, "Any", "Simple", "Light", 1, 0, 0, 0, "Charisma", None, ("Bardic Inspiration die", "", "")),
    ("Cleric", 8, "Wisdom", "Charisma", 2, "History, Insight, Medicine, Persuasion, Religion", "Simple", "Light, Medium, Shields", 1, 1, 0, 1, "Wisdom", None, ("Channel Divinity uses", "", "")),
    ("Druid", 8, "Intelligence", "Wisdom", 2, "Animal Handling, Arcana, Insight, Medicine, Nature, Perception, Religion, Survival", "Simple", "Light, Shields", 1, 0, 0, 1, "Wisdom", None, ("Wild Shape uses", "", "")),
    ("Fighter", 10, "Strength", "Constitution", 2, "Acrobatics, Animal Handling, Athletics, History, Insight, Intimidation, Persuasion, Perception, Survival", "All", "Light, Medium, Heavy, Shields", 1, 1, 1, 1, None, None, ("Second Wind uses", "Weapon masteries", "")),
    ("Monk", 8, "Strength", "Dexterity", 2, "Acrobatics, Athletics, History, Insight, Religion, Stealth", "Monk", "None", 0, 0, 0, 0, None, "Monk", ("Martial Arts die", "Focus Points", "Unarmored Movement")),
    ("Paladin", 10, "Wisdom", "Charisma", 2, "Athletics, Insight, Intimidation, Medicine, Persuasion, Religion", "All", "Light, Medium, Heavy, Shields", 1, 1, 1, 1, "Charisma", None, ("Channel Divinity uses", "", "")),
    ("Ranger", 10, "Strength", "Dexterity", 3, "Animal Handling, Athletics, Insight, Investigation, Nature, Perception, Stealth, Survival", "All", "Light, Medium, Shields", 1, 1, 0, 1, "Wisdom", None, ("Favored Enemy uses", "", "")),
    ("Rogue", 8, "Dexterity", "Intelligence", 4, "Acrobatics, Athletics, Deception, Insight, Intimidation, Investigation, Perception, Persuasion, Sleight of Hand, Stealth", "Rogue", "Light", 1, 0, 0, 0, None, None, ("Sneak Attack", "", "")),
    ("Sorcerer", 6, "Constitution", "Charisma", 2, "Arcana, Deception, Insight, Intimidation, Persuasion, Religion", "Simple", "None", 0, 0, 0, 0, "Charisma", None, ("Sorcery Points", "", "")),
    ("Warlock", 8, "Wisdom", "Charisma", 2, "Arcana, Deception, History, Intimidation, Investigation, Nature, Religion", "Simple", "Light", 1, 0, 0, 0, "Charisma", None, ("Eldritch Invocations", "", "")),
    ("Wizard", 6, "Intelligence", "Wisdom", 2, "Arcana, History, Insight, Investigation, Medicine, Nature, Religion", "Simple", "None", 0, 0, 0, 0, "Intelligence", None, ("", "", "")),
]
WEAPON_TEXT = {"All": "Simple and Martial weapons", "Simple": "Simple weapons",
               "Monk": "Simple weapons and Martial weapons with the Light property",
               "Rogue": "Simple weapons and Martial weapons with the Finesse or Light property"}
TOOLS = {"Bard": "3 Musical Instruments", "Druid": "Herbalism Kit", "Monk": "1 Artisan's Tools or Musical Instrument", "Rogue": "Thieves' Tools"}
# SRD 5.2.1 "Standard Array by Class" (Str, Dex, Con, Int, Wis, Cha)
STANDARD_ARRAY_BY_CLASS = {
    "Barbarian": [15, 13, 14, 10, 12, 8], "Bard": [8, 14, 12, 13, 10, 15], "Cleric": [14, 8, 13, 10, 15, 12],
    "Druid": [8, 12, 14, 13, 15, 10], "Fighter": [15, 14, 13, 8, 10, 12], "Monk": [12, 15, 13, 10, 14, 8],
    "Paladin": [15, 10, 13, 8, 12, 14], "Ranger": [12, 15, 13, 8, 14, 10], "Rogue": [12, 15, 13, 14, 10, 8],
    "Sorcerer": [10, 13, 14, 8, 12, 15], "Warlock": [8, 14, 13, 12, 10, 15], "Wizard": [8, 12, 13, 15, 14, 10],
}
# Trackable class resources: max is a rule the sheet works out, recharge is what a Short Rest restores
# ("all", "one" = one use back, "none" = Long Rest only).
RESOURCES = {
    "Barbarian": [("Rage", {"column": 0}, "one")],
    "Bard": [("Bardic Inspiration", {"ability_mod": "cha", "min": 1}, {"from_level": 5, "then": "all", "else": "none"})],
    "Cleric": [("Channel Divinity", {"column": 0}, "one")],
    "Druid": [("Wild Shape", {"column": 0}, "one")],
    "Fighter": [("Second Wind", {"column": 0}, "one"), ("Action Surge", {"by_level": {"2": 1, "17": 2}}, "all"),
                ("Indomitable", {"by_level": {"9": 1, "13": 2, "17": 3}}, "none")],
    "Monk": [("Focus Points", {"column": 1}, "all")],
    "Paladin": [("Channel Divinity", {"column": 0}, "one"), ("Lay on Hands pool (HP)", {"per_level": 5}, "none")],
    "Ranger": [("Favored Enemy (free Hunter's Mark)", {"column": 0}, "none")],
    "Rogue": [], "Sorcerer": [("Sorcery Points", {"column": 0}, "none")], "Warlock": [],
    "Wizard": [("Arcane Recovery", {"fixed": 1}, "none")],
}

SPECIES = [  # name, size, speed, darkvision, HP per level, extra skill picks, carry multiplier, traits
    ("Dragonborn", "Medium", 30, 60, 0, 0, 1, "Draconic Ancestry (choose dragon colour) · Breath Weapon (replaces one attack; uses = Proficiency Bonus per Long Rest) · Damage Resistance (ancestry type) · Darkvision 60 ft · Draconic Flight at level 5"),
    ("Dwarf", "Medium", 30, 120, 1, 0, 1, "Darkvision 120 ft · Dwarven Resilience (Poison resistance, Advantage vs Poisoned) · Dwarven Toughness (+1 HP per level, included) · Stonecunning (Tremorsense 60 ft, PB uses)"),
    ("Elf", "Medium", 30, 60, 0, 1, 1, "Darkvision 60 ft (Drow 120) · Elven Lineage (Drow, High Elf or Wood Elf; Wood Elf speed 35) · Fey Ancestry (Advantage vs Charmed) · Keen Senses (Insight, Perception or Survival: mark it as a skill pick) · Trance"),
    ("Gnome", "Small", 30, 60, 0, 0, 1, "Darkvision 60 ft · Gnomish Cunning (Advantage on Int, Wis, Cha saves) · Gnomish Lineage (Forest or Rock Gnome)"),
    ("Goliath", "Medium", 35, 0, 0, 0, 2, "Giant Ancestry (choose a boon; PB uses per Long Rest) · Large Form at level 5 · Powerful Build (Advantage to escape Grappled; carry as one size larger, included)"),
    ("Halfling", "Small", 30, 0, 0, 0, 1, "Brave (Advantage vs Frightened) · Halfling Nimbleness · Luck (reroll natural 1s on D20 Tests) · Naturally Stealthy"),
    ("Human", "Medium (or Small)", 30, 0, 0, 1, 1, "Resourceful (Heroic Inspiration after each Long Rest) · Skillful (one extra skill: mark it as a skill pick) · Versatile (an extra Origin feat: choose it above)"),
    ("Orc", "Medium", 30, 120, 0, 0, 1, "Adrenaline Rush (Dash as a Bonus Action + temp HP = PB; PB uses per Short or Long Rest) · Darkvision 120 ft · Relentless Endurance (drop to 1 HP instead of 0, once per Long Rest)"),
    ("Tiefling", "Medium (or Small)", 30, 60, 0, 0, 1, "Darkvision 60 ft · Fiendish Legacy (Abyssal, Chthonic or Infernal) · Otherworldly Presence (Thaumaturgy cantrip)"),
]
SPECIES_RESOURCE = {"Dragonborn": ("Breath Weapon", "none"), "Dwarf": ("Stonecunning", "none"),
                    "Goliath": ("Giant Ancestry boon", "none"), "Orc": ("Adrenaline Rush", "all")}
CHOICE_LABEL = {"Dragonborn": "Dragon ancestor", "Elf": "Elven lineage", "Gnome": "Gnomish lineage", "Goliath": "Giant ancestry", "Tiefling": "Fiendish legacy"}
SPELLS_NOTE = " Spells from this use Intelligence, Wisdom or Charisma (your choice); add them on the Spells tab."
LATER = " At level 3 you learn {a} and at level 5 {b}: always prepared, each cast once per Long Rest without a slot (or with slots)."
GIANT = " Uses: your Proficiency Bonus per Long Rest."
LINEAGES = [  # species, choice, what it gives, darkvision (0 = species default), extra speed
    *[("Dragonborn", f"{c} ({d})", f"Your Breath Weapon deals {d} damage, and you have Resistance to {d} damage.", 0, 0) for c, d in
      [("Black", "Acid"), ("Blue", "Lightning"), ("Brass", "Fire"), ("Bronze", "Lightning"), ("Copper", "Acid"),
       ("Gold", "Fire"), ("Green", "Poison"), ("Red", "Fire"), ("Silver", "Cold"), ("White", "Cold")]],
    ("Elf", "Drow", "Darkvision 120 ft (included). You know the Dancing Lights cantrip." + LATER.format(a="Faerie Fire", b="Darkness") + SPELLS_NOTE, 120, 0),
    ("Elf", "High Elf", "You know the Prestidigitation cantrip; after a Long Rest you can swap it for another Wizard cantrip." + LATER.format(a="Detect Magic", b="Misty Step") + SPELLS_NOTE, 0, 0),
    ("Elf", "Wood Elf", "Speed 35 ft (included). You know the Druidcraft cantrip." + LATER.format(a="Longstrider", b="Pass without Trace") + SPELLS_NOTE, 0, 5),
    ("Gnome", "Forest Gnome", "You know the Minor Illusion cantrip, and always have Speak with Animals prepared: cast it without a slot a number of times equal to your Proficiency Bonus per Long Rest." + SPELLS_NOTE, 0, 0),
    ("Gnome", "Rock Gnome", "You know the Mending and Prestidigitation cantrips. Spend 10 minutes casting Prestidigitation to build a Tiny clockwork device (AC 5, 1 HP) that repeats one Prestidigitation effect when someone touches it as a Bonus Action. Up to three at a time; each falls apart after 8 hours." + SPELLS_NOTE, 0, 0),
    ("Goliath", "Cloud Giant", "Cloud's Jaunt: as a Bonus Action, teleport up to 30 ft to an unoccupied space you can see." + GIANT, 0, 0),
    ("Goliath", "Fire Giant", "Fire's Burn: when you hit with an attack roll and deal damage, add 1d10 Fire damage." + GIANT, 0, 0),
    ("Goliath", "Frost Giant", "Frost's Chill: when you hit with an attack roll and deal damage, add 1d6 Cold damage and reduce the target's Speed by 10 ft until the start of your next turn." + GIANT, 0, 0),
    ("Goliath", "Hill Giant", "Hill's Tumble: when you hit a Large or smaller creature with an attack roll and deal damage, you can knock it Prone." + GIANT, 0, 0),
    ("Goliath", "Stone Giant", "Stone's Endurance: when you take damage, use your Reaction to roll 1d12 + your Constitution modifier and reduce the damage by that much." + GIANT, 0, 0),
    ("Goliath", "Storm Giant", "Storm's Thunder: when a creature within 60 ft damages you, use your Reaction to deal 1d8 Thunder damage to it." + GIANT, 0, 0),
    ("Tiefling", "Abyssal", "Resistance to Poison damage. You know the Poison Spray cantrip." + LATER.format(a="Ray of Sickness", b="Hold Person") + SPELLS_NOTE, 0, 0),
    ("Tiefling", "Chthonic", "Resistance to Necrotic damage. You know the Chill Touch cantrip." + LATER.format(a="False Life", b="Ray of Enfeeblement") + SPELLS_NOTE, 0, 0),
    ("Tiefling", "Infernal", "Resistance to Fire damage. You know the Fire Bolt cantrip." + LATER.format(a="Hellish Rebuke", b="Darkness") + SPELLS_NOTE, 0, 0),
]

FEATS = [  # origin feats
    ("Alert", "Add your Proficiency Bonus to Initiative (included). Swap Initiative with a willing ally after rolling."),
    ("Magic Initiate (Cleric)", "Two Cleric cantrips and one level 1 Cleric spell (cast once free per Long Rest). Choose Int, Wis or Cha as the ability."),
    ("Magic Initiate (Druid)", "Two Druid cantrips and one level 1 Druid spell (cast once free per Long Rest). Choose Int, Wis or Cha as the ability."),
    ("Magic Initiate (Wizard)", "Two Wizard cantrips and one level 1 Wizard spell (cast once free per Long Rest). Choose Int, Wis or Cha as the ability."),
    ("Savage Attacker", "Once per turn when you hit with a weapon, roll its damage dice twice and use either roll."),
    ("Skilled", "Proficiency in any three skills or tools (skills: mark them as skill picks)."),
    ("Crafter", "Proficiency with three kinds of Artisan's Tools. 20% off nonmagical items you buy. After a Long Rest you can make one simple item (e.g. a ladder, torch or rope) with tools you're proficient with; it lasts until your next Long Rest."),
    ("Healer", "With a Healer's Kit, use an action to let a creature within 5 ft spend one of its Hit Point Dice: it heals that roll + your Proficiency Bonus. When you roll dice to restore HP (this or a spell), reroll any 1s."),
    ("Lucky", "Luck Points equal to your Proficiency Bonus, back after a Long Rest. Spend one to give yourself Advantage on a d20 roll, or to give Disadvantage to an attack roll against you."),
    ("Musician", "Proficiency with three Musical Instruments. When you finish a Short or Long Rest, play a song: up to your Proficiency Bonus of allies who hear it gain Heroic Inspiration."),
    ("Tavern Brawler", "Unarmed strikes deal 1d4 + Str Bludgeoning damage, and you reroll 1s on that damage. You're proficient with improvised weapons. Once per turn when you hit with an unarmed strike, you can also push the target 5 ft away."),
    ("Tough", "Your Hit Point maximum goes up by 2 for every level you have (included in your HP)."),
]
FIGHTING_STYLES = ["Archery", "Defense", "Great Weapon Fighting", "Two-Weapon Fighting"]

ARMOR = [  # name, category, base AC, max Dex bonus, Str required, stealth
    ("None (unarmored)", "None", 10, 99, 0, ""), ("Padded Armor", "Light", 11, 99, 0, "Disadvantage"),
    ("Leather Armor", "Light", 11, 99, 0, ""), ("Studded Leather Armor", "Light", 12, 99, 0, ""),
    ("Hide Armor", "Medium", 12, 2, 0, ""), ("Chain Shirt", "Medium", 13, 2, 0, ""), ("Scale Mail", "Medium", 14, 2, 0, "Disadvantage"),
    ("Breastplate", "Medium", 14, 2, 0, ""), ("Half Plate Armor", "Medium", 15, 2, 0, "Disadvantage"),
    ("Ring Mail", "Heavy", 14, 0, 0, "Disadvantage"), ("Chain Mail", "Heavy", 16, 0, 13, "Disadvantage"),
    ("Splint Armor", "Heavy", 17, 0, 15, "Disadvantage"), ("Plate Armor", "Heavy", 18, 0, 15, "Disadvantage"),
]
MASTERY = [  # SRD 5.2.1, paraphrased
    ("Cleave", "On a melee hit, make one more attack against a second creature within 5 ft of the first and in your reach; no ability modifier on its damage (once per turn)."),
    ("Graze", "On a miss, the target still takes damage equal to the ability modifier you attacked with."),
    ("Nick", "The extra attack from the Light property can be part of the Attack action instead of a Bonus Action (once per turn)."),
    ("Push", "On a hit, push a Large or smaller creature up to 10 ft straight away from you."),
    ("Sap", "On a hit, the target has Disadvantage on its next attack roll before the start of your next turn."),
    ("Slow", "On a hit that deals damage, cut the target's Speed by 10 ft until the start of your next turn."),
    ("Topple", "On a hit, the target makes a Con save (DC 8 + your attack modifier + Proficiency Bonus) or falls Prone."),
    ("Vex", "On a hit that deals damage, you have Advantage on your next attack roll against that creature before the end of your next turn."),
]
ABILITY_USES = {
    "str": "Athletics · melee weapon attacks and damage (non-finesse) · thrown melee weapons like javelins · Grapple/Shove DC · carrying (Str × 15 lb) · long jump Str score ft, high jump 3 + mod ft · heavy armor's minimum Str",
    "dex": "Acrobatics, Sleight of Hand, Stealth · AC in light, medium (up to +2) or no armor · Initiative · ranged weapons · finesse weapons when higher than Str",
    "con": "Hit Points every level · healing from Hit Point Dice · keeping Concentration (Con save, DC 10 or half the damage, whichever is higher)",
    "int": "Arcana, History, Investigation, Nature, Religion · Wizard spellcasting",
    "wis": "Animal Handling, Insight, Medicine, Perception, Survival · Passive Perception · Cleric, Druid and Ranger spellcasting · Monk AC",
    "cha": "Deception, Intimidation, Performance, Persuasion · Bard, Paladin, Sorcerer and Warlock spellcasting · Bardic Inspiration uses",
}
ALIGNMENTS = ["Lawful Good", "Neutral Good", "Chaotic Good", "Lawful Neutral", "Neutral", "Chaotic Neutral", "Lawful Evil", "Neutral Evil", "Chaotic Evil"]


def num(x: str):
    x = x.strip().replace("<br>", "")
    if x in ("—", ""):
        return 0
    try:
        return int(x)
    except ValueError:
        return x


def level_rows(cname: str, rows: list[list[str]]) -> list[dict]:
    """One class's level table, as the sheet reads it (build.py's "class level table")."""
    out = []
    for cells in rows:
        lv, feats, d = int(cells[0]), cells[2], cells[3:]
        res, cant, prep, slots, pact, pactlv, mast = ["", "", ""], 0, 0, [0] * 9, 0, 0, 0
        if cname == "Barbarian":
            res = [num(d[0]), d[1], num(d[2])]; mast = num(d[2])
        elif cname in ("Bard", "Cleric", "Druid", "Sorcerer"):
            res[0] = d[0] if cname == "Bard" else num(d[0]); cant = num(d[1]); prep = num(d[2]); slots = [num(x) for x in d[3:12]]
        elif cname == "Fighter":
            res = [num(d[0]), num(d[1]), ""]; mast = num(d[1])
        elif cname == "Monk":
            res = [d[0], num(d[1]), d[2] if d[2] != "—" else "+0 ft."]
        elif cname in ("Paladin", "Ranger"):
            res[0] = num(d[0]); prep = num(d[1]); slots = [num(x) for x in d[2:7]] + [0] * 4; mast = 2
        elif cname == "Rogue":
            res[0] = d[0]; mast = 2
        elif cname == "Warlock":
            res[0] = num(d[0]); cant = num(d[1]); prep = num(d[2]); pact = num(d[3]); pactlv = num(d[4])
        elif cname == "Wizard":
            cant = num(d[0]); prep = num(d[1]); slots = [num(x) for x in d[2:11]]
        out.append({"level": lv, "features": [] if feats.strip() in ("—", "") else [f.strip() for f in feats.split(",")],
                    "columns": res, "cantrips": cant, "prepared": prep, "slots": slots, "pact_slots": pact,
                    "pact_level": pactlv, "masteries": mast})
    return out


def gear(opts: dict) -> dict:
    return {k: [{"item": n, "qty": q} for n, q in items] for k, items in opts.items()}


def main(tools: Path) -> None:
    sys.path.insert(0, str(tools))
    from origins import BACKGROUNDS, BG_GEAR, CLASS_GEAR  # noqa: E402

    srd = json.loads((tools / "srd.json").read_text())
    items = json.loads((tools / "items.json").read_text())
    rules = {
        "meta": {
            "title": "Character builder rules (2024)",
            "source": "SRD 5.2.1 (CC BY 4.0) and the 2024 Player's Handbook (entries marked srd: false), as in the dnd repo's character sheet",
        },
        "abilities": ABILITIES,
        "skills": SKILLS,
        "ability_uses": ABILITY_USES,
        "point_buy": {"budget": 27, "min": 8, "max": 15, "costs": {"8": 0, "9": 1, "10": 2, "11": 3, "12": 4, "13": 5, "14": 7, "15": 9}},
        "standard_array": [15, 14, 13, 12, 10, 8],
        "rolled": {"min": 3, "max": 18},
        "score_cap": 20,
        "alignments": ALIGNMENTS,
        "classes": [
            {"name": c[0], "hit_die": c[1], "saves": [FULL[c[2]], FULL[c[3]]], "skill_picks": c[4],
             "skill_options": "any" if c[5] == "Any" else [s.strip() for s in c[5].split(",")],
             "weapon_rule": c[6], "weapons_text": WEAPON_TEXT[c[6]], "armor_text": c[7],
             "armor_training": {"light": bool(c[8]), "medium": bool(c[9]), "heavy": bool(c[10]), "shield": bool(c[11])},
             "spell_ability": FULL[c[12]] if c[12] else None, "unarmored_defense": c[13], "tools_text": TOOLS.get(c[0], ""),
             "column_labels": list(c[14]), "standard_array": STANDARD_ARRAY_BY_CLASS[c[0]],
             "resources": [{"name": n, "max": m, "short_rest": r} for n, m, r in RESOURCES[c[0]]],
             "fighting_style_level": 1 if c[0] == "Fighter" else 2 if c[0] in ("Paladin", "Ranger") else None,
             "starting_gear": gear(CLASS_GEAR[c[0]]), "levels": level_rows(c[0], srd[c[0]]), "srd": True}
            for c in CLASSES
        ],
        "species": [
            {"name": s[0], "size": s[1], "speed": s[2], "darkvision": s[3], "hp_per_level": s[4], "extra_skill_picks": s[5],
             "carry_multiplier": s[6], "traits": s[7], "choice_label": CHOICE_LABEL.get(s[0]),
             "choices": [{"name": l[1], "text": l[2], "darkvision": l[3], "extra_speed": l[4]} for l in LINEAGES if l[0] == s[0]],
             "resource": ({"name": SPECIES_RESOURCE[s[0]][0], "max": {"proficiency_bonus": True}, "short_rest": SPECIES_RESOURCE[s[0]][1]}
                          if s[0] in SPECIES_RESOURCE else None),
             "heroic_inspiration_on_long_rest": s[0] == "Human", "extra_origin_feat": s[0] == "Human", "srd": True}
            for s in SPECIES
        ],
        "backgrounds": [
            {"name": b[0], "abilities": [FULL[b[1]], FULL[b[2]], FULL[b[3]]], "feat": b[4], "skills": [b[5], b[6]], "tool": b[7],
             "starting_gear": gear(BG_GEAR[b[0]]), "srd": b[0] in SRD_BACKGROUNDS}
            for b in BACKGROUNDS
        ],
        "feats": [{"name": n, "type": "Origin", "text": t, "srd": n in SRD_FEATS} for n, t in FEATS],
        "fighting_styles": FIGHTING_STYLES,
        "armor": [{"name": a[0], "category": a[1], "base_ac": a[2], "max_dex": a[3], "str_required": a[4], "stealth": a[5]} for a in ARMOR],
        "weapons": [{"name": w["name"], "damage": w["dmg"], "properties": w["props"], "mastery": w["mastery"], "category": w["cat"],
                     "kind": w["kind"], "weight": w["weight"], "cost": w["cost"]} for w in srd["weapons"]],
        "masteries": [{"name": n, "text": t} for n, t in MASTERY],
        "items": items,
    }
    OUT.write_bytes(gzip.compress(json.dumps(rules, ensure_ascii=False, separators=(",", ":")).encode(), 9, mtime=0))
    print(f"wrote {OUT}: {len(rules['classes'])} classes, {len(rules['species'])} species, {len(rules['backgrounds'])} backgrounds, "
          f"{len(rules['feats'])} feats, {len(rules['weapons'])} weapons, {len(rules['items'])} items")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(Path(sys.argv[1]))
