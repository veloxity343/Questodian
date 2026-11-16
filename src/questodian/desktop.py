"""Run Questodian as a desktop app: the server on a private port, shown in a native window.

Needs the optional ``desktop`` extra (pywebview). Without it, or without a usable
web view on this system, the app opens in the default browser instead.
"""

from __future__ import annotations

import socket
import threading
import time
import urllib.request
import webbrowser

import uvicorn

from . import __version__
from .app import create_app
from .db import Database


def _free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def _wait_until_up(url: str, timeout: float = 20.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            with urllib.request.urlopen(url + "api/meta", timeout=1):
                return
        except OSError:
            time.sleep(0.1)
    raise RuntimeError("the Questodian server didn't start")


class _Server(uvicorn.Server):
    def install_signal_handlers(self) -> None:  # runs in a thread; the window owns the process
        pass


class _Bridge:
    """Called from the page as ``window.pywebview.api.*``."""

    def __init__(self, base: str):
        self.base = base
        self.players = None

    def project(self, path: str) -> None:
        """Open, or bring back, the players' window (a handout shown on a second screen)."""
        import webview

        url = self.base + path.lstrip("/")
        if self.players is not None:
            try:
                self.players.load_url(url)
                self.players.show()
                return
            except Exception:  # the window was closed
                self.players = None
        self.players = webview.create_window("Questodian: players", url, width=1280, height=800, background_color="#000000")
        self.players.events.closed += lambda: setattr(self, "players", None)


def run(data_dir: str | None = None) -> int:
    db = Database(data_dir)
    port = _free_port()
    base = f"http://127.0.0.1:{port}/"
    server = _Server(uvicorn.Config(create_app(db), host="127.0.0.1", port=port, log_level="warning"))
    thread = threading.Thread(target=server.run, daemon=True)
    thread.start()
    _wait_until_up(base)

    try:
        import webview
    except ImportError:
        return _in_browser(base, thread, "install questodian[desktop] for a native window")

    window = webview.create_window(
        f"Questodian {__version__}", base, js_api=_Bridge(base), width=1600, height=960, min_size=(900, 600),
        background_color="#0e131b",
    )
    window.events.closed += lambda: setattr(server, "should_exit", True)
    try:
        webview.start(private_mode=False)  # keep localStorage (open tabs, preferences) between runs
    except Exception as exc:  # no usable web view here (e.g. Linux without GTK or Qt)
        return _in_browser(base, thread, f"no native window available: {exc}")
    server.should_exit = True
    thread.join(timeout=5)
    db.dispose()
    return 0


def _in_browser(base: str, thread: threading.Thread, why: str) -> int:
    print(f"Questodian {__version__} running at {base} ({why}). Press Ctrl+C to stop.")
    webbrowser.open(base)
    try:
        thread.join()
    except KeyboardInterrupt:
        pass
    return 0
