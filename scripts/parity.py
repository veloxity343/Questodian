"""Compare the Python and Rust backends: same requests, same responses.

Starts both servers on scratch data folders, sends the same requests to each and
reports every difference in status code or JSON body. Responses that involve dice
are compared by shape, since the two can't roll the same numbers.

    uv run python scripts/parity.py            # needs `cargo build -p questodian-cli` first
    uv run python scripts/parity.py -v         # list every request

Exit status is 0 when everything matches.
"""

from __future__ import annotations

import argparse
import gzip
import json
import random
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RUST_BIN = ROOT / "target" / "debug" / ("questodian.exe" if sys.platform == "win32" else "questodian")
PY_PORT, RUST_PORT = 8811, 8812

# Keys that legitimately differ between the two servers.
IGNORED_KEYS = {"data_dir", "database"}


def call(port: int, method: str, path: str, body=None) -> tuple[int, object]:
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(f"http://127.0.0.1:{port}{path}", data=data, method=method)
    if data is not None:
        req.add_header("Content-Type", "application/json")
    try:
        with urllib.request.urlopen(req, timeout=30) as res:
            status, raw = res.status, res.read()
    except urllib.error.HTTPError as e:
        status, raw = e.code, e.read()
    try:
        return status, json.loads(raw)
    except ValueError:
        return status, raw.decode(errors="replace")


def strip(value):
    if isinstance(value, dict):
        return {k: strip(v) for k, v in value.items() if k not in IGNORED_KEYS}
    if isinstance(value, list):
        return [strip(v) for v in value]
    return value


def shape(value):
    """Types and keys only, for responses that contain random dice."""
    if isinstance(value, dict):
        # A die's flags appear only when set, which depends on the roll.
        return {k: shape(v) for k, v in value.items() if k not in ("exploded", "rerolled", "discarded")}
    if isinstance(value, list):
        return [shape(v) for v in value[:1]]
    if isinstance(value, bool) or value is None:
        return type(value).__name__
    if isinstance(value, (int, float)):
        return "number"
    return type(value).__name__


def first_difference(a, b, where="") -> str | None:
    if type(a) is not type(b) and not (isinstance(a, (int, float)) and isinstance(b, (int, float))):
        return f"{where or '.'}: {a!r:.80} != {b!r:.80}"
    if isinstance(a, dict):
        for k in sorted(set(a) | set(b)):
            if k not in a or k not in b:
                return f"{where}.{k}: only in {'rust' if k in b else 'python'}"
            d = first_difference(a[k], b[k], f"{where}.{k}")
            if d:
                return d
        return None
    if isinstance(a, list):
        if len(a) != len(b):
            return f"{where}: {len(a)} items != {len(b)}"
        for i, (x, y) in enumerate(zip(a, b)):
            d = first_difference(x, y, f"{where}[{i}]")
            if d:
                return d
        return None
    return None if a == b else f"{where or '.'}: {a!r:.80} != {b!r:.80}"


class Checker:
    def __init__(self, verbose: bool):
        self.verbose = verbose
        self.count = 0
        self.failures: list[str] = []

    def same(self, method: str, path: str, body=None, *, by_shape=False, status_only=False):
        self.count += 1
        ps, pb = call(PY_PORT, method, path, body)
        rs, rb = call(RUST_PORT, method, path, body)
        label = f"{method} {path}" + (f" {json.dumps(body)[:60]}" if body is not None else "")
        if self.verbose:
            print(f"  {ps} {rs}  {label}")
        if ps != rs:
            self.failures.append(f"{label}\n    status: python {ps}, rust {rs}\n    python: {str(pb)[:200]}\n    rust:   {str(rb)[:200]}")
            return
        if status_only or ps == 422:
            return
        if by_shape:
            pb, rb = shape(pb), shape(rb)
        diff = first_difference(strip(pb), strip(rb))
        if diff:
            self.failures.append(f"{label}\n    {diff}")


def start(cmd: list[str], port: int, cwd: Path) -> subprocess.Popen:
    proc = subprocess.Popen(cmd, cwd=cwd, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        try:
            call(port, "GET", "/api/meta")
            return proc
        except OSError:
            time.sleep(0.2)
    proc.kill()
    raise SystemExit(f"server on port {port} didn't start: {proc.stderr.read().decode()[-2000:]}")


def srd_entries(edition: str) -> list[dict]:
    name = "srd.json.gz" if edition == "2014" else "srd-2024.json.gz"
    return json.loads(gzip.decompress((ROOT / "src/questodian/data" / name).read_bytes()))["entries"]


# --------------------------------------------------------------------------- the checks


def check_tools(c: Checker, rng: random.Random) -> None:
    c.same("GET", "/api/meta")
    for expr in ["1 + 2 * 3", "(1+2)*3", "2^3^2", "2**3", "-3 + +5", "7 // 2", "-7 // 2", "7 / 2", "7 % 3", "-7 % 3",
                 "10/4", "1/3", "2/3*3", "mod(18)", "mod(9)", "prof(17)", "max(1, 5, 3) + min(4, 2)", "floor(7/2)",
                 "ceil(7/2)", "round(2.5)", "round(-2.5)", "round(1.23456, 2)", "abs(-4)", "sqrt(2)", "pi", "e * 2",
                 "1234567.5 * 1", "0.00001234 * 1", ".5 + 1.25", "3", "  42  ", "-(2)", "((1))",
                 "", "2+", "(1+2", "foo(2)", "1 $ 2", "1 2", "1/0", "0^-1", "(-8)^0.5", "9^9^9", "prof(0)",
                 "round(1, 2, 3)", "import os", "1d6!" * 200, "5d6kh9", "3d6r6", "2d6kh1dl1", "d0", "1001d6", "2d"]:
        c.same("POST", "/api/roll", {"expression": expr})
    for expr in ["d20", "2d20kh1+5", "adv+5", "dis", "4d6dl1", "d%", "4dF", "3d6!", "2d6ro2", "8d6//2", "stats"]:
        c.same("POST", "/api/roll", {"expression": expr}, by_shape=True)
        c.same("GET", f"/api/roll?expr={urllib.parse.quote(expr)}&times=3", by_shape=True)
    c.same("GET", "/api/roll?expr=1%2B1&times=0", status_only=True)
    c.same("GET", "/api/roll?expr=1%2B1&times=101", status_only=True)
    c.same("GET", "/api/roll", status_only=True)
    c.same("GET", "/api/roll?expr=%20")
    c.same("POST", "/api/roll", {}, status_only=True)
    c.same("POST", "/api/stats", by_shape=True)

    classes = ["Barbarian", "Bard", "Cleric", "Druid", "Fighter", "Monk", "Paladin", "Ranger", "Rogue", "Sorcerer", "Warlock", "Wizard"]
    for edition in ("2014", "2024"):
        for cls in classes:
            for level in (1, 2, 3, 5, 9, 20):
                c.same("POST", "/api/slots", {"classes": [{"name": cls, "level": level}], "edition": edition})
        for _ in range(20):
            picks = rng.sample(classes, 2)
            c.same("POST", "/api/slots", {"classes": [{"name": p, "level": rng.randint(1, 10)} for p in picks], "edition": edition})

    skills = ["acrobatics", "arcana", "athletics", "insight", "investigation", "perception", "stealth"]
    for _ in range(150):
        sheet = {
            "classes": [{"name": rng.choice(classes), "level": rng.randint(0, 20)} for _ in range(rng.randint(1, 2))],
            "abilities": {a: rng.randint(1, 30) for a in ("str", "dex", "con", "int", "wis", "cha")},
            "save_proficiencies": rng.sample(["str", "dex", "con", "int", "wis", "cha"], rng.randint(0, 3)),
            "skills": {s: rng.choice(["none", "half", "proficient", "expertise"]) for s in rng.sample(skills, 3)},
            "jack_of_all_trades": rng.random() < 0.3,
            "initiative_misc": rng.randint(-2, 5),
            "speed": rng.choice([25, 30, 35]),
            "exhaustion": rng.randint(0, 6),
            "spellcasting": {"ability": rng.choice(["", "int", "wis", "cha"])},
            "xp": str(rng.randint(0, 400000)),
        }
        c.same("POST", f"/api/derive?edition={rng.choice(['', '2014', '2024'])}", sheet)
    c.same("POST", "/api/derive?edition=1999", {})
    c.same("POST", "/api/derive", {"exhaustion": 9}, status_only=True)
    c.same("POST", "/api/derive", {"classes": []}, status_only=True)

    for size in (1, 4, 6):
        for level in (1, 5, 11, 20):
            for xp in (0, 50, 400, 3000, 20000):
                c.same("GET", f"/api/encounter-budget?party_size={size}&party_level={level}&xp={xp}")
    c.same("GET", "/api/encounter-budget")
    c.same("GET", "/api/encounter-budget?party_level=x", status_only=True)

    c.same("GET", "/api/projection")
    c.same("PUT", "/api/projection", {"id": "abc", "zoom": 2, "x": 1.5, "y": -3})
    c.same("GET", "/api/projection")
    c.same("PUT", "/api/projection", {"zoom": 0}, status_only=True)


def check_srd(c: Checker, rng: random.Random) -> None:
    for edition in ("2014", "2024", ""):
        c.same("GET", f"/api/srd/categories?edition={edition}")
    c.same("GET", "/api/srd/categories?edition=2015")
    c.same("GET", "/api/srd/search?limit=0", status_only=True)
    c.same("GET", "/api/srd/search?limit=501", status_only=True)
    c.same("GET", "/api/srd/search?offset=-1", status_only=True)
    c.same("GET", "/api/srd/entry", status_only=True)
    c.same("GET", "/api/srd/entry?id=nope")
    for edition in ("2014", "2024"):
        entries = srd_entries(edition)
        for e in rng.sample(entries, 40):
            c.same("GET", f"/api/srd/entry?id={urllib.parse.quote(e['id'])}&edition={edition}")
            words = e["name"].split()
            for q in {e["name"], e["name"].lower()[:4], rng.choice(words), " ".join(words[-2:])}:
                c.same("GET", f"/api/srd/search?q={urllib.parse.quote(q)}&edition={edition}&limit=500")
        for q in ["fire", "of the", "dragon", "cure wounds", "  ", "zzzz", "a"]:
            c.same("GET", f"/api/srd/search?q={urllib.parse.quote(q)}&edition={edition}&limit=500")
        categories = sorted({e["category"] for e in entries})
        for cat in categories:
            c.same("GET", f"/api/srd/search?category={cat}&edition={edition}&limit=500")
            c.same("GET", f"/api/srd/search?category={cat}&edition={edition}&limit=7&offset=3")
        # Filters on structured data: use real keys and values from the data.
        for e in rng.sample([e for e in entries if e["data"]], 60):
            key = rng.choice(sorted(e["data"]))
            value = e["data"][key]
            if isinstance(value, list):
                value = value[0] if value else ""
            if isinstance(value, (dict, list)):
                continue
            want = {True: "true", False: "false", None: "None"}.get(value, value) if isinstance(value, bool) or value is None else value
            qs = urllib.parse.urlencode({"category": e["category"], key: want, "edition": edition, "limit": 500})
            c.same("GET", f"/api/srd/search?{qs}")


def check_ui(c: Checker) -> None:
    asset = next((ROOT / "src/questodian/web/assets").iterdir()).name
    for path in ["/", f"/assets/{asset}", "/assets/missing.js", "/characters/abc", "/campaigns/x/sessions",
                 "/../pyproject.toml", "/%2e%2e/pyproject.toml", "/api/nope", "/api/", "/favicon.ico", "/index.html"]:
        c.count += 1
        ps, pb = call(PY_PORT, "GET", path)
        rs, rb = call(RUST_PORT, "GET", path)
        if c.verbose:
            print(f"  {ps} {rs}  GET {path}")
        if ps != rs or (isinstance(pb, str) and pb != rb):
            c.failures.append(f"GET {path}\n    python {ps} {str(pb)[:120]!r}\n    rust   {rs} {str(rb)[:120]!r}")


# --------------------------------------------------------------------------- scenarios with records

import re

ID_RE = re.compile(r"^[0-9a-f]{12}$")
TIME_RE = re.compile(r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\+00:00$")


def anonymize(value, ids: dict):
    """Replace record ids with ID1, ID2... (in order of appearance) and times with TS."""
    if isinstance(value, dict):
        return {k: anonymize(v, ids) for k, v in value.items()}
    if isinstance(value, list):
        return [anonymize(v, ids) for v in value]
    if isinstance(value, str):
        if ID_RE.match(value):
            return ids.setdefault(value, f"ID{len(ids) + 1}")
        if TIME_RE.match(value):
            return "TS"
        for raw, token in ids.items():
            value = value.replace(raw, token)
        # Stored handout images get a random suffix: ID3-1a2b3c.png
        value = re.sub(r"^(ID\d+)-[0-9a-f]{6}(\.\w+)$", r"\1-RANDOM\2", value)
    return value


class Session:
    """Runs a scenario against one server, remembering what it created."""

    def __init__(self, port: int):
        self.port = port
        self.log: list[tuple[str, int, object]] = []
        self.ids: dict[str, str] = {}

    def __call__(self, method: str, path: str, body=None, keep_body=True):
        for name, real in self.ids.items():
            path = path.replace("{" + name + "}", real)
            if body is not None:
                body = json.loads(json.dumps(body).replace("{" + name + "}", real))
        status, out = call(self.port, method, path, body)
        self.log.append((f"{method} {path}", status, out if keep_body and status != 422 else None))
        return out

    def keep(self, name: str, doc) -> None:
        if isinstance(doc, dict) and "id" in doc:
            self.ids[name] = doc["id"]


def record_scenario(api: Session) -> None:
    keep = api.keep
    keep("camp", api("POST", "/api/campaigns", {"name": "Seven Bells", "edition": "2024", "players": [{"name": "Ann"}]}))
    keep("old", api("POST", "/api/campaigns", {"name": "old school"}))
    api("POST", "/api/campaigns", None)
    api("POST", "/api/campaigns", [1, 2])
    api("POST", "/api/campaigns", {"edition": "2015"})
    api("POST", "/api/campaigns", {"players": "x"})
    keep("pip", api("POST", "/api/characters", {"name": "Pip", "campaign_id": "{camp}", "xp": "300", "inspiration": "yes",
                                               "abilities": {"str": "12"}, "skills": {"stealth": "expertise"}, "extra": 1}))
    keep("bo", api("POST", "/api/characters", {"name": "bo", "classes": [{"name": "Wizard", "level": "3"}]}))
    api("POST", "/api/characters", {"classes": []})
    api("POST", "/api/characters", {"exhaustion": 7})
    api("POST", "/api/characters", {"campaign_id": "missing"})
    api("POST", "/api/characters", {"campaign_id": None, "name": None})
    api("POST", "/api/characters", {"id": "my-own_id", "created": "2001-01-01T00:00:00+00:00"})
    api("PUT", "/api/characters/{pip}", {"hp": {"current": 5}, "name": "Pip the Bold", "id": "ignored"})
    api("PUT", "/api/characters/{pip}", [])
    api("PUT", "/api/characters/{pip}", None)
    api("PUT", "/api/characters/nope", {"name": "x"})
    api("GET", "/api/characters/{pip}")
    api("GET", "/api/characters/bad%20id!")
    keep("s1", api("POST", "/api/sessions", {"campaign_id": "{camp}", "number": 2, "title": "Second"}))
    keep("s2", api("POST", "/api/sessions", {"campaign_id": "{camp}", "number": "1", "status": "done"}))
    api("POST", "/api/sessions", {"status": "later"})
    keep("scene", api("POST", "/api/scenes", {"campaign_id": "{camp}", "session_id": "{s1}", "title": "b gate", "position": 1}))
    api("POST", "/api/scenes", {"campaign_id": "{camp}", "session_id": "{s1}", "title": "A gate", "position": 1})
    api("POST", "/api/npcs", {"campaign_id": "{camp}", "name": "Mara", "tags": "spy, ally ,"})
    api("POST", "/api/npcs", {"campaign_id": "{camp}", "name": "aldo", "tags": ["x"]})
    api("POST", "/api/monsters", {"name": "Bell-Ringer", "cr": "2", "abilities": {"str": 14}})
    api("POST", "/api/notes", {"campaign_id": "{camp}", "kind": "quest", "pinned": 1, "title": "Find the bell"})
    api("POST", "/api/notes", {"campaign_id": "{camp}", "kind": "pin", "done": "on"})
    api("POST", "/api/notes", {"kind": "todo"})
    keep("enc", api("POST", "/api/encounters", {"campaign_id": "{camp}", "session_id": "{s1}", "scene_id": "{scene}", "party_size": "5"}))
    api("POST", "/api/encounters", {"party_level": 21})
    api("POST", "/api/trackers", {"campaign_id": "{camp}", "name": "Tolls", "max": 7, "steps": [{"label": "one"}]})
    api("POST", "/api/trackers", {"max": 0})
    api("POST", "/api/handouts", {"campaign_id": "{camp}", "folder": "Maps", "title": "city"})
    api("POST", "/api/handouts", {"campaign_id": "{camp}", "folder": "letters", "title": "Note"})
    api("POST", "/api/handbook", {"title": "zeta rules", "tags": "a,b", "campaign_id": "{camp}"})
    api("POST", "/api/handbook", {"title": "Alpha", "category": "Lore"})
    for kind in ("campaigns", "sessions", "characters", "handbook", "scenes", "npcs", "monsters", "notes", "encounters", "trackers", "handouts"):
        api("GET", f"/api/{kind}")
        api("GET", f"/api/{kind}?campaign_id={{camp}}")
    api("GET", "/api/scenes?session_id={s1}")
    api("GET", "/api/notes?kind=quest")
    api("GET", "/api/nothings")
    api("DELETE", "/api/sessions/{s1}")
    api("GET", "/api/scenes")
    api("GET", "/api/encounters")
    api("DELETE", "/api/campaigns/{camp}")
    api("DELETE", "/api/campaigns/{camp}")
    for kind in ("sessions", "characters", "handbook", "scenes", "npcs", "notes", "encounters", "trackers", "handouts"):
        api("GET", f"/api/{kind}")


def game_scenario(api: Session) -> None:
    keep = api.keep
    keep("c24", api("POST", "/api/campaigns", {"name": "New rules", "edition": "2024"}))
    keep("c14", api("POST", "/api/campaigns", {"name": "Old rules"}))
    keep("pc", api("POST", "/api/characters", {"name": "Pip", "campaign_id": "{c24}", "abilities": {"dex": 16},
                                              "classes": [{"name": "Bard", "level": 5}], "jack_of_all_trades": True,
                                              "hp": {"max": 20, "current": 6, "temp": 3}, "conditions": ["prone"]}))
    keep("pc2", api("POST", "/api/characters", {"name": "Ann", "campaign_id": "{c14}", "classes": [{"name": "Bard", "level": 5}],
                                               "jack_of_all_trades": True, "hp": {"max": 12, "current": 12}}))
    keep("mon", api("POST", "/api/monsters", {"campaign_id": "{c24}", "name": "Bell-Ringer", "ac": 13, "hp": 30, "xp": 450}))
    keep("npc", api("POST", "/api/npcs", {"campaign_id": "{c24}", "name": "Mara", "role": "Spy", "stat_ref": "srd:monsters/spy"}))
    keep("npc2", api("POST", "/api/npcs", {"campaign_id": "{c24}", "name": "Tob", "stats": "Commoner"}))
    keep("npc3", api("POST", "/api/npcs", {"campaign_id": "{c24}", "name": "Bad", "stat_ref": "srd:monsters/nope"}))
    keep("enc", api("POST", "/api/encounters", {"campaign_id": "{c24}", "name": "Gate"}))
    act = lambda body, keep_body=True: api("POST", "/api/encounters/{enc}/action", body, keep_body)
    act({"action": "add_monster", "srd_id": "monsters/zombie"})
    act({"action": "add_monster", "srd_id": "monsters/zombie", "count": 2})
    act({"action": "add_monster", "monster_id": "{mon}"})
    act({"action": "add_monster", "srd_id": "monsters/goblin"})  # not in the 2024 SRD
    act({"action": "add_monster", "srd_id": "monsters/zombie", "count": 51})
    act({"action": "add_npc", "npc_id": "{npc}"})
    act({"action": "add_npc", "npc_id": "{npc2}"})
    act({"action": "add_npc", "npc_id": "{npc3}"})
    act({"action": "add_character", "character_ids": ["{pc}", "{pc2}"]})
    act({"action": "add_character", "character_id": "missing"})
    act({"action": "add", "combatant": {"name": "Torch", "hp": "5", "kind": "npc", "initiative": 3}})
    act({"action": "add", "combatant": {"kind": "dragon"}})
    act({"action": "explode"})
    doc = api("GET", "/api/encounters/{enc}")
    ids = [x["id"] for x in doc["combatants"]]
    for i, cid in enumerate(ids):
        api.ids[f"x{i}"] = cid
    for i in range(len(ids)):
        act({"action": "update", "id": f"{{x{i}}}", "changes": {"initiative": (i * 7) % 5, "id": "nope", "junk": 1}})
    act({"action": "update", "id": "{x0}", "changes": {"hp": "lots"}})
    act({"action": "update", "id": "nobody", "changes": {}})
    act({"action": "sort"})
    for _ in range(3):
        act({"action": "next"})
    act({"action": "prev"})
    for _ in range(len(ids) + 1):
        act({"action": "next"})
    pip = next(i for i, x in enumerate(doc["combatants"]) if x["name"] == "Pip")
    for amount in (-2, -8, -1, 5, 0, -40):  # temp HP, drop to 0, damage at 0, heal, massive damage
        act({"action": "hp", "id": f"{{x{pip}}}", "amount": amount})
    api("GET", "/api/characters/{pc}")
    act({"action": "remove", "id": "{x1}"})
    act({"action": "end"})
    api("GET", "/api/characters/{pc}")
    act({"action": "clear"})
    act({"action": "clear", "all": True})
    act({"action": "add_monster", "srd_id": "monsters/wolf", "average_hp": False}, keep_body=False)
    act({"action": "roll_hp"}, keep_body=False)
    act({"action": "roll_initiative", "which": "monsters"}, keep_body=False)
    act({"action": "roll_initiative", "which": "unset"}, keep_body=False)
    act({"action": "add_monster", "srd_id": "monsters/zombie", "count": 3}, keep_body=False)
    act({"action": "roll_initiative", "group": True}, keep_body=False)
    rolled = api("GET", "/api/encounters/{enc}", keep_body=False) or {}
    by_kind: dict = {}
    for x in rolled.get("combatants", []):
        if x["kind"] == "monster":
            by_kind.setdefault(x["srd_id"], set()).add(x["initiative"])
    grouped = bool(by_kind) and all(len(v) == 1 for v in by_kind.values())
    api.log.append(("grouped initiative: one roll per kind of monster", 200, grouped))
    act({"action": "clear", "all": True}, keep_body=False)  # drop the randomly rolled monsters

    keep("enc14", api("POST", "/api/encounters", {"campaign_id": "{c14}", "combatants": [{"name": "Guard", "surprised": True}, {"name": "Scout"}]}))
    api("POST", "/api/encounters/{enc14}/action", {"action": "next"})
    api("POST", "/api/encounters/{enc14}/action", {"action": "end"})
    api("POST", "/api/encounters/{enc}/action", {"action": "add", "combatant": {"name": "Sentry", "surprised": True}})
    api("POST", "/api/encounters/{enc}/action", {"action": "roll_initiative"}, keep_body=False)
    api("POST", "/api/encounters/{enc}/action", {"action": "clear", "all": True}, keep_body=False)

    keep("sess", api("POST", "/api/sessions", {"campaign_id": "{c14}", "loot": [{"name": "Gem", "qty": 0, "value": "50 gp"}]}))
    api("POST", "/api/sessions/{sess}/award", {"xp": 300, "character_ids": ["{pc}", "{pc2}"]})
    api("POST", "/api/sessions/{sess}/award", {"xp": -1000, "character_ids": ["{pc2}"]})
    api("POST", "/api/sessions/{sess}/award", {"xp": 5})
    api("POST", "/api/sessions/{sess}/award", {"loot_index": 0, "character_id": "{pc2}"})
    api("POST", "/api/sessions/{sess}/award", {"loot_index": 3, "character_id": "{pc2}"})
    api("POST", "/api/sessions/{sess}/award", {})
    api("POST", "/api/sessions/{sess}/log", {"text": "  The bell tolls  ", "kind": ""})
    api("POST", "/api/sessions/{sess}/log", {"text": "   "})
    api("POST", "/api/sessions/{sess}/log", {})
    api("GET", "/api/characters/{pc2}")
    for who in ("{pc}", "{pc2}"):
        api("PUT", f"/api/characters/{who}", {"hp": {"max": 20, "current": 0}, "exhaustion": 3,
                                             "hit_dice": {"total": "5d8", "remaining": 0},
                                             "spellcasting": {"slots": {"1": {"max": 4, "used": 3}}},
                                             "resources": [{"name": "A", "max": 2, "reset": "short"}, {"name": "B", "max": 1}]})
        for kind in ("short", "long"):
            api("POST", f"/api/characters/{who}/rest", {"type": kind})
        api("PUT", f"/api/characters/{who}", {"hp": {"max": 20, "current": 1}})
        for kind in ("short", "long"):
            api("POST", f"/api/characters/{who}/rest", {"type": kind})
    api("POST", "/api/characters/{pc}/rest", {"type": "nap"})
    api("POST", "/api/characters/missing/rest", {"type": "long"})


PNG = bytes.fromhex("89504e470d0a1a0a0000000d4948445200000001000000010806000000"
                    "1f15c4890000000d49444154789c63f8ffff3f0005fe02fea7d6a5d00000000049454e44ae426082")


def upload(api: Session, path: str, data: bytes, mime: str):
    for name, real in api.ids.items():
        path = path.replace("{" + name + "}", real)
    req = urllib.request.Request(f"http://127.0.0.1:{api.port}{path}", data=data, method="PUT")
    req.add_header("Content-Type", mime)
    try:
        with urllib.request.urlopen(req, timeout=30) as res:
            status, out = res.status, json.loads(res.read())
    except urllib.error.HTTPError as e:
        status, out = e.code, None
    api.log.append((f"PUT {path} [{mime}]", status, out))
    return out


def raw_get(api: Session, path: str):
    for name, real in api.ids.items():
        path = path.replace("{" + name + "}", real)
    try:
        with urllib.request.urlopen(f"http://127.0.0.1:{api.port}{path}", timeout=30) as res:
            out = {"type": res.headers["Content-Type"], "cache": res.headers["Cache-Control"], "bytes": len(res.read())}
            status = res.status
    except urllib.error.HTTPError as e:
        status, out = e.code, None
    api.log.append((f"GET {path} (file)", status, out))


def handout_scenario(api: Session) -> None:
    api.keep("h", api("POST", "/api/handouts", {"title": "Map", "folder": "Maps"}))
    raw_get(api, "/api/handouts/{h}/file")
    upload(api, "/api/handouts/{h}/file", PNG, "image/png")
    raw_get(api, "/api/handouts/{h}/file")
    upload(api, "/api/handouts/{h}/file", b"<svg xmlns='http://www.w3.org/2000/svg'/>", "image/svg+xml; charset=utf-8")
    raw_get(api, "/api/handouts/{h}/file")
    upload(api, "/api/handouts/{h}/file", b"not an image", "text/plain")
    upload(api, "/api/handouts/{h}/file", b"", "image/png")
    upload(api, "/api/handouts/missing/file", PNG, "image/png")
    api("GET", "/api/handouts/{h}")
    api("GET", "/api/handouts")
    api("PUT", "/api/handouts/{h}", {"filename": "../questodian.db"})
    raw_get(api, "/api/handouts/{h}/file")
    api("DELETE", "/api/handouts/{h}")
    api("DELETE", "/api/handouts/{h}")
    api("PUT", "/api/projection", {"id": "{h}", "zoom": "1.5"})


def download(api: Session, path: str):
    """A download: its headers that matter and its body (JSON or text)."""
    for name, real in api.ids.items():
        path = path.replace("{" + name + "}", real)
    try:
        with urllib.request.urlopen(f"http://127.0.0.1:{api.port}{path}", timeout=30) as res:
            raw = res.read()
            out = {"disposition": res.headers["Content-Disposition"], "type": res.headers["Content-Type"]}
            status = res.status
    except urllib.error.HTTPError as e:
        api.log.append((f"GET {path} (download)", e.code, None))
        return None
    try:
        out["body"] = json.loads(raw)
    except ValueError:
        out["body"] = raw.decode()
    api.log.append((f"GET {path} (download)", status, out))
    return out["body"]


def transfer_scenario(api: Session) -> dict:
    keep = api.keep
    keep("tc", api("POST", "/api/campaigns", {"name": "Export Me! ünï", "edition": "2024"}))
    keep("tch", api("POST", "/api/characters", {
        "name": "Wren Ash", "campaign_id": "{tc}", "race": "Elf", "background": "Sage", "alignment": "Neutral Good",
        "player": "Kim", "classes": [{"name": "Wizard", "subclass": "Evoker", "level": 5}, {"name": "Bard", "level": 1}],
        "abilities": {"str": 8, "dex": 14, "con": 13, "int": 18, "wis": 12, "cha": 10}, "save_proficiencies": ["int", "wis"],
        "skills": {"arcana": "expertise", "animal-handling": "proficient", "sleight-of-hand": "half"},
        "jack_of_all_trades": True, "hp": {"max": 30, "current": 22, "temp": 4}, "exhaustion": 1,
        "proficiencies": {"armor": "", "weapons": "Daggers", "tools": "", "languages": "Common, Elvish"},
        "attacks": [{"name": "Dagger", "bonus": "+5", "damage": "1d4+2", "notes": "finesse"}],
        "spellcasting": {"ability": "int", "slots": {"2": {"max": 3, "used": 1}, "1": {"max": 4, "used": 0}},
                         "spells": [{"name": "Fire Bolt", "level": 0}, {"name": "Shield", "level": 1, "prepared": True},
                                    {"name": "Misty Step", "level": 2}]},
        "resources": [{"name": "Arcane Recovery", "current": 1, "max": 1, "reset": "long"}],
        "features": [{"name": "Sculpt Spells", "source": "Evoker", "description": "Protect allies."}],
        "inventory": [{"name": "Spellbook", "qty": 1, "equipped": True}, {"name": "Ink", "qty": 3}],
        "currency": {"gp": 15, "sp": 3}, "personality": {"traits": "Curious", "flaws": "Arrogant"},
        "appearance": "Tall", "backstory": "Raised in a library.", "notes": "Owes the guild."}))
    api("PUT", "/api/campaigns/{tc}", {"players": [{"name": "Kim", "character_id": "{tch}"}]})
    keep("ts", api("POST", "/api/sessions", {"campaign_id": "{tc}", "number": 3, "title": "The Vault", "date": "2026-11-01",
                                            "recap": "They found it.", "prep": "Traps.", "attendance": ["{tch}"],
                                            "loot": [{"name": "Wand", "qty": 1, "assigned_to": "Wren Ash"}, {"name": "Coins", "qty": 40}],
                                            "xp_awarded": 450}))
    api("POST", "/api/sessions/{ts}/log", {"text": "Door opened", "kind": "combat"})
    keep("tm", api("POST", "/api/monsters", {"campaign_id": "{tc}", "name": "Vault Guard"}))
    keep("tn", api("POST", "/api/npcs", {"campaign_id": "{tc}", "name": "Keeper", "stat_ref": "monster:{tm}"}))
    keep("te", api("POST", "/api/encounters", {"campaign_id": "{tc}", "session_id": "{ts}", "combatants": [
        {"name": "Wren", "kind": "pc", "character_id": "{tch}"}, {"name": "Guard", "monster_id": "{tm}"},
        {"name": "Keeper", "kind": "npc", "npc_id": "{tn}"}]}))
    keep("th", api("POST", "/api/handouts", {"campaign_id": "{tc}", "title": "Vault map"}))
    upload(api, "/api/handouts/{th}/file", PNG, "image/png")
    keep("tb", api("POST", "/api/handbook", {"title": "Vault Rules", "body": "No running.", "tags": "vault"}))

    download(api, "/api/export/characters/{tch}?format=md")
    download(api, "/api/export/sessions/{ts}?format=md")
    download(api, "/api/export/handbook/{tb}?format=md")
    download(api, "/api/export/npcs/{tn}?format=md")
    download(api, "/api/export/characters/{tch}")
    download(api, "/api/export/nothings/x")
    download(api, "/api/export/characters/missing")
    campaign = download(api, "/api/export?campaign_id={tc}")
    full = download(api, "/api/export")

    api("POST", "/api/import?mode=skip", full)
    api("POST", "/api/import?mode=overwrite", campaign)
    single = download(api, "/api/export/characters/{tch}")
    api("POST", "/api/import?mode=copy", single)
    api("POST", "/api/import?mode=copy", campaign)
    api("POST", "/api/import?mode=sideways", campaign)
    for bad in ([], {"format": "other"}, {"version": 99, "collections": {}}, {"version": "x"}, {"collections": []},
                {"collections": {"characters": {}}}, {"kind": "dragon", "data": {}}, {"nothing": 1},
                {"collections": {"handouts": [{"title": "Bad", "file_b64": "!!!"}]}},
                {"collections": {"handbook": [{"title": "ok"}], "characters": [{"classes": "bad"}]}}):
        api("POST", "/api/import", bad)
    api("POST", "/api/import", None)
    v1 = {"format": "questodian", "version": 1, "collections": {
        "campaigns": [{"id": "v1camp", "name": "Old"}],
        "sessions": [{"id": "v1sess", "campaign_id": "v1camp", "number": 4, "title": "Ambush",
                      "encounter": {"round": 2, "turn": 1, "active": True, "combatants": [{"name": "Orc", "hp": 15}]}},
                     {"id": "v1empty", "campaign_id": "v1camp", "encounter": {"combatants": []}}],
        "characters": [{"id": "v1pc", "campaign_id": "gone", "name": "Lost link"}], "junk": [1]}}
    api("POST", "/api/import", v1)
    api("GET", "/api/encounters")
    api("POST", "/api/import", {"kind": "handbook", "data": [{"title": "A"}, 5, {"title": "B"}]})
    api("POST", "/api/import", {"kind": "notes", "data": {"title": "plural kind works"}})
    api("POST", "/api/import/markdown", {"files": [{"filename": "house_rules.md", "text": "# Crits\r\n\nDouble dice."},
                                                  {"filename": "lore.MARKDOWN", "text": "no heading"}], "campaign_id": "{tc}"})
    api("POST", "/api/import/markdown", {"files": [{"text": "# x"}], "campaign_id": "missing"})
    for kind in KINDS:
        api("GET", f"/api/{kind}")
    return full


def cross_import(c: Checker, py_bundle, rust_bundle) -> None:
    """Each server imports the other's full backup; the records must come out the same."""
    for label, target_port, bundle in (("python bundle into rust", RUST_PORT, py_bundle),
                                       ("rust bundle into python", PY_PORT, rust_bundle)):
        c.count += 1
        status, summary = call(target_port, "POST", "/api/import?mode=overwrite", bundle)
        if status != 200:
            c.failures.append(f"{label}: status {status}: {str(summary)[:300]}")
            continue
        for kind in KINDS:
            by_id = {d["id"]: d for d in call(target_port, "GET", f"/api/{kind}")[1][kind]}
            for doc in bundle["collections"][kind]:
                doc = {k: v for k, v in doc.items() if k not in ("file_b64", "filename")}
                got = {k: v for k, v in by_id.get(doc["id"], {}).items() if k != "filename"}
                diff = first_difference(doc, got, f"{label} {kind}")
                if diff:
                    c.failures.append(diff)
                    break


def check_records(c: Checker) -> None:
    py, rust = Session(PY_PORT), Session(RUST_PORT)
    record_scenario(py)
    record_scenario(rust)
    game_scenario(py)
    game_scenario(rust)
    handout_scenario(py)
    handout_scenario(rust)
    py_bundle = transfer_scenario(py)
    rust_bundle = transfer_scenario(rust)
    py_ids: dict = {}
    rust_ids: dict = {}
    for (label, ps, pb), (_, rs, rb) in zip(py.log, rust.log):
        c.count += 1
        label = anonymize(label, py_ids)
        if c.verbose:
            print(f"  {ps} {rs}  {label}")
        if pb is False or rb is False:  # a property both servers must have (see game_scenario)
            c.failures.append(f"{label}: python {pb}, rust {rb}")
            continue
        pb, rb = anonymize(pb, py_ids), anonymize(rb, rust_ids)
        # Sessions are sorted by campaign id first, and ids are random: compare them as a set,
        # in the list and inside backups.
        key = lambda d: json.dumps(d, sort_keys=True)
        for body in (pb, rb):
            if ps != 200 or not isinstance(body, dict):
                continue
            if label == "GET /api/sessions":
                body["sessions"].sort(key=key)
            collections = body.get("body", {}).get("collections") if isinstance(body.get("body"), dict) else None
            if collections and "sessions" in collections:
                collections["sessions"].sort(key=key)
        if ps != rs:
            c.failures.append(f"{label}\n    status: python {ps}, rust {rs}\n    python: {str(pb)[:300]}\n    rust:   {str(rb)[:300]}")
            continue
        if ps >= 400:
            continue  # error wording may differ; the status is what the UI acts on
        diff = first_difference(strip(pb), strip(rb))
        if diff:
            c.failures.append(f"{label}\n    {diff}")
    cross_import(c, py_bundle, rust_bundle)


KINDS = ("campaigns", "sessions", "characters", "handbook", "scenes", "npcs", "monsters", "notes", "encounters",
         "trackers", "handouts")


def check_python_reads_rust_database(c: Checker, data_dir: Path, rust_records: dict) -> None:
    """The Python app opens the database the Rust server wrote and sees the same records."""
    from alembic.autogenerate import compare_metadata
    from alembic.migration import MigrationContext

    from questodian.db import Database
    from questodian.models import Base
    from questodian.repo import Repo

    c.count += 1
    db = Database(data_dir)
    with db.engine.connect() as conn:
        diffs = compare_metadata(MigrationContext.configure(conn), Base.metadata)
    if diffs:
        c.failures.append(f"schema of the Rust database differs from the Python models: {diffs}")
    with db.session() as s:
        repo = Repo(s)
        for kind in KINDS:
            diff = first_difference(rust_records[kind], repo.list(kind), kind)
            if diff:
                c.failures.append(f"Python reading the Rust database: {diff}")
    db.dispose()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("-v", "--verbose", action="store_true")
    parser.add_argument("--seed", type=int, default=1)
    args = parser.parse_args()
    if not RUST_BIN.exists():
        raise SystemExit(f"{RUST_BIN} not found: run `cargo build -p questodian-cli` first")
    with tempfile.TemporaryDirectory() as tmp:
        py = start([sys.executable, "-m", "questodian", "--data-dir", f"{tmp}/py", "serve", "--port", str(PY_PORT), "--no-browser"], PY_PORT, ROOT)
        rust = start([str(RUST_BIN), "--data-dir", f"{tmp}/rust", "serve", "--port", str(RUST_PORT), "--no-browser"], RUST_PORT, ROOT)
        try:
            checker = Checker(args.verbose)
            rng = random.Random(args.seed)
            check_tools(checker, rng)
            check_srd(checker, rng)
            check_records(checker)
            if (ROOT / "src/questodian/web/index.html").is_file():
                check_ui(checker)
            rust_records = {k: call(RUST_PORT, "GET", f"/api/{k}")[1][k] for k in KINDS}
        finally:
            py.terminate()
            rust.terminate()
            rust.wait()
        check_python_reads_rust_database(checker, Path(tmp) / "rust", rust_records)

        # A database the Rust app upgraded from revision 0001.
        upgraded = Path(tmp) / "upgraded"
        upgraded.mkdir()
        con = sqlite3.connect(upgraded / "questodian.db")
        con.executescript((ROOT / "crates/core/tests/fixtures/v0001.sql").read_text())
        con.close()
        rust = start([str(RUST_BIN), "--data-dir", str(upgraded), "serve", "--port", str(RUST_PORT), "--no-browser"], RUST_PORT, ROOT)
        try:
            rust_records = {k: call(RUST_PORT, "GET", f"/api/{k}")[1][k] for k in KINDS}
        finally:
            rust.terminate()
            rust.wait()
        check_python_reads_rust_database(checker, upgraded, rust_records)
    for f in checker.failures:
        print(f"DIFF {f}")
    print(f"{checker.count} requests, {len(checker.failures)} differences")
    return 1 if checker.failures else 0


if __name__ == "__main__":
    sys.exit(main())
