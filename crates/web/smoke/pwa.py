"""The web version end to end in a real (headless) Chromium: the built app (frontend/dist-web),
its service worker relaying the API to the window that hosts the database, a second window
borrowing it and taking over, and working offline.

    make pwa QUESTODIAN_BASE=/Questodian/
    uv run --with playwright python crates/web/smoke/pwa.py

It also deploys a second build over the first (rebuilding the frontend with the same base) to
check that an open app keeps working on its version and the new one takes over afterwards.

Uses Playwright's Chromium (`playwright install chromium` once, or set CHROMIUM to a browser), or
its WebKit with BROWSER=webkit (`playwright install --with-deps webkit`; WebKit is the engine
Safari is built on, but this isn't Safari).
Each run starts with empty browser storage. Exits non-zero if a check fails.
"""

import functools
import http.server
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import threading
from pathlib import Path

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[3]
DIST = ROOT / "frontend" / "dist-web"
PNG = bytes.fromhex(
    "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c489"
    "0000000d49444154789c6360000002000154a24f5d0000000049454e44ae426082"
)
BUILD = """() => new Promise(res => { const c = new MessageChannel(); c.port1.onmessage = e => res(e.data);
  navigator.serviceWorker.controller.postMessage({type: "build"}, [c.port2]); })"""
FETCH = """([m, u, b]) => fetch(u, {method: m, headers: b ? {"content-type": "application/json"} : {}, body: b})
  .then(async r => ({status: r.status, text: await r.text()}))"""
failures = []


def check(what, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + what + (f": {detail}" if detail else ""))
    if not ok:
        failures.append(what)


class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


def main():
    index = DIST / "index.html"
    if not index.exists():
        sys.exit("Build it first: make pwa")
    base = re.search(r'src="([^"]*)assets/', index.read_text()).group(1)  # e.g. /Questodian/
    site = Path(tempfile.mkdtemp())
    shutil.copytree(DIST, site / base.strip("/") if base.strip("/") else site, dirs_exist_ok=True)
    handler = functools.partial(Quiet, directory=str(site))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    url = f"http://127.0.0.1:{server.server_port}{base}"

    def api(page, method, path, body=None):
        return page.evaluate(FETCH, [method, url + "api/" + path, body])

    with sync_playwright() as p:
        engine = os.environ.get("BROWSER", "chromium")
        print("     browser:", engine)
        if engine == "chromium":
            browser = p.chromium.launch(executable_path=os.environ.get("CHROMIUM") or None)
        else:
            browser = getattr(p, engine).launch()
        context = browser.new_context()
        errors = []
        context.on("console", lambda m: m.type == "error" and "403" not in m.text and "favicon.ico" not in str(m.location) and errors.append(f"{m.text} ({m.location.get('url')})"))
        context.on("response", lambda r: r.status == 404 and errors.append(f"404: {r.url}"))

        host = context.new_page()
        host.goto(url)
        host.wait_for_selector("main", timeout=30000)
        check("the service worker controls the page", host.evaluate("!!navigator.serviceWorker.controller"))
        created = api(host, "POST", "campaigns", json.dumps({"name": "Web campaign", "edition": "2024"}))
        check("the API answers through the service worker", created["status"] == 201, created["status"])

        handout = json.loads(api(host, "POST", "handouts", json.dumps({"name": "Map"}))["text"])
        file_url = url + f"api/handouts/{handout['id']}/file"
        status = host.evaluate(
            "([u, b]) => fetch(u, {method: 'PUT', headers: {'content-type': 'image/png'}, body: new Uint8Array(b)}).then(r => r.status)",
            [file_url, list(PNG)],
        )
        check("an image uploads", status == 200, status)
        width = host.evaluate(
            "u => new Promise(res => { const i = new Image(); i.onload = () => res(i.naturalWidth); i.onerror = () => res(-1); i.src = u; })",
            file_url + "?v=1",
        )
        check("an <img> shows it", width == 1, width)

        host.goto(url + "campaigns")
        host.locator("main").get_by_text("Web campaign").first.wait_for(timeout=10000)  # not the header's campaign menu
        check("the UI shows the data", True)
        host.get_by_role("link", name="Characters").first.click()
        host.wait_for_url("**/characters", timeout=10000)
        check("menu links stay under the base path", host.url == url + "characters", host.url)

        guest = context.new_page()
        guest.goto(url + "characters")  # a deep link
        guest.wait_for_selector("main", timeout=30000)
        check("a second window borrows the database", api(guest, "GET", "campaigns")["status"] == 200)
        before = len(json.loads(api(guest, "GET", "campaigns")["text"])["campaigns"])
        form = context.new_page()  # a form posting to the API, as another website could
        form.goto(url + "characters")
        form.wait_for_selector("main", timeout=30000)
        with form.expect_navigation(url=url + "api/campaigns"):
            form.evaluate("""u => { const f = document.createElement("form"); f.method = "POST"; f.enctype = "text/plain";
          f.action = u; const i = document.createElement("input"); i.name = '{"name":"x","z":"'; i.value = '"}';
          f.append(i); document.body.append(f); f.submit(); }""", url + "api/campaigns")
        after = len(json.loads(api(guest, "GET", "campaigns")["text"])["campaigns"])
        check("a form posting to the API is refused", after == before and "refused" in form.content(), after)
        form.close()
        players = context.new_page()
        players.goto(url + "project")
        players.wait_for_function("document.getElementById('root').childElementCount > 0", timeout=30000)
        check("the players' view loads", True)
        players.close()
        quick = context.new_page()  # saves, then closes without waiting for the answer
        quick.goto(url + "characters")
        quick.wait_for_selector("main", timeout=30000)
        quick.evaluate("u => { fetch(u, {method: 'POST', headers: {'content-type': 'application/json'}, body: JSON.stringify({title: 'Quick'})}); }", url + "api/handouts")
        quick.close()
        guest.wait_for_timeout(1500)
        names = [e.get("title") for e in json.loads(api(guest, "GET", "handouts")["text"]).get("handouts", [])]
        check("a save from a window that closes straight away is kept", "Quick" in names, names)

        # A host going into the background hands over to a waiting window, and queues again.
        def hosting(page):
            return page.evaluate("document.documentElement.dataset.host === 'yes'")

        def seen(page, hidden):
            page.evaluate("""h => { Object.defineProperty(document, "hidden", {configurable: true, get: () => h});
              document.dispatchEvent(new Event("visibilitychange")); }""", hidden)

        def wait_until(test, tries=40):
            for _ in range(tries):
                if test():
                    return True
                guest.wait_for_timeout(250)
            return False

        check("one window has the data", hosting(host) and not hosting(guest))
        seen(host, True)
        check("in the background, it hands over to the waiting window", wait_until(lambda: hosting(guest) and not hosting(host)))
        check("... which answers", api(host, "GET", "campaigns")["status"] == 200)
        seen(host, False)
        seen(guest, True)
        check("and back, when that one goes into the background", wait_until(lambda: hosting(host) and not hosting(guest)))
        seen(guest, False)

        # A save in flight when its host hands over (longer than the second it waits for answers): sent
        # again to the new host, made once, promptly.
        guest.evaluate("""u => { const t = performance.now(); window.__moved = fetch(u, {method: "POST", headers: {"content-type": "application/json"},
          body: JSON.stringify({format: "questodian", version: 3, collections: {handbook: Array.from({length: 70000},
            (_, i) => ({title: "Moved " + i}))}})}).then(r => [r.status, Math.round(performance.now() - t)]); }""", url + "api/import?mode=copy")
        guest.wait_for_timeout(1200)  # under way in the host's worker
        seen(host, True)
        status, ms = guest.evaluate("window.__moved")
        cut = host.evaluate("document.documentElement.dataset.cutOff")
        moved = [e for e in json.loads(api(guest, "GET", "handbook")["text"]).get("handbook", []) if e.get("title", "").startswith("Moved ")]
        check("a save cut off by a handover is sent again, made once, promptly",
              cut == "1" and status in (200, 409) and len(moved) == 70000 and ms < 30000,
              f"cut off {cut}, {status}, {len(moved)} of 70000, {ms} ms")
        seen(host, False)
        wait_until(lambda: hosting(guest))

        host.goto(url + "data")
        host.get_by_role("button", name="Check this browser").click()
        host.get_by_text("Everything checked works.").or_(host.get_by_text("failed.")).first.wait_for(timeout=60000)
        report = host.locator(".browser-check").inner_text()
        check("the self-test passes", host.get_by_text("Everything checked works.").count() == 1, report.replace("\n", " | "))
        left = [e.get("title") for e in json.loads(api(guest, "GET", "handouts")["text"]).get("handouts", [])]
        check("... and leaves nothing behind", not any("self-test" in (t or "") for t in left), left)

        # A save in flight when its host closes is sent again, and made once: a big import from
        # the guest, with the host closed while it works.
        guest.evaluate("""u => { window.__import = fetch(u, {method: "POST", headers: {"content-type": "application/json"},
          body: JSON.stringify({format: "questodian", version: 3, collections: {handbook: Array.from({length: 3000},
            (_, i) => ({title: "Bulk " + i, body: "x".repeat(200)}))}})}).then(r => r.status); }""", url + "api/import?mode=copy")
        guest.wait_for_timeout(150)
        host.close()
        status = guest.evaluate("window.__import")
        bulk = [e for e in json.loads(api(guest, "GET", "handbook")["text"]).get("handbook", []) if e.get("title", "").startswith("Bulk ")]
        check("a save cut off by its host closing is made once", status in (200, 409) and len(bulk) == 3000, f"{status}, {len(bulk)} of 3000")
        guest.wait_for_timeout(1000)
        after = api(guest, "GET", "campaigns")
        count = len(json.loads(after["text"]).get("campaigns", [])) if after["status"] == 200 else after["text"][:100]
        check("when the host window closes, another takes over", count == 1, count)

        context.set_offline(True)
        guest.reload()
        guest.wait_for_selector("main", timeout=30000)
        check("opens offline", api(guest, "GET", "campaigns")["status"] == 200)
        guest.goto(url + "library")
        guest.wait_for_selector("main", timeout=30000)
        search = api(guest, "GET", "srd/search?q=fireball&edition=2024")
        check("a deep link and SRD search work offline", search["status"] == 200)
        context.set_offline(False)

        # A new version deployed while the app is open (the old hashed files go, as on GitHub Pages).
        first = guest.evaluate(BUILD)
        site_base = site / base.strip("/") if base.strip("/") else site
        env = {**os.environ, "QUESTODIAN_BASE": base}
        subprocess.run(["npm", "--prefix", str(ROOT / "frontend"), "run", "build:web"], env=env, check=True, capture_output=True)
        shutil.rmtree(site_base)
        shutil.copytree(DIST, site_base)
        guest.reload()
        guest.wait_for_selector("main", timeout=30000)
        check("after a deploy, an open app keeps its version", guest.evaluate(BUILD) == first)
        check("... and keeps working", api(guest, "GET", "campaigns")["status"] == 200)
        waiting = "navigator.serviceWorker.getRegistration().then(r => !!(r && r.waiting))"
        for _ in range(60):
            if guest.evaluate(waiting):
                break
            guest.wait_for_timeout(500)
        check("the new version is downloaded and waiting", guest.evaluate(waiting))
        guest.close()
        # Watch from a page outside the app (not one of its windows) until the new version takes over.
        outside = context.new_page()
        outside.goto(url.rsplit("/", 2)[0] + "/")
        state = f"navigator.serviceWorker.getRegistration({json.dumps(url)}).then(r => r ? [!!r.waiting, r.active && r.active.state] : null)"
        for _ in range(40):
            if outside.evaluate(state) == [False, "activated"]:
                break
            outside.wait_for_timeout(250)
        print("     registration (waiting, active state):", outside.evaluate(state))
        outside.close()
        fresh = context.new_page()
        fresh.goto(url)
        fresh.wait_for_selector("main", timeout=30000)
        for _ in range(20):  # the new version takes over once the old one's windows are closed
            if fresh.evaluate(BUILD) != first:
                break
            fresh.wait_for_timeout(500)
            fresh.reload()
            fresh.wait_for_selector("main", timeout=30000)
        check("once every window has closed, the new version is used", fresh.evaluate(BUILD) != first)
        check("... with the same data", len(json.loads(api(fresh, "GET", "campaigns")["text"])["campaigns"]) == 1)

        check("no console errors", not errors, errors)
        browser.close()
    server.shutdown()
    shutil.rmtree(site)
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
