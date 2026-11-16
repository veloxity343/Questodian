"""5e rules numbers and helper calculations, for the 2014 (SRD 5.1) and 2024 (SRD 5.2.1) rules.

Where the editions differ, functions take an ``edition`` ("2014" or "2024").
"""

from __future__ import annotations

import math

ABILITIES = ["str", "dex", "con", "int", "wis", "cha"]
ABILITY_NAMES = {
    "str": "Strength",
    "dex": "Dexterity",
    "con": "Constitution",
    "int": "Intelligence",
    "wis": "Wisdom",
    "cha": "Charisma",
}

SKILLS = {
    "acrobatics": "dex",
    "animal-handling": "wis",
    "arcana": "int",
    "athletics": "str",
    "deception": "cha",
    "history": "int",
    "insight": "wis",
    "intimidation": "cha",
    "investigation": "int",
    "medicine": "wis",
    "nature": "int",
    "perception": "wis",
    "performance": "cha",
    "persuasion": "cha",
    "religion": "int",
    "sleight-of-hand": "dex",
    "stealth": "dex",
    "survival": "wis",
}

CONDITIONS = [
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
]

# Character advancement: XP required to reach each level.
XP_BY_LEVEL = [0, 300, 900, 2700, 6500, 14000, 23000, 34000, 48000, 64000,
               85000, 100000, 120000, 140000, 165000, 195000, 225000, 265000, 305000, 355000]

CR_XP = {
    "0": 10, "1/8": 25, "1/4": 50, "1/2": 100, "1": 200, "2": 450, "3": 700, "4": 1100,
    "5": 1800, "6": 2300, "7": 2900, "8": 3900, "9": 5000, "10": 5900, "11": 7200,
    "12": 8400, "13": 10000, "14": 11500, "15": 13000, "16": 15000, "17": 18000,
    "18": 20000, "19": 22000, "20": 25000, "21": 33000, "22": 41000, "23": 50000,
    "24": 62000, "25": 75000, "26": 90000, "27": 105000, "28": 120000, "29": 135000,
    "30": 155000,
}

# XP budget per character by party level: (low, moderate, high). SRD 5.2.1 "Combat Encounter
# Difficulty". The SRD 5.1 has no encounter-building table, so 2014 campaigns use this one too
# (monster XP values are the same scale in both editions).
XP_BUDGET = [
    (50, 75, 100), (100, 150, 200), (150, 225, 400), (250, 375, 500), (500, 750, 1100),
    (600, 1000, 1400), (750, 1300, 1700), (1000, 1700, 2100), (1300, 2000, 2600), (1600, 2300, 3100),
    (1900, 2900, 4100), (2200, 3700, 4700), (2600, 4200, 5400), (2900, 4900, 6200), (3300, 5400, 7800),
    (3800, 6100, 9800), (4500, 7200, 11700), (5000, 8700, 14200), (5500, 10700, 17200), (6400, 13200, 22000),
]
DIFFICULTIES = ("low", "moderate", "high")

POINT_BUY_COST = {8: 0, 9: 1, 10: 2, 11: 3, 12: 4, 13: 5, 14: 7, 15: 9}
POINT_BUY_BUDGET = 27
STANDARD_ARRAY = [15, 14, 13, 12, 10, 8]

HIT_DICE = {
    "barbarian": 12, "bard": 8, "cleric": 8, "druid": 8, "fighter": 10, "monk": 8,
    "paladin": 10, "ranger": 10, "rogue": 8, "sorcerer": 6, "warlock": 8, "wizard": 6,
}

# Multiclass spellcaster slot table (also the full-caster table), indexed by caster level.
SPELL_SLOTS = [
    [],
    [2], [3], [4, 2], [4, 3], [4, 3, 2], [4, 3, 3], [4, 3, 3, 1], [4, 3, 3, 2],
    [4, 3, 3, 3, 1], [4, 3, 3, 3, 2], [4, 3, 3, 3, 2, 1], [4, 3, 3, 3, 2, 1],
    [4, 3, 3, 3, 2, 1, 1], [4, 3, 3, 3, 2, 1, 1], [4, 3, 3, 3, 2, 1, 1, 1],
    [4, 3, 3, 3, 2, 1, 1, 1], [4, 3, 3, 3, 2, 1, 1, 1, 1], [4, 3, 3, 3, 3, 1, 1, 1, 1],
    [4, 3, 3, 3, 3, 2, 1, 1, 1], [4, 3, 3, 3, 3, 2, 2, 1, 1],
]

CASTER_PROGRESSION = {
    "bard": 1, "cleric": 1, "druid": 1, "sorcerer": 1, "wizard": 1,
    "paladin": 0.5, "ranger": 0.5,
}


def ability_modifier(score: int) -> int:
    return math.floor((int(score) - 10) / 2)


def proficiency_bonus(level: int) -> int:
    level = max(1, min(int(level), 20))
    return 2 + (level - 1) // 4


def level_for_xp(xp: int) -> int:
    level = 1
    for i, needed in enumerate(XP_BY_LEVEL, start=1):
        if xp >= needed:
            level = i
    return level


def xp_for_next_level(level: int) -> int | None:
    return XP_BY_LEVEL[level] if 1 <= level < 20 else None


def point_buy_cost(scores: list[int]) -> int:
    total = 0
    for s in scores:
        if s not in POINT_BUY_COST:
            raise ValueError(f"point-buy scores must be 8-15, got {s}")
        total += POINT_BUY_COST[s]
    return total


def carrying_capacity(strength: int, size: str = "medium") -> dict:
    mult = {"tiny": 0.5, "small": 1, "medium": 1, "large": 2, "huge": 4, "gargantuan": 8}.get(size.lower(), 1)
    cap = strength * 15 * mult
    return {"capacity": cap, "push_drag_lift": cap * 2}


def jump(strength: int, running: bool = True) -> dict:
    mod = ability_modifier(strength)
    return {
        "long_jump_ft": strength if running else strength // 2,
        "high_jump_ft": max(0, 3 + mod) if running else max(0, 3 + mod) // 2,
    }


def caster_level(classes: list[dict], edition: str = "2014") -> int:
    """Combined caster level for the multiclass slot table (warlock excluded)."""
    total = 0.0
    for c in classes:
        weight = CASTER_PROGRESSION.get(str(c.get("name", "")).lower(), 0)
        lvl = int(c.get("level") or 0)
        if weight == 0.5 and len(classes) == 1:
            # Single-class half casters round up: from 2nd level in 2014 (classes/paladin),
            # from 1st level in 2024, where a level 1 Paladin or Ranger has two 1st-level slots.
            total += math.ceil(lvl / 2) if lvl >= 2 or edition == "2024" else 0
        else:
            total += math.floor(lvl * weight)
    return int(total)


def spell_slots_for(classes: list[dict], edition: str = "2014") -> list[int]:
    lvl = caster_level(classes, edition)
    return list(SPELL_SLOTS[min(lvl, 20)]) if lvl > 0 else []


def is_warlock_only(classes: list[dict]) -> bool:
    """A single-class warlock, whose slots (Pact Magic) all come back on a short rest."""
    return bool(classes) and all(str(c.get("name", "")).lower() == "warlock" for c in classes)


def encounter_xp(monster_crs: list[str]) -> int:
    return sum(CR_XP.get(str(cr), 0) for cr in monster_crs)


def total_level(character: dict) -> int:
    return sum(int(c.get("level") or 0) for c in character.get("classes", [])) or 1


# What each level of exhaustion does under the 2014 rules (conditions/exhaustion, SRD 5.1).
# Effects are cumulative.
EXHAUSTION_2014 = [
    "Disadvantage on ability checks",
    "Speed halved",
    "Disadvantage on attack rolls and saving throws",
    "Hit point maximum halved",
    "Speed reduced to 0",
    "Death",
]


def exhaustion_effects(level: int, edition: str = "2014") -> dict:
    """The mechanical effects of an exhaustion level (conditions/exhaustion in each SRD).

    2024: D20 Tests are reduced by 2 per level, Speed by 5 ft per level; level 6 is death.
    2014: the cumulative six-step table above.
    """
    level = max(0, min(int(level or 0), 6))
    if edition == "2024":
        effects = [f"D20 Tests -{2 * level}", f"Speed -{5 * level} ft."] if level else []
        if level == 6:
            effects.append("Death")
        return {"d20_penalty": 2 * level, "effects": effects}
    return {"d20_penalty": 0, "effects": EXHAUSTION_2014[:level]}


def exhausted_speed(speed: int, level: int, edition: str = "2014") -> int:
    level = max(0, min(int(level or 0), 6))
    if edition == "2024":
        return max(0, speed - 5 * level)
    if level >= 5:
        return 0
    return speed // 2 if level >= 2 else speed


def derive(character: dict, edition: str = "2014") -> dict:
    """Compute modifiers, bonuses and passives for a character sheet.

    The 2024 exhaustion penalty applies to every D20 Test (ability checks, saves, skills,
    initiative, spell attacks). The spell save DC isn't a roll. Passive scores aren't rolls
    either, so they're left alone in 2024 (our reading; the SRD doesn't say). In 2014,
    exhaustion 1+ gives Disadvantage on ability checks, which is -5 to passive scores
    (rules/ability-checks).
    """
    abilities = character.get("abilities", {})
    mods = {a: ability_modifier(abilities.get(a, 10)) for a in ABILITIES}
    level = total_level(character)
    pb = proficiency_bonus(level)
    exhaustion = exhaustion_effects(character.get("exhaustion", 0), edition)
    penalty = exhaustion["d20_penalty"]
    save_profs = set(character.get("save_proficiencies", []))
    saves = {a: mods[a] + (pb if a in save_profs else 0) for a in ABILITIES}
    skills = {}
    skill_profs = character.get("skills", {})
    jack = bool(character.get("jack_of_all_trades"))
    for skill, ability in SKILLS.items():
        prof = skill_profs.get(skill, "none")
        bonus = mods[ability]
        if prof == "expertise":
            bonus += pb * 2
        elif prof == "proficient":
            bonus += pb
        elif prof == "half" or jack:
            bonus += pb // 2
        skills[skill] = bonus
    # Initiative is a Dexterity check. 2014 Jack of All Trades covers any ability check
    # without proficiency (classes/bard); the 2024 feature only covers skills.
    # Plain ability checks have no proficiency, so 2014 Jack of All Trades adds to them.
    jack_bonus = pb // 2 if jack and edition != "2024" else 0
    checks = {a: mods[a] + jack_bonus - penalty for a in ABILITIES}
    initiative = mods["dex"] + int(character.get("initiative_misc") or 0) + jack_bonus
    passive_minus = 5 if edition != "2024" and int(character.get("exhaustion") or 0) >= 1 else 0
    spell_ability = character.get("spellcasting", {}).get("ability") or None
    spell = {}
    if spell_ability in mods:
        spell = {"save_dc": 8 + pb + mods[spell_ability], "attack_bonus": pb + mods[spell_ability] - penalty}
    return {
        "level": level,
        "proficiency_bonus": pb,
        "modifiers": mods,
        "checks": checks,
        "saves": {a: v - penalty for a, v in saves.items()},
        "skills": {k: v - penalty for k, v in skills.items()},
        "initiative": initiative - penalty,
        "passive_perception": 10 + skills["perception"] - passive_minus,
        "passive_investigation": 10 + skills["investigation"] - passive_minus,
        "passive_insight": 10 + skills["insight"] - passive_minus,
        "spell": spell,
        "speed": exhausted_speed(int(character.get("speed") or 0), character.get("exhaustion", 0), edition),
        "exhaustion": exhaustion["effects"],
        # Every SRD species and race is Small or Medium, which carry the same (Str x 15).
        "carrying_capacity": abilities.get("str", 10) * 15,
        "next_level_xp": xp_for_next_level(level),
    }


def encounter_budget(party_size: int, party_level: int) -> dict[str, int]:
    """Total XP budgets for a party: {"low": .., "moderate": .., "high": ..}."""
    row = XP_BUDGET[max(1, min(int(party_level), 20)) - 1]
    return {d: v * max(1, int(party_size)) for d, v in zip(DIFFICULTIES, row)}


def encounter_difficulty(monster_xp: int, party_size: int, party_level: int) -> str:
    """"trivial" below the low budget, "deadly" beyond high, else the budget it fits."""
    budget = encounter_budget(party_size, party_level)
    if monster_xp <= 0 or monster_xp < budget["low"] * 0.5:
        return "trivial"
    for d in DIFFICULTIES:
        if monster_xp <= budget[d]:
            return d
    return "deadly"
