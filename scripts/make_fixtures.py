"""Build the database fixtures the Rust tests open: real data at every schema version.

For each fixture this writes, under crates/core/tests/fixtures/:

    <name>.sql            a SQL dump of a database the Python app made at that version
    <name>.expected.json  every record, as the Python app reads it after upgrading a copy

    uv run python scripts/make_fixtures.py

Fixtures:
    pre_alembic   tables at the 0001 schema with no alembic_version table (oldest databases)
    v0001         Alembic revision 0001, including sessions that hold an encounter
    v0002         the current schema, with data in every table
"""

from __future__ import annotations

import json
import shutil
import sqlite3
import tempfile
from pathlib import Path

from alembic import command
from sqlalchemy import create_engine

from questodian import migrate, schemas
from questodian.db import DB_FILENAME, Database
from questodian.repo import KINDS, Repo

OUT = Path(__file__).resolve().parent.parent / "crates" / "core" / "tests" / "fixtures"
STAMP = "2025-06-01T10:00:00+00:00"


def dump(path: Path) -> str:
    con = sqlite3.connect(path)
    lines = list(con.iterdump())
    con.close()
    return "\n".join(lines) + "\n"


def insert(con: sqlite3.Connection, table: str, row: dict) -> None:
    cols = list(row)
    values = [json.dumps(v) if isinstance(v, (dict, list)) else v for v in row.values()]
    con.execute(f"INSERT INTO {table} ({', '.join(cols)}) VALUES ({', '.join('?' * len(cols))})", values)


def record(schema, **fields) -> dict:
    doc = schema.model_validate(fields).model_dump()
    doc.update(created=STAMP, updated=STAMP)
    return doc


def old_data(con: sqlite3.Connection) -> None:
    """Rows in the 0001 schema: no campaign edition, and sessions with an `encounter` column."""
    camp = record(schemas.Campaign, id="camp1", name="Lost Mine", players=[{"name": "Ann", "character_id": "char1"}])
    del camp["edition"]
    insert(con, "campaigns", camp)
    insert(con, "characters", record(schemas.Character, id="char1", campaign_id="camp1", name="Ann's Rogue",
                                     classes=[{"name": "Rogue", "level": 3}], skills={"stealth": "expertise"},
                                     hp={"max": 21, "current": 9, "temp": 0}, inventory=[{"name": "Rope", "weight": 10}]))
    insert(con, "characters", {**record(schemas.Character, id="char2", name="Unlinked"), "campaign_id": None})
    insert(con, "handbook", record(schemas.HandbookEntry, id="hb1", campaign_id="camp1", title="Crits", tags=["house"]))
    fight = {"round": 2, "turn": 1, "active": True, "combatants": [
        {"id": "c1", "name": "Ann's Rogue", "kind": "pc", "initiative": 17, "hp": 9, "max_hp": 21, "character_id": "char1"},
        {"id": "c2", "name": "Goblin 1", "kind": "monster", "initiative": 12, "hp": 7, "max_hp": 7, "srd_id": "monsters/goblin"},
    ]}
    empty = {"round": 0, "turn": 0, "active": False, "combatants": []}
    for sid, number, encounter in (("sess1", 1, fight), ("sess2", 2, empty)):
        session = record(schemas.Session, id=sid, campaign_id="camp1", number=number, title=f"Part {number}",
                         log=[{"id": "l1", "time": STAMP, "kind": "note", "text": "Started"}])
        session["encounter"] = encounter
        insert(con, "sessions", session)


def new_data(repo: Repo) -> None:
    """Current-schema data in every table, through the app itself."""
    camp = repo.save("campaigns", {"id": "camp1", "name": "Seven Bells", "edition": "2024"})
    repo.save("characters", {"id": "char1", "campaign_id": camp["id"], "name": "Pip", "exhaustion": 2,
                             "spellcasting": {"ability": "wis", "slots": {"1": {"max": 2, "used": 1}}}})
    repo.save("handbook", {"id": "hb1", "title": "Lore", "tags": "bells, town"})
    repo.save("sessions", {"id": "sess1", "campaign_id": "camp1", "number": 1, "loot": [{"name": "Bell", "qty": 2}]})
    repo.save("scenes", {"id": "scene1", "campaign_id": "camp1", "session_id": "sess1", "title": "Gate", "status": "active"})
    repo.save("npcs", {"id": "npc1", "campaign_id": "camp1", "name": "Mara", "stat_ref": "srd:monsters/spy", "tags": ["spy"]})
    repo.save("monsters", {"id": "mon1", "name": "Bell-Ringer", "cr": "2", "xp": 450, "abilities": {"str": 16}})
    repo.save("notes", {"id": "note1", "campaign_id": "camp1", "session_id": "sess1", "kind": "quest", "pinned": True})
    repo.save("encounters", {"id": "enc1", "campaign_id": "camp1", "session_id": "sess1", "scene_id": "scene1",
                             "active": True, "round": 1, "combatants": [{"id": "x", "name": "Spy", "hp": 27, "max_hp": 27}]})
    repo.save("trackers", {"id": "trk1", "campaign_id": "camp1", "name": "Tolls", "value": 3, "max": 7})
    repo.save("handouts", {"id": "ho1", "campaign_id": "camp1", "title": "Map", "folder": "Maps"})


def expected(sql: str) -> dict:
    """Every record as the Python app reads it after opening (and upgrading) the dump."""
    with tempfile.TemporaryDirectory() as tmp:
        con = sqlite3.connect(Path(tmp) / DB_FILENAME)
        con.executescript(sql)
        con.close()
        db = Database(tmp)
        with db.session() as s:
            repo = Repo(s)
            out = {kind: repo.list(kind) for kind in KINDS}
        db.dispose()
        return out


def write(name: str, path: Path) -> None:
    sql = dump(path)
    (OUT / f"{name}.sql").write_text(sql)
    (OUT / f"{name}.expected.json").write_text(json.dumps(expected(sql), indent=1, ensure_ascii=False) + "\n")
    print(f"wrote {name}")


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        v1 = tmp / "v0001.db"
        engine = create_engine(f"sqlite:///{v1}")
        command.upgrade(migrate.alembic_config(engine), "0001")
        engine.dispose()
        con = sqlite3.connect(v1)
        old_data(con)
        con.commit()
        con.close()
        write("v0001", v1)

        pre = tmp / "pre_alembic.db"
        shutil.copy(v1, pre)
        con = sqlite3.connect(pre)
        con.execute("DROP TABLE alembic_version")
        con.commit()
        con.close()
        write("pre_alembic", pre)

        (tmp / "v2").mkdir()
        db = Database(tmp / "v2")
        with db.session() as s:
            new_data(Repo(s))
        db.dispose()
        write("v0002", tmp / "v2" / DB_FILENAME)


if __name__ == "__main__":
    main()
