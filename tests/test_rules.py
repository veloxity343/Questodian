from questodian import rules
from questodian.repo import normalize


def test_modifiers_and_proficiency():
    assert [rules.ability_modifier(s) for s in (1, 8, 9, 10, 11, 12, 20, 30)] == [-5, -1, -1, 0, 0, 1, 5, 10]
    assert [rules.proficiency_bonus(lv) for lv in (1, 4, 5, 8, 9, 13, 17, 20)] == [2, 2, 3, 3, 4, 5, 6, 6]


def test_levels_from_xp():
    assert rules.level_for_xp(0) == 1
    assert rules.level_for_xp(299) == 1
    assert rules.level_for_xp(300) == 2
    assert rules.level_for_xp(355000) == 20
    assert rules.xp_for_next_level(1) == 300
    assert rules.xp_for_next_level(20) is None


def test_point_buy():
    assert rules.point_buy_cost([15, 15, 15, 8, 8, 8]) == 27
    assert rules.point_buy_cost([8] * 6) == 0


def test_spell_slots():
    assert rules.spell_slots_for([{"name": "Wizard", "level": 5}]) == [4, 3, 2]
    assert rules.spell_slots_for([{"name": "Paladin", "level": 1}]) == []
    # 2024 Paladins and Rangers cast from level 1 (classes/paladin, classes/ranger in SRD 5.2.1)
    assert rules.spell_slots_for([{"name": "Paladin", "level": 1}], "2024") == [2]
    assert rules.spell_slots_for([{"name": "Ranger", "level": 3}], "2024") == [3]
    assert rules.spell_slots_for([{"name": "Paladin", "level": 2}]) == [2]
    assert rules.spell_slots_for([{"name": "Paladin", "level": 20}]) == [4, 3, 3, 3, 2]
    # multiclass: wizard 3 + paladin 4 -> caster level 3 + 2 = 5
    assert rules.spell_slots_for([{"name": "Wizard", "level": 3}, {"name": "Paladin", "level": 4}]) == [4, 3, 2]
    assert rules.spell_slots_for([{"name": "Fighter", "level": 10}]) == []


def test_derive_character():
    ch = normalize(
        "characters",
        {
            "classes": [{"name": "Rogue", "level": 5}],
            "abilities": {"str": 8, "dex": 18, "con": 14, "int": 12, "wis": 13, "cha": 10},
            "save_proficiencies": ["dex", "int"],
            "skills": {"stealth": "expertise", "perception": "proficient"},
            "spellcasting": {"ability": "int"},
        },
    )
    d = rules.derive(ch)
    assert d["level"] == 5 and d["proficiency_bonus"] == 3
    assert d["saves"]["dex"] == 7 and d["saves"]["str"] == -1
    assert d["skills"]["stealth"] == 4 + 6
    assert d["skills"]["perception"] == 1 + 3
    assert d["passive_perception"] == 14
    assert d["initiative"] == 4
    assert d["spell"] == {"save_dc": 12, "attack_bonus": 4}


def test_jack_of_all_trades():
    ch = normalize("characters", {"classes": [{"name": "Bard", "level": 2}], "jack_of_all_trades": True})
    assert rules.derive(ch)["skills"]["arcana"] == 1


def test_derive_exhaustion_by_edition():
    ch = normalize("characters", {"abilities": {"dex": 14, "wis": 12}, "exhaustion": 2, "speed": 30,
                                  "spellcasting": {"ability": "wis"}})
    old, new = rules.derive(ch, "2014"), rules.derive(ch, "2024")
    # 2014: speed halved from level 2; no numeric penalty to rolls
    assert old["speed"] == 15 and old["initiative"] == 2 and old["exhaustion"][1] == "Speed halved"
    # 2024: D20 Tests -2 per level, Speed -5 ft per level; passives and the save DC unaffected
    assert new["speed"] == 20 and new["initiative"] == 2 - 4 and new["saves"]["dex"] == 2 - 4
    assert new["skills"]["perception"] == 1 - 4 and new["passive_perception"] == 11
    assert new["spell"] == {"save_dc": 11, "attack_bonus": 3 - 4}
    assert rules.derive({**ch, "exhaustion": 5}, "2014")["speed"] == 0
    # 2014 exhaustion 1+: Disadvantage on ability checks, so passives are 5 lower
    assert old["passive_perception"] == 11 - 5 and new["checks"]["dex"] == 2 - 4


def test_jack_of_all_trades_initiative_is_2014_only():
    ch = normalize("characters", {"classes": [{"name": "Bard", "level": 5}], "abilities": {"dex": 14},
                                  "jack_of_all_trades": True})
    assert rules.derive(ch, "2014")["initiative"] == 2 + 1  # half of +3, rounded down
    assert rules.derive(ch, "2014")["checks"]["str"] == 0 + 1
    assert rules.derive(ch, "2024")["initiative"] == 2
