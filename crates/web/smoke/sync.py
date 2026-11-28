"""Sync in the web version, end to end, between two browsers (two devices), with Google stood in
for: its sign-in script answers at once, and Drive's API is a folder in this script's memory.
It checks the app's side (the Sync panel, sign-in, the Drive calls, the core's sync steps), not
Google's: that was tried on the real thing with the Drive test.

    make pwa QUESTODIAN_BASE=/Questodian/
    uv run --with playwright python crates/web/smoke/sync.py

Chromium only: in Playwright's WebKit the stand-ins don't take effect (the real Google script
loads instead; requests from a page a service worker runs seem not to be intercepted there).
Safari itself was tried with the real Google on an iPad. Exits non-zero if a check fails.
"""

import functools
import gzip
import http.server
import json
import re
import shutil
import sys
import tempfile
import threading
import uuid
from pathlib import Path

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[3]
DIST = ROOT / "frontend" / "dist-web"
SCOPE = "https://www.googleapis.com/auth/drive.appdata"
PNG = bytes.fromhex(
    "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c489"
    "0000000d49444154789c6360000002000154a24f5d0000000049454e44ae426082"
)
# Google's sign-in script, standing in: every request for a token is answered straight away.
GIS = """window.google = { accounts: { oauth2: {
  initTokenClient(config) { return { requestAccessToken() {
    window.__signIns = (window.__signIns || 0) + 1;
    setTimeout(() => config.callback({ access_token: "token-" + Date.now(), expires_in: 3599, scope: "%s" }), 50);
  } }; },
  revoke(token, done) { done && done(); },
} } };""" % SCOPE
CORS = {"Access-Control-Allow-Origin": "*", "Access-Control-Allow-Headers": "*", "Access-Control-Allow-Methods": "*",
        "Access-Control-Expose-Headers": "Location"}
failures = []


def check(what, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + what + (f": {detail}" if detail else ""))
    if not ok:
        failures.append(what)


class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


class Drive:
    """The app's hidden Drive folder: id -> (name, bytes)."""

    def __init__(self):
        self.files = {}
        self.uploads = {}
        self.lock = threading.Lock()

    def names(self):
        return sorted(name for name, _ in self.files.values())

    def handle(self, route):
        request = route.request
        url, method = request.url, request.method
        reply = lambda status, body=b"", headers=None, kind="application/json": route.fulfill(
            status=status, body=body, headers={**CORS, **(headers or {})}, content_type=kind)
        if method == "OPTIONS":
            return reply(204)
        if not (request.headers.get("authorization") or "").startswith("Bearer token-"):
            return reply(401, json.dumps({"error": {"message": "no token"}}))
        with self.lock:
            if method == "GET" and "/drive/v3/files?" in url:
                assert "spaces=appDataFolder" in url
                return reply(200, json.dumps({"files": [{"id": i, "name": n} for i, (n, _) in self.files.items()]}))
            match = re.search(r"/drive/v3/files/([^?]+)\?alt=media", url)
            if method == "GET" and match:
                return reply(200, self.files[match.group(1)][1], kind="application/octet-stream")
            if method == "POST" and "uploadType=multipart" in url:
                boundary = re.search(r"boundary=(\S+)", request.headers["content-type"]).group(1).encode()
                parts = request.post_data_buffer.split(b"--" + boundary)
                metadata = json.loads(parts[1].split(b"\r\n\r\n", 1)[1].rstrip(b"\r\n"))
                content = parts[2].split(b"\r\n\r\n", 1)[1][:-2]
                assert metadata["parents"] == ["appDataFolder"]
                return reply(200, json.dumps({"id": self.add(metadata["name"], content)}))
            if method == "POST" and "uploadType=resumable" in url:
                token = uuid.uuid4().hex
                self.uploads[token] = json.loads(request.post_data)["name"]
                return reply(200, b"{}", {"Location": f"https://www.googleapis.com/upload/resumable/{token}"})
            match = re.search(r"/drive/v3/files/([^?/]+)$", url)
            if method == "DELETE" and match:
                self.files.pop(match.group(1), None)
                return reply(204)
            match = re.search(r"/upload/resumable/(\w+)", url)
            if method == "PUT" and match:
                return reply(200, json.dumps({"id": self.add(self.uploads.pop(match.group(1)), request.post_data_buffer)}))
        return reply(400, json.dumps({"error": {"message": f"unexpected {method} {url}"}}))

    def add(self, name, content):
        file_id = uuid.uuid4().hex
        self.files[file_id] = (name, content)
        return file_id


def main():
    index = DIST / "index.html"
    if not index.exists():
        sys.exit("Build it first: make pwa")
    base = re.search(r'src="([^"]*)assets/', index.read_text()).group(1)
    site = Path(tempfile.mkdtemp())
    shutil.copytree(DIST, site / base.strip("/"), dirs_exist_ok=True)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Quiet, directory=str(site)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    url = f"http://127.0.0.1:{server.server_port}{base}"
    drive = Drive()
    profiles = []
    errors = []

    with sync_playwright() as p:
        engine = "chromium"

        def device():
            profiles.append(tempfile.mkdtemp())
            context = getattr(p, engine).launch_persistent_context(profiles[-1])
            context.route("https://accounts.google.com/gsi/client",
                          lambda route: route.fulfill(body=GIS, content_type="text/javascript", headers=CORS))
            context.route("https://www.googleapis.com/**", drive.handle)
            context.on("console", lambda m: m.type == "error" and "favicon" not in str(m.location) and errors.append(m.text))
            page = context.pages[0] if context.pages else context.new_page()
            page.goto(url)
            page.wait_for_selector("main", timeout=30000)
            page.wait_for_function("!!navigator.serviceWorker.controller", timeout=30000)
            return context, page

        def api(page, method, path, body=None):
            return page.evaluate("""([m, u, b]) => fetch(u, {method: m, headers: b ? {"content-type": "application/json"} : {}, body: b})
              .then(async r => ({status: r.status, body: await r.json().catch(() => null)}))""",
                                 [method, url + "api/" + path, json.dumps(body) if body is not None else None])

        def sync_panel(page):
            page.goto(url + "data")
            return page.locator("section.panel", has_text="Sync").first

        def synced(page, timeout=30000):
            page.get_by_text(re.compile(r"Synced (just now|a minute ago)")).wait_for(timeout=timeout)

        a_context, a = device()
        campaign = api(a, "POST", "campaigns", {"name": "Shared campaign"})["body"]
        handout = api(a, "POST", "handouts", {"title": "Shared map", "campaign_id": campaign["id"]})["body"]
        status = a.evaluate("""([u, b]) => fetch(u, {method: 'PUT', headers: {'content-type': 'image/png'}, body: new Uint8Array(b)}).then(r => r.status)""",
                            [url + f"api/handouts/{handout['id']}/file", list(PNG)])
        check("device A has a campaign and a handout with an image", status == 200, status)

        panel = sync_panel(a)
        panel.get_by_role("button", name="Connect Google Drive").click()
        synced(a)
        names = drive.names()
        check("connecting sends A's records and image to Drive",
              any(n.startswith("changes-") for n in names) and any(n.startswith("image-") for n in names), names)
        backups = [n for n in names if n.startswith("backup-")]
        check("... and makes the first cloud backup", len(backups) == 1 and backups[0].endswith(".json.gz"), backups)
        if backups:
            content = next(c for n, c in drive.files.values() if n == backups[0])
            bundle = json.loads(gzip.decompress(content))
            shared = bundle["collections"]["handouts"][0]
            check("... whose image is the folder's, not a copy",
                  "file_b64" not in shared and shared.get("sync_image", "").startswith("image-"), list(shared))

        b_context, b = device()
        panel = sync_panel(b)
        panel.get_by_role("button", name="Connect Google Drive").click()
        synced(b)
        check("B's panel says what came in", b.get_by_text("2 changes came in").count() == 1,
              panel.inner_text().replace("\n", " | "))
        got = api(b, "GET", f"campaigns/{campaign['id']}")
        check("device B gets A's campaign", got["status"] == 200 and got["body"]["name"] == "Shared campaign", got["status"])
        width = b.evaluate("u => new Promise(res => { const i = new Image(); i.onload = () => res(i.naturalWidth); i.onerror = () => res(-1); i.src = u; })",
                           url + f"api/handouts/{handout['id']}/file?v=1")
        check("... and the handout's image", width == 1, width)

        # An edit on B goes to A; A's open page shows it without a reload.
        api(b, "PUT", f"campaigns/{campaign['id']}", {"name": "Renamed on B"})
        b.get_by_role("button", name="Sync now").click()
        synced(b)
        a.goto(url + "campaigns")
        a.locator("main").get_by_text("Shared campaign").first.wait_for(timeout=15000)
        a.evaluate("document.dispatchEvent(new Event('visibilitychange'))")  # in view again: syncs
        try:
            a.locator("main").get_by_text("Renamed on B").first.wait_for(timeout=20000)
            shown = True
        except Exception:
            shown = False
        check("A shows B's edit when it syncs, without a reload", shown)

        # A change waits for a while after it's made, then goes by itself.
        # The app's saves go through api.ts, which announces each one; this save is the test's own.
        api(a, "POST", "notes", {"title": "Made on A"})
        a.evaluate("window.dispatchEvent(new Event('questodian:changed'))")
        b_notes = lambda: [n["title"] for n in api(b, "GET", "notes")["body"]["notes"]]
        found = False
        for _ in range(12):  # A sends it within ~15 s; B fetches when asked
            a.wait_for_timeout(2500)
            b.evaluate("document.dispatchEvent(new Event('visibilitychange'))")
            b.wait_for_timeout(1500)
            if "Made on A" in b_notes():
                found = True
                break
        check("a change is sent by itself shortly after it's made (15 s)", found, b_notes())

        # A character sheet open on B takes A's change when it arrives, so B's next edit doesn't undo it.
        character = api(a, "POST", "characters", {"name": "Wren"})["body"]
        a.evaluate("window.dispatchEvent(new Event('questodian:changed'))")
        sync_panel(a).get_by_role("button", name="Sync now").click()
        synced(a)
        b.evaluate("document.dispatchEvent(new Event('visibilitychange'))")
        b.wait_for_timeout(3000)
        b.goto(url + f"characters/{character['id']}")
        value_is = "v => [...document.querySelectorAll('main input')].some(i => i.value === v)"
        b.wait_for_function(value_is, arg="Wren", timeout=15000)
        api(a, "PUT", f"characters/{character['id']}", {"name": "Wren, renamed on A"})
        sync_panel(a).get_by_role("button", name="Sync now").click()
        synced(a)
        b.evaluate("document.dispatchEvent(new Event('visibilitychange'))")
        try:
            b.wait_for_function(value_is, arg="Wren, renamed on A", timeout=20000)
            refreshed = True
        except Exception:
            refreshed = False
        check("an open character sheet shows the other device's change", refreshed)

        # The hour's sign-in runs out: sync pauses, and one tap carries on.
        b.evaluate("""() => { const t = JSON.parse(localStorage.getItem('questodian-google-token'));
          t.expires = Date.now(); localStorage.setItem('questodian-google-token', JSON.stringify(t)); }""")
        panel = sync_panel(b)
        b.get_by_role("button", name="Sync now").click()
        b.get_by_text("Paused").wait_for(timeout=10000)
        before = b.evaluate("window.__signIns || 0")
        b.get_by_role("button", name="Carry on syncing").click()
        synced(b)
        check("after an hour it pauses, and one tap signs in again and syncs", b.evaluate("window.__signIns || 0") == before + 1)

        # A cloud backup restored on B undoes what came after it, on B and then on A.
        api(b, "POST", "notes", {"title": "Made after the backup"})
        panel = sync_panel(b)
        b.get_by_role("button", name="Back up now").click()
        synced(b)
        made = [n for n in drive.names() if n.startswith("backup-")]
        check("Back up now adds a backup, under a name of its own", len(made) == 2 and len(set(made)) == 2, made)
        first = [n for n in drive.names() if n.startswith("backup-")][0]
        api(b, "POST", "notes", {"title": "Made after both backups"})
        sync_panel(b)
        b.locator(".backup-list li").last.get_by_role("button", name="Restore").click()
        b.get_by_role("dialog").get_by_role("button", name="Restore").click()
        b.get_by_text("Restored").first.wait_for(timeout=30000)
        titles = lambda page: [n["title"] for n in api(page, "GET", "notes")["body"]["notes"]]
        check("restoring the oldest backup on B removes the notes made since", "Made after the backup" not in titles(b), titles(b))
        check("... and keeps a backup of what was there", any("before-restore" in n for n in drive.names()), drive.names())
        a.evaluate("document.dispatchEvent(new Event('visibilitychange'))")
        gone = False
        for _ in range(20):
            a.wait_for_timeout(1000)
            if "Made after the backup" not in titles(a) and "Made on A" not in titles(a):
                gone = True
                break
        check("... and sync takes the restore to A", gone, titles(a))
        print("     restored", first)

        b.get_by_role("button", name="Disconnect").click()
        b.get_by_role("button", name="Connect Google Drive").wait_for(timeout=10000)
        check("disconnecting keeps the data here", api(b, "GET", f"campaigns/{campaign['id']}")["status"] == 200)

        check("no console errors", not errors, errors)
        a_context.close()
        b_context.close()
    server.shutdown()
    shutil.rmtree(site)
    for profile in profiles:
        shutil.rmtree(profile, ignore_errors=True)
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
