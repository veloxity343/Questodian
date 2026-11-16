import pytest
from pydantic import ValidationError

from questodian import game
from questodian.repo import NotFound, normalize


def test_normalize_fills_defaults():
    doc = normalize("characters", {"name": "Vex", "hp": {"max": 30}, "custom_field": 1})
    assert doc["hp"] == {"max": 30, "current": 10, "temp": 0}
    assert doc["abilities"]["str"] == 10
    assert "custom_field" not in doc


def test_normalize_coerces_and_rejects():
    doc = normalize("characters", {"ac": "15", "inspiration": 1, "campaign_id": None})
    assert doc["ac"] == 15 and doc["inspiration"] is True and doc["campaign_id"] == ""
    with pytest.raises(ValidationError):
        normalize("characters", {"inventory": "junk"})


def test_defaults_are_not_shared():
    a = normalize("encounters", {})
    a["combatants"].append({"name": "x"})
    assert normalize("encounters", {})["combatants"] == []


def test_handbook_tags_from_string():
    assert normalize("handbook", {"tags": "a, b ,,c"})["tags"] == ["a", "b", "c"]


def test_crud(repo):
    c = repo.create("campaigns", {"name": "Curse of Strahd"})
    assert repo.get("campaigns", c["id"])["name"] == "Curse of Strahd"
    repo.update("campaigns", c["id"], {"setting": "Barovia"})
    got = repo.get("campaigns", c["id"])
    assert got["setting"] == "Barovia" and got["created"] == c["created"]
    assert [x["id"] for x in repo.list("campaigns")] == [c["id"]]
    repo.delete("campaigns", c["id"])
    with pytest.raises(NotFound):
        repo.get("campaigns", c["id"])


def test_rejects_bad_ids_and_kinds(repo):
    with pytest.raises(NotFound):
        repo.get("characters", "../../etc/passwd")
    with pytest.raises(ValueError):
        repo.list("secrets")


def test_unknown_campaign_link_rejected(repo):
    with pytest.raises(ValueError, match="campaign"):
        repo.create("characters", {"campaign_id": "nope"})


def test_deleting_campaign_removes_sessions_and_unlinks(repo):
    c = repo.create("campaigns", {"name": "C"})
    s = repo.create("sessions", {"campaign_id": c["id"]})
    ch = repo.create("characters", {"campaign_id": c["id"]})
    hb = repo.create("handbook", {"campaign_id": c["id"]})
    repo.delete("campaigns", c["id"])
    assert not repo.exists("sessions", s["id"])
    assert repo.get("characters", ch["id"])["campaign_id"] == ""
    assert repo.get("handbook", hb["id"])["campaign_id"] == ""


def test_list_filters_and_sorts(repo):
    c = repo.create("campaigns", {"name": "C"})
    repo.create("sessions", {"campaign_id": c["id"], "number": 2})
    repo.create("sessions", {"campaign_id": c["id"], "number": 1})
    repo.create("sessions", {})
    assert [s["number"] for s in repo.list("sessions", campaign_id=c["id"])] == [1, 2]
    repo.create("characters", {"name": "bob"})
    repo.create("characters", {"name": "Alice"})
    assert [x["name"] for x in repo.list("characters")] == ["Alice", "bob"]


def test_advance_turn_and_rounds():
    enc = {"round": 0, "turn": 0, "active": False, "combatants": [
        {"name": "A", "initiative": 5, "init_bonus": 0},
        {"name": "B", "initiative": 15, "init_bonus": 0},
        {"name": "C", "initiative": 15, "init_bonus": 3},
    ]}
    game.advance_turn(enc)  # start
    assert enc["active"] and enc["round"] == 1 and [c["name"] for c in enc["combatants"]] == ["C", "B", "A"]
    game.advance_turn(enc)
    game.advance_turn(enc)
    game.advance_turn(enc)
    assert enc["round"] == 2 and enc["turn"] == 0
    game.advance_turn(enc, -1)
    assert enc["round"] == 1 and enc["turn"] == 2


def test_hp_changes_use_temp_hp_first():
    c = {"hp": 10, "max_hp": 12, "temp_hp": 5}
    game.apply_hp_change(c, -7)
    assert (c["hp"], c["temp_hp"]) == (8, 0)
    game.apply_hp_change(c, 100)
    assert c["hp"] == 12
    game.apply_hp_change(c, -50)
    assert c["hp"] == 0


def test_session_delete_unlinks_but_campaign_delete_removes(repo):
    c = repo.create("campaigns", {"name": "C"})
    s = repo.create("sessions", {"campaign_id": c["id"]})
    scene = repo.create("scenes", {"campaign_id": c["id"], "session_id": s["id"], "title": "Lanternfest"})
    note = repo.create("notes", {"campaign_id": c["id"], "session_id": s["id"]})
    repo.delete("sessions", s["id"])
    assert repo.get("scenes", scene["id"])["session_id"] == ""
    assert repo.get("notes", note["id"])["session_id"] == ""
    for kind in ("npcs", "monsters", "encounters", "trackers", "handouts"):
        repo.create(kind, {"campaign_id": c["id"]})
    repo.delete("campaigns", c["id"])
    for kind in ("scenes", "notes", "npcs", "monsters", "encounters", "trackers", "handouts"):
        assert repo.list(kind) == [], kind


def test_links_are_checked_and_filterable(repo):
    c = repo.create("campaigns", {"name": "C"})
    with pytest.raises(ValueError, match="session"):
        repo.create("scenes", {"campaign_id": c["id"], "session_id": "missing"})
    s = repo.create("sessions", {"campaign_id": c["id"]})
    repo.create("scenes", {"campaign_id": c["id"], "session_id": s["id"], "position": 2, "title": "B"})
    repo.create("scenes", {"campaign_id": c["id"], "session_id": s["id"], "position": 1, "title": "A"})
    repo.create("scenes", {"campaign_id": c["id"], "title": "Loose"})
    assert [x["title"] for x in repo.list("scenes", session_id=s["id"])] == ["A", "B"]
    repo.create("notes", {"campaign_id": c["id"], "kind": "quest"})
    repo.create("notes", {"campaign_id": c["id"], "kind": "pin"})
    assert [n["kind"] for n in repo.list("notes", kind="quest")] == ["quest"]
    with pytest.raises(ValueError):
        repo.list("notes", title="x")


@pytest.mark.parametrize(
    ("platform", "env", "expected"),
    [
        ("win32", {"APPDATA": "C:/Users/me/AppData/Roaming"}, "C:/Users/me/AppData/Roaming/Questodian"),
        ("darwin", {}, "~/Library/Application Support/Questodian"),
        ("linux", {"XDG_DATA_HOME": "/xdg"}, "/xdg/questodian"),
    ],
)
def test_default_data_dir_per_os(monkeypatch, platform, env, expected):
    from pathlib import Path

    from questodian import db

    monkeypatch.delenv("QUESTODIAN_DATA", raising=False)
    for k in ("APPDATA", "XDG_DATA_HOME"):
        monkeypatch.delenv(k, raising=False)
    for k, v in env.items():
        monkeypatch.setenv(k, v)
    monkeypatch.setattr(db.sys, "platform", platform)
    assert db.default_data_dir() == Path(expected).expanduser()


def test_data_dir_env_wins(monkeypatch, tmp_path):
    from questodian import db

    monkeypatch.setenv("QUESTODIAN_DATA", str(tmp_path))
    assert db.default_data_dir() == tmp_path

