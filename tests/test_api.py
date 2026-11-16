import pytest


@pytest.fixture
def api(client):
    def call(method, path, body=None, headers=None, raw=False):
        res = client.request(method, path, json=body, headers=headers)
        return res.status_code, (res.text if raw else res.json())

    return call


def test_ui_and_spa_fallback(api):
    status, html = api("GET", "/", raw=True)
    assert status == 200 and "Questodian" in html
    assert api("GET", "/assets/app.js", raw=True)[0] == 200
    assert "Questodian" in api("GET", "/characters/abc", raw=True)[1]  # client-side route
    assert api("GET", "/api/nope")[0] == 404
    assert "Questodian" in api("GET", "/../pyproject.toml", raw=True)[1]


def test_openapi_docs(api):
    status, spec = api("GET", "/openapi.json")
    assert status == 200 and "/api/characters/{doc_id}" in spec["paths"]


def test_meta_and_roll(api):
    status, meta = api("GET", "/api/meta")
    assert status == 200 and "perception" in meta["skills"] and len(meta["conditions"]) == 15
    status, r = api("POST", "/api/roll", {"expression": "2d6+3"})
    assert status == 200 and 5 <= r["total"] <= 15
    status, r = api("GET", "/api/roll?expr=d20&times=3")
    assert len(r["results"]) == 3
    status, r = api("POST", "/api/roll", {"expression": "2d"})
    assert status == 400 and "detail" in r


def test_srd_search(api):
    _, r = api("GET", "/api/srd/search?q=fireball&category=spells")
    assert r["results"][0]["name"] == "Fireball"
    _, r = api("GET", "/api/srd/search?category=spells&level=0&classes=Wizard&limit=500")
    assert r["total"] > 5 and all(e["data"]["level"] == 0 and "Wizard" in e["data"]["classes"] for e in r["results"])
    _, e = api("GET", "/api/srd/entry?id=conditions/prone")
    assert "prone" in e["body"].lower()
    assert api("GET", "/api/srd/entry?id=nope")[0] == 404


def test_crud_and_cross_origin_block(api):
    status, c = api("POST", "/api/campaigns", {"name": "Tomb"})
    assert status == 201
    _, got = api("PUT", f"/api/campaigns/{c['id']}", {"notes": "traps"})
    assert got["notes"] == "traps" and got["name"] == "Tomb"
    _, lst = api("GET", "/api/campaigns")
    assert len(lst["campaigns"]) == 1
    status, _ = api("POST", "/api/campaigns", {"name": "evil"}, headers={"Origin": "http://evil.example"})
    assert status == 403
    assert api("DELETE", f"/api/campaigns/{c['id']}")[0] == 200
    assert api("GET", f"/api/campaigns/{c['id']}")[0] == 404


def test_encounter_flow(api):
    _, camp = api("POST", "/api/campaigns", {"name": "C"})
    _, ch = api("POST", "/api/characters", {"name": "Hero", "campaign_id": camp["id"], "hp": {"max": 20, "current": 20, "temp": 0}})
    _, e = api("POST", "/api/encounters", {"campaign_id": camp["id"], "name": "Ambush"})
    path = f"/api/encounters/{e['id']}/action"
    _, e = api("POST", path, {"action": "add_character", "character_ids": [ch["id"]]})
    _, e = api("POST", path, {"action": "add_monster", "srd_id": "monsters/goblin", "count": 2})
    names = [c["name"] for c in e["combatants"]]
    assert names == ["Hero", "Goblin 1", "Goblin 2"]
    assert e["combatants"][1]["xp"] == 50
    _, e = api("POST", path, {"action": "roll_initiative"})
    assert all(c["initiative"] for c in e["combatants"])
    _, e = api("POST", path, {"action": "next"})
    assert e["active"] and e["round"] == 1
    hero = next(c for c in e["combatants"] if c["kind"] == "pc")
    _, e = api("POST", path, {"action": "hp", "id": hero["id"], "amount": -8})
    _, sheet = api("GET", f"/api/characters/{ch['id']}")
    assert sheet["hp"]["current"] == 12  # synced back to the character sheet
    for _ in range(3):
        _, e = api("POST", path, {"action": "next"})
    assert e["round"] == 2
    _, e = api("POST", path, {"action": "end"})
    assert not e["active"]
    assert any("Combat begins" in x["text"] for x in e["log"])
    assert api("POST", path, {"action": "explode"})[0] == 422
    _, listed = api("GET", f"/api/encounters?campaign_id={camp['id']}")
    assert [x["name"] for x in listed["encounters"]] == ["Ambush"]


def test_encounter_uses_campaign_edition_and_homebrew(api):
    _, camp = api("POST", "/api/campaigns", {"name": "Seven Bells", "edition": "2024"})
    _, e = api("POST", "/api/encounters", {"campaign_id": camp["id"]})
    path = f"/api/encounters/{e['id']}/action"
    _, e = api("POST", path, {"action": "add_monster", "srd_id": "monsters/zombie"})
    z = e["combatants"][0]
    assert (z["ac"], z["hp"], z["init_bonus"], z["xp"]) == (8, 15, -2, 50)
    _, m = api("POST", "/api/monsters", {"campaign_id": camp["id"], "name": "The Bell-Ringer", "ac": 12, "hp": 40, "xp": 200})
    _, e = api("POST", path, {"action": "add_monster", "monster_id": m["id"]})
    assert e["combatants"][1]["name"] == "The Bell-Ringer" and e["combatants"][1]["monster_id"] == m["id"]
    _, e = api("POST", path, {"action": "add_monster", "srd_id": "monsters/zombie"})
    assert [c["name"] for c in e["combatants"]] == ["Zombie 1", "The Bell-Ringer", "Zombie 2"]
    _, e = api("POST", path, {"action": "roll_hp"})
    zombies = [c for c in e["combatants"] if c["srd_id"]]
    assert all(c["hp"] == c["max_hp"] and 8 <= c["hp"] <= 22 for c in zombies)  # 2d8 + 6
    _, npc = api("POST", "/api/npcs", {"campaign_id": camp["id"], "name": "Pell", "stat_ref": "srd:monsters/commoner"})
    _, e = api("POST", path, {"action": "add_npc", "npc_id": npc["id"]})
    pell = e["combatants"][-1]
    assert pell["kind"] == "npc" and pell["npc_id"] == npc["id"] and pell["hp"] == 4
    _, budget = api("GET", "/api/encounter-budget?party_size=4&party_level=1&xp=200")
    assert budget == {"budget": {"low": 200, "moderate": 300, "high": 400}, "difficulty": "low"}


def test_projection_relay(api):
    _, p = api("GET", "/api/projection")
    assert p["id"] == "" and p["seq"] == 0
    _, p = api("PUT", "/api/projection", {"id": "h1", "zoom": 2, "x": 10, "y": -5})
    assert p["seq"] == 1
    _, p = api("GET", "/api/projection")
    assert (p["id"], p["zoom"], p["x"], p["seq"]) == ("h1", 2, 10, 1)
    assert api("PUT", "/api/projection", {"id": "h1", "zoom": 0})[0] == 422


def test_srd_editions(api):
    _, r = api("GET", "/api/srd/search?q=zombie&category=monsters&edition=2024")
    assert r["results"][0]["name"] == "Zombie"
    _, z = api("GET", "/api/srd/entry?id=monsters/zombie&edition=2024")
    assert "Undead Fortitude" in z["body"]
    _, cats = api("GET", "/api/srd/categories?edition=2024")
    ids = [c["id"] for c in cats["categories"]]
    assert "species" in ids and "races" not in ids and cats["meta"]["edition"] == "2024"
    assert api("GET", "/api/srd/search?q=x&edition=1999")[0] == 400


def test_handout_image_roundtrip(api, client):
    _, camp = api("POST", "/api/campaigns", {"name": "C"})
    _, h = api("POST", "/api/handouts", {"campaign_id": camp["id"], "title": "Map", "folder": "Maps"})
    assert api("GET", f"/api/handouts/{h['id']}/file")[0] == 404
    png = b"\x89PNG\r\n\x1a\nfake"
    res = client.put(f"/api/handouts/{h['id']}/file", content=png, headers={"content-type": "image/png"})
    assert res.status_code == 200 and res.json()["mime"] == "image/png"
    got = client.get(f"/api/handouts/{h['id']}/file")
    assert got.content == png
    assert client.put(f"/api/handouts/{h['id']}/file", content=b"x", headers={"content-type": "text/plain"}).status_code == 400
    _, bundle = api("GET", f"/api/export?campaign_id={camp['id']}")
    assert bundle["collections"]["handouts"][0]["file_b64"]
    assert api("DELETE", f"/api/handouts/{h['id']}")[0] == 200
    _, summary = api("POST", "/api/import?mode=copy", bundle)
    new = next(x for x in api("GET", "/api/handouts")[1]["handouts"])
    assert client.get(f"/api/handouts/{new['id']}/file").content == png


def test_award_xp_and_loot(api):
    _, ch = api("POST", "/api/characters", {"name": "Hero"})
    _, s = api("POST", "/api/sessions", {"loot": [{"name": "Potion of Healing", "qty": 2}]})
    _, s = api("POST", f"/api/sessions/{s['id']}/award", {"xp": 150, "character_ids": [ch["id"]]})
    assert s["xp_awarded"] == 150
    _, s = api("POST", f"/api/sessions/{s['id']}/award", {"loot_index": 0, "character_id": ch["id"]})
    assert s["loot"][0]["assigned_to"] == "Hero"
    _, sheet = api("GET", f"/api/characters/{ch['id']}")
    assert sheet["xp"] == 150 and sheet["inventory"][0]["name"] == "Potion of Healing"


def test_rest(api):
    _, ch = api("POST", "/api/characters", {
        "classes": [{"name": "Wizard", "level": 4}],
        "hp": {"max": 22, "current": 3, "temp": 2},
        "hit_dice": {"total": "4d6", "remaining": 0},
        "spellcasting": {"slots": {"1": {"max": 4, "used": 4}}},
        "resources": [{"name": "Arcane Recovery", "current": 0, "max": 1, "reset": "long"}, {"name": "Thing", "current": 0, "max": 2, "reset": "short"}],
    })
    _, short = api("POST", f"/api/characters/{ch['id']}/rest", {"type": "short"})
    assert [r["current"] for r in short["resources"]] == [0, 2]
    _, long_ = api("POST", f"/api/characters/{ch['id']}/rest", {"type": "long"})
    assert long_["hp"]["current"] == 22 and long_["hp"]["temp"] == 0
    assert long_["hit_dice"]["remaining"] == 2
    assert long_["spellcasting"]["slots"]["1"]["used"] == 0
    assert long_["resources"][0]["current"] == 1


def _campaign_character(api, edition, **sheet):
    _, camp = api("POST", "/api/campaigns", {"edition": edition})
    _, ch = api("POST", "/api/characters", {"campaign_id": camp["id"], **sheet})
    return ch


def test_rest_rules_by_edition(api):
    sheet = {"classes": [{"name": "Fighter", "level": 6}], "hp": {"max": 50, "current": 10},
             "hit_dice": {"total": "6d10", "remaining": 0}}
    old = _campaign_character(api, "2014", **sheet)
    new = _campaign_character(api, "2024", **sheet)
    assert api("POST", f"/api/characters/{old['id']}/rest", {"type": "long"})[1]["hit_dice"]["remaining"] == 3
    assert api("POST", f"/api/characters/{new['id']}/rest", {"type": "long"})[1]["hit_dice"]["remaining"] == 6


def test_rests_at_zero_hp(api):
    old = _campaign_character(api, "2014", hp={"max": 10, "current": 0})
    new = _campaign_character(api, "2024", hp={"max": 10, "current": 0})
    status, r = api("POST", f"/api/characters/{old['id']}/rest", {"type": "long"})
    assert status == 400 and "1 hit point" in r["detail"]
    # 2014 short rests have no hit point requirement; 2024 ones do.
    assert api("POST", f"/api/characters/{old['id']}/rest", {"type": "short"})[0] == 200
    assert api("POST", f"/api/characters/{new['id']}/rest", {"type": "short"})[0] == 400


def test_warlock_slots_return_on_short_rest(api):
    slots = {"spellcasting": {"slots": {"2": {"max": 2, "used": 2}}}}
    _, warlock = api("POST", "/api/characters", {"classes": [{"name": "Warlock", "level": 3}], **slots})
    _, wizard = api("POST", "/api/characters", {"classes": [{"name": "Wizard", "level": 3}], **slots})
    rested = api("POST", f"/api/characters/{warlock['id']}/rest", {"type": "short"})[1]
    assert rested["spellcasting"]["slots"]["2"]["used"] == 0
    rested = api("POST", f"/api/characters/{wizard['id']}/rest", {"type": "short"})[1]
    assert rested["spellcasting"]["slots"]["2"]["used"] == 2


def test_slots_endpoint_takes_edition(api):
    body = {"classes": [{"name": "Ranger", "level": 1}]}
    assert api("POST", "/api/slots", body)[1]["slots"] == []
    assert api("POST", "/api/slots", {**body, "edition": "2024"})[1]["slots"] == [2]


def test_damage_at_zero_hp_and_massive_damage(api):
    _, enc = api("POST", "/api/encounters", {})
    _, ch = api("POST", "/api/characters", {"name": "Pip", "hp": {"max": 10, "current": 2}})
    _, enc = api("POST", f"/api/encounters/{enc['id']}/action", {"action": "add_character", "character_id": ch["id"]})
    cid = enc["combatants"][0]["id"]
    act = lambda amount: api("POST", f"/api/encounters/{enc['id']}/action", {"action": "hp", "id": cid, "amount": amount})[1]
    act(-2)
    act(-3)  # at 0 HP: one death save failure
    assert api("GET", f"/api/characters/{ch['id']}")[1]["death_saves"]["failures"] == 1
    act(4)  # healing from 0 resets death saves
    assert api("GET", f"/api/characters/{ch['id']}")[1]["death_saves"] == {"successes": 0, "failures": 0}
    log = act(-14)["log"]  # 4 HP, then 10 left over = max HP
    assert "massive damage" in log[-2]["text"]
    assert api("GET", f"/api/characters/{ch['id']}")[1]["death_saves"]["failures"] == 3
    act(5)  # healing doesn't undo a death
    assert api("GET", f"/api/characters/{ch['id']}")[1]["death_saves"]["failures"] == 3


def test_massive_damage_counts_after_temp_hp(api):
    _, enc = api("POST", "/api/encounters", {})
    _, ch = api("POST", "/api/characters", {"hp": {"max": 10, "current": 5, "temp": 5}})
    _, enc = api("POST", f"/api/encounters/{enc['id']}/action", {"action": "add_character", "character_id": ch["id"]})
    cid = enc["combatants"][0]["id"]
    api("POST", f"/api/encounters/{enc['id']}/action", {"action": "hp", "id": cid, "amount": -19})
    assert api("GET", f"/api/characters/{ch['id']}")[1]["death_saves"]["failures"] == 0  # 9 left over < 10


def test_derive_endpoint_takes_edition(api):
    sheet = {"classes": [{"name": "Bard", "level": 5}], "abilities": {"dex": 14}, "jack_of_all_trades": True, "exhaustion": 1}
    assert api("POST", "/api/derive", sheet)[1]["initiative"] == 3
    assert api("POST", "/api/derive?edition=2024", sheet)[1]["initiative"] == 2 - 2
    assert api("POST", "/api/derive?edition=1999", sheet)[0] == 400


def test_export_import_endpoints(api):
    _, ch = api("POST", "/api/characters", {"name": "Exported"})
    _, bundle = api("GET", "/api/export")
    assert bundle["collections"]["characters"][0]["name"] == "Exported"
    status, md = api("GET", f"/api/export/characters/{ch['id']}?format=md", raw=True)
    assert status == 200 and md.startswith("# Exported")
    _, single = api("GET", f"/api/export/characters/{ch['id']}")
    _, summary = api("POST", "/api/import?mode=copy", single)
    assert summary["imported"] == {"characters": 1}
    _, res = api("POST", "/api/import/markdown", {"files": [{"filename": "a.md", "text": "# Hello\n\nworld"}]})
    assert res["created"][0]["title"] == "Hello"
    assert api("POST", "/api/import", {"format": "other"})[0] == 400


def test_bad_campaign_link_is_400(api):
    status, r = api("POST", "/api/characters", {"campaign_id": "missing"})
    assert status == 400 and "campaign" in r["detail"]


def test_failed_import_writes_nothing(api):
    status, _ = api("POST", "/api/import", {"collections": {"handbook": [{"title": "ok"}], "characters": [{"classes": "bad"}]}})
    assert status == 400
    assert api("GET", "/api/handbook")[1]["handbook"] == []


def test_handout_file_name_cannot_leave_the_folder(client):
    h = client.post("/api/handouts", json={"title": "Map"}).json()
    client.put(f"/api/handouts/{h['id']}", json={"filename": "../questodian.db"})
    assert client.get(f"/api/handouts/{h['id']}/file").status_code == 404
    assert client.delete(f"/api/handouts/{h['id']}").status_code == 200


def test_lair_xp_comes_from_the_srd(api):
    _, camp = api("POST", "/api/campaigns", {"edition": "2024"})
    _, enc = api("POST", "/api/encounters", {"campaign_id": camp["id"]})
    _, enc = api("POST", f"/api/encounters/{enc['id']}/action", {"action": "add_monster", "srd_id": "monsters/aboleth"})
    aboleth = enc["combatants"][0]
    assert (aboleth["xp"], aboleth["xp_in_lair"], aboleth["in_lair"]) == (5900, 7200, False)
    _, enc = api("POST", f"/api/encounters/{enc['id']}/action", {"action": "add_monster", "srd_id": "monsters/zombie"})
    assert enc["combatants"][1]["xp_in_lair"] == 0


def test_grouped_initiative_gives_identical_monsters_one_roll(api):
    _, enc = api("POST", "/api/encounters", {})
    path = f"/api/encounters/{enc['id']}/action"
    api("POST", path, {"action": "add_monster", "srd_id": "monsters/zombie", "count": 3})
    api("POST", path, {"action": "add_monster", "srd_id": "monsters/goblin", "count": 2})
    for _ in range(5):
        _, enc = api("POST", path, {"action": "roll_initiative", "group": True})
        by_kind = {}
        for c in enc["combatants"]:
            by_kind.setdefault(c["srd_id"], set()).add(c["initiative"])
        assert all(len(rolls) == 1 for rolls in by_kind.values())


def test_warlock_slots_come_from_the_srd_table(api):
    assert api("POST", "/api/slots", {"classes": [{"name": "Warlock", "level": 5}]})[1]["slots"] == [0, 0, 2]
    assert api("POST", "/api/slots", {"classes": [{"name": "Warlock", "level": 1}], "edition": "2024"})[1]["slots"] == [1]
