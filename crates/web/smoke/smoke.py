"""Smoke test of the web build in a real (headless) Chromium: the API answered in a worker, with
the database and handout images in browser storage.

    make wasm
    uv run --with playwright python crates/web/smoke/smoke.py

Uses Playwright's Chromium (`playwright install chromium` once, or set CHROMIUM to a browser).
Each run starts with empty browser storage. Exits non-zero if a check fails.
"""

import functools
import http.server
import json
import os
import shutil
import sys
import tempfile
import threading
from pathlib import Path

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[3]
PNG = bytes.fromhex(
    "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c489"
    "0000000d49444154789c6360000002000154a24f5d0000000049454e44ae426082"
)
failures = []


def check(what, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + what + (f": {detail}" if detail else ""))
    if not ok:
        failures.append(what)


class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


def serve(folder):
    handler = functools.partial(Quiet, directory=folder)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def call(page, method, url, body=None, ctype=None):
    return page.evaluate("([m, u, b, c]) => api(m, u, b, c)", [method, url, body, ctype])


def main():
    pkg = ROOT / "target" / "web-pkg"
    if not (pkg / "questodian_web.js").exists():
        sys.exit("Build it first: make wasm")
    site = tempfile.mkdtemp()
    for name in ("index.html", "worker.js"):
        shutil.copy(Path(__file__).parent / name, site)
    shutil.copytree(pkg, Path(site) / "pkg")
    server = serve(site)
    url = f"http://127.0.0.1:{server.server_port}/"

    with sync_playwright() as p:
        browser = p.chromium.launch(executable_path=os.environ.get("CHROMIUM") or None)
        context = browser.new_context()
        page = context.new_page()
        errors = []
        page.on("console", lambda m: m.type == "error" and errors.append(m.text))
        page.goto(url)

        version = call(page, "GET", "/sqlite")
        check("SQLite runs in the worker", version.get("status") == 200, version.get("text") or version.get("error"))
        check("the API answers", call(page, "GET", "/api/meta").get("status") == 200)

        campaign = json.loads(call(page, "POST", "/api/campaigns", json.dumps({"name": "Smoke test"}))["text"])
        npc = call(page, "POST", "/api/npcs", json.dumps({"name": "Mira", "campaign_id": campaign["id"]}))
        check("records are created", npc["status"] == 201, npc["status"])
        bad = call(page, "POST", "/api/npcs", json.dumps({"name": "x", "campaign_id": "nope"}))
        check("links are checked", bad["status"] == 400, bad["text"])

        handout = json.loads(call(page, "POST", "/api/handouts", json.dumps({"name": "Map"}))["text"])
        upload = page.evaluate(
            "([id, b]) => api('PUT', `/api/handouts/${id}/file`, new Uint8Array(b), 'image/png')",
            [handout["id"], list(PNG)],
        )
        check("an image uploads", upload["status"] == 200, upload["status"])
        image = call(page, "GET", f"/api/handouts/{handout['id']}/file")
        check("the image comes back", image["status"] == 200 and image["size"] == len(PNG), image["size"])

        roll = call(page, "GET", "/api/roll?expr=2d6%2B3")
        check("dice roll", roll["status"] == 200)
        search = call(page, "GET", "/api/srd/search?q=fireball&edition=2024")
        check("SRD search", search["status"] == 200 and "Fireball" in search["text"])

        export = call(page, "GET", "/api/export")
        bundle = json.loads(export["text"])
        check("export includes the image", bool(bundle["collections"]["handouts"][0].get("file_b64")))

        page.reload()
        campaigns = json.loads(call(page, "GET", "/api/campaigns")["text"])["campaigns"]
        check("records survive a reload", len(campaigns) == 1, len(campaigns))
        image = call(page, "GET", f"/api/handouts/{handout['id']}/file")
        check("images survive a reload", image["size"] == len(PNG), image["size"])
        for _ in range(10):  # each open resets the storage's free slots, so this never runs out
            page.reload()
        again = call(page, "GET", "/api/campaigns")
        check("ten more opens", again.get("status") == 200, again.get("error", ""))

        call(page, "DELETE", f"/api/handouts/{handout['id']}")
        gone = call(page, "GET", f"/api/handouts/{handout['id']}/file")
        check("deleting a handout removes its image", gone["status"] == 404, gone["status"])
        imported = call(page, "POST", "/api/import?mode=copy", json.dumps(bundle))
        check("import", imported["status"] == 200, imported["text"][:100])
        handouts = json.loads(call(page, "GET", "/api/handouts")["text"])["handouts"]
        image = call(page, "GET", f"/api/handouts/{handouts[0]['id']}/file") if handouts else {"size": 0}
        check("an imported image is stored", image["size"] == len(PNG), image["size"])

        second = context.new_page()
        second.goto(url)
        refused = call(second, "GET", "/api/campaigns")
        check("a second tab can't open the database", "error" in refused, refused.get("error", "")[:80])

        check("no console errors", not errors, errors)
        browser.close()
    server.shutdown()
    shutil.rmtree(site)
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
