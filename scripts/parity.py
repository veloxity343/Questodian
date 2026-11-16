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
            if (ROOT / "src/questodian/web/index.html").is_file():
                check_ui(checker)
        finally:
            py.terminate()
            rust.terminate()
    for f in checker.failures:
        print(f"DIFF {f}")
    print(f"{checker.count} requests, {len(checker.failures)} differences")
    return 1 if checker.failures else 0


if __name__ == "__main__":
    sys.exit(main())
