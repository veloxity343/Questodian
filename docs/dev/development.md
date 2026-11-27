# Development

[Documentation](../README.md) › Development

## Setting up

You need [Rust](https://rustup.rs/) and [Node.js](https://nodejs.org/). For the desktop app on
Linux, you also need the WebKitGTK packages
[Tauri lists](https://v2.tauri.app/start/prerequisites/).

```bash
make install
```

That installs the frontend and Tauri CLI packages; Rust crates download on the first build.

The web build (the core in WebAssembly, for the browser version) and `make check`, which lints
it, also need the wasm32 target, `wasm-bindgen` at the version in `Cargo.lock`, and clang, which
compiles SQLite's C code:

```bash
rustup target add wasm32-unknown-unknown
```

```bash
cargo install wasm-bindgen-cli --version 0.2.130
```

```bash
sudo apt install clang llvm
```

(On macOS, Xcode's clang works.)

## Everyday tasks

Run `make` to list them all. The main ones:

| Command | What it does |
|---|---|
| `make dev` | The API on :8765 and the UI on http://localhost:5173, both reloading as you edit. One Ctrl+C stops both |
| `make check` | Rust format check, lint and tests; the frontend typecheck and tests. Run it before committing |
| `make run` | Build the UI and serve the app in your browser |
| `make app` | The desktop app (debug build) |
| `make desktop` | The desktop app and its installers for this OS, into `target/release/bundle/` |
| `make wasm` | The core built for a browser, into `target/web-pkg/` |
| `make pwa` | The web version (installable, works offline) into `frontend/dist-web/`. `QUESTODIAN_BASE=/Questodian/` builds it for GitHub Pages |

`make api` and `make web` run the two dev servers on their own.

To try a build against scratch data instead of your own, set `QUESTODIAN_DATA` to an empty folder
first. Setting `QUESTODIAN_LOG=1` makes the desktop app log every request.

## How the code is laid out

A Cargo workspace plus a React app:

- `crates/core/`: everything but the windows, shared by the desktop app, the command line and the
  web build
  - `server.rs`: the HTTP API (axum) and serving the built UI
  - `db.rs`, `migrations/`: opening the database, backups and schema upgrades. The same code
    runs in a browser, with one connection and no write-ahead log there (see the top of `db.rs`)
  - `files.rs`: handout images: the `handouts/` folder, or in a browser, `handouts.db`
  - `repo.rs`, `kinds.rs`: reading and writing records, the same way for every kind
  - `schemas.rs`: every record and request body, with defaults and lenient input
  - `game.rs`: initiative tracker, rewards and rests
  - `rules.rs`: 5e numbers and derived character stats, per edition
  - `builder.rs`: the character builder's rules data, with the user's edits laid over it
  - `dice.rs`: the dice and arithmetic parser and evaluator
  - `srd.rs`: the bundled SRD, loading and search
  - `transfer.rs`: import and export (JSON bundles, Markdown)
- `crates/cli/`: the `questodian` command
- `crates/web/`: the core for a browser (WebAssembly): the API answered inside a Web Worker, with
  the database in the browser's own storage (OPFS). Only one tab or window can have it open at a
  time.
- `src-tauri/`: the desktop app. It serves the UI and API to its window through its own
  `questodian://` address, with nothing listening on the network. `updates.rs` is the update check.
- `frontend/src/`: the React UI
  - `dm/`: the DM screen (`map.tsx`: the map editor, its drawing and exports)
  - `views/`: the other pages (`characters.tsx` and `sheet-panels.tsx`: the character sheet;
    `builder.tsx`: the builder's panels; `rules-data.tsx`; `settings.tsx`)
  - `components/`: shared pieces (`board.tsx`: arrangeable panels; `splitter.tsx`: drag handles)
  - `context/`: app-wide state (dice and the quick roll, dialogs, update checks)
  - `lib/`: logic with no UI:
    - `builder.ts`: everything the character builder works out, and how;
    - `map.ts`: map data and grid maths;
    - `derive.ts`: a hand-filled sheet's numbers;
    - `hooks.ts`: shared hooks (saved preferences, drafts that save themselves, drag to reorder,
      pan and zoom);
    - `desktop.ts`: the desktop app's own addresses.
  - `web/`: the web version's start-up (only in `make pwa` builds). The first window to open
    becomes the host: its worker (`db-worker.ts`) runs the core from `crates/web`. The service
    worker (`sw.js`) passes every window's `/api/...` requests to the host, so images, the
    players' window and other tabs all work, and keeps the app's files for offline use.
    - **Taking over:** when the host window closes, the next window takes over (a Web Lock
      queues them). A host that goes into the background while another window waits hands over
      and queues again, since a browser may pause a background window.
    - **Saves sent again:** a request with no answer (its host closed) is sent again to the
      next host. Each save carries an id (`x-questodian-save`), and `crates/web` makes a save
      whose id it has seen only once, answering with the first answer (the `saved_requests`
      table, kept a day).
    - **Updates:** a new build installs in the background and takes over once every window of
      the old one has closed, so two versions never share the database.
    - **Refused:** API writes that arrive as page navigations (a form on another site).
    - **Self-test:** `selftest.ts` is the Data page's **Check this browser**: what the web version
      relies on, tried in the browser it runs in, with a report to copy.
    - **Backups:** `backup.ts`. On a touch screen, exports open the Share sheet (Save to Files)
      instead of downloading, and the Data link shows a dot a week after the last full backup.

    Icons and the manifest are in `frontend/web-public/`; `vite.config.ts` writes `sw.js` with
    the list of files to keep.
- `data/`: the bundled SRD and the builder's rules data: see [Bundled data](data.md)
- `scripts/`: the scripts that build `data/`

Nested sheet data (inventory, spells, the encounter and so on) is stored in JSON columns. Links
between records are real foreign keys: deleting a campaign deletes its sessions, and unlinks
characters and handbook entries.

## Tests

`make check` runs them all:
- Rust unit tests in each module;
- `crates/core/tests/api.rs`: the HTTP API end to end;
- `crates/core/tests/database.rs`: upgrading real old databases (see
  [Database and migrations](database.md));
- the frontend's Vitest tests (`*.test.ts` next to the code they test).

The web version has two tests in a real browser, not part of `make check`:
- the core alone: after `make wasm`, `uv run --with playwright python crates/web/smoke/smoke.py`;
- the whole app (the service worker, a second window taking over, handing over from the
  background, a save cut off by its host closing, the self-test, offline, an update deployed
  while it's open): after `make pwa QUESTODIAN_BASE=/Questodian/`,
  `uv run --with playwright python crates/web/smoke/pwa.py`.

UI changes should also be tried in a real browser, and in the desktop window for anything that
drags, scrolls or uses the system: some webviews behave differently from Chromium. Pointer events
work everywhere, HTML drag and drop doesn't, so reordering uses `useDragReorder` (`lib/hooks.ts`).

## Continuous integration

GitHub Actions (`.github/workflows/ci.yml`) runs on every push to `main` and on pull requests:
- the Rust format check, lint and tests;
- the web build, and its lint ("Web build": not required for merging into `main`, but releases
  need it, since the Release workflow runs all of these checks first);
- the frontend typecheck and tests;
- a release build of the `questodian` command, which is started to check the UI, SRD search and
  saving work. The binary is attached to each run.

`main` only takes changes through pull requests, with these three checks passing (they're named
"Backend tests", "Frontend typecheck" and "Build and smoke-test package"). Keep those job names:
the branch rules refer to them.

## Conventions

- The app is Questodian, and plain, specific wording is preferred in anything users read.
- Rules and numbers come from the bundled SRD for the right edition, never from memory.
- Schema changes always come with a migration and an upgrade test: see
  [Database and migrations](database.md).
