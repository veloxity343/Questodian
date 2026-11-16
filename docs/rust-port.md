# Rust + Tauri port: parity plan

The plan for moving Questodian's backend from Python (FastAPI, SQLAlchemy, Alembic,
pywebview) to Rust (axum, SeaORM on SQLite, Tauri 2). The React UI stays. The Python
app keeps working, and stays the reference, until the Rust one passes everything
listed here.

**Parity** means: the same database file opens unchanged, the same exports import
both ways, and the React UI can't tell which backend it is talking to.

## 1. What has to be ported

About 3,300 lines of Python, 69 tests.

| Python | Lines | Rust module | What it does | Port notes |
|---|---:|---|---|---|
| `dice.py` | 544 | `dice` | Tokenizer, parser, evaluator for dice and arithmetic | Pure logic. Keep the limits (1000 dice, 100,000 sides, 100 explosions, 500 chars). Output numbers are cleaned: whole floats become ints, others rounded to 10 places (`_clean`); breakdown text uses `%.6g` (`_fmt`). |
| `rules.py` | 233 | `rules` | 5e tables and derived stats (`derive`, slots, encounter budget) | Pure. Tables are data; check every number with `rules-lawyer` against the SRD, not against the Python. |
| `srd.py` | 136 | `srd` | Loads `srd.json.gz` / `srd-2024.json.gz`, search with ranking and filters | Embed both files in the binary. Ranking is exact > prefix > contains > all words in name > all words in text, then by name. Filters compare lowercased strings and match inside lists. |
| `schemas.py` | 417 | `schemas` | Pydantic models: defaults for every field, unknown keys ignored | The biggest parity risk; see section 4. |
| `models.py` | 255 | `kinds` | 11 tables | One description per kind; column types are read from the database (see section 10). |
| `repo.py` | 187 | `repo` | Generic list/get/save/update/delete for 11 kinds, link checks, ordering | One generic implementation on sqlx, driven by `kinds`. |
| `db.py`, `migrate.py`, `migrations/` | 300 | `db` | Data folder, pragmas, Alembic upgrade, backup before migrating | See section 3. |
| `game.py` | 305 | `game` | Encounter actions (14), XP/loot awards, session log, short/long rests, HP sync to PCs | Port with its tests first. |
| `transfer.py` | 334 | `transfer` | Export bundle v2 (with base64 images), v1 upgrade, import in overwrite/skip/copy mode in one transaction, id remapping, Markdown in and out | The Markdown output should match the Python output byte for byte. |
| `app.py` | 375 | `server` | HTTP routes, write guard, error shape, SPA serving | axum `Router`. |
| `cli.py` | 181 | `cli` (clap) | `serve`, `app`, `roll`, `calc`, `srd`, `export`, `import` | Same commands and flags. |
| `desktop.py` | 103 | `src-tauri` | Native window, players' window | Tauri replaces pywebview. |

### HTTP API (everything under `/api`)

| Group | Routes |
|---|---|
| Tools | `GET meta`, `GET roll?expr=&times=`, `POST roll`, `POST stats`, `POST derive`, `POST slots`, `GET encounter-budget` |
| SRD | `GET srd/categories`, `GET srd/search` (any extra query key is a data filter), `GET srd/entry` (all take `edition=2014\|2024`) |
| Records ×11 | `GET /{kind}?campaign_id&session_id&kind`, `POST /{kind}` (201), `GET/PUT/DELETE /{kind}/{id}` for campaigns, sessions, characters, handbook, scenes, npcs, monsters, notes, encounters, trackers, handouts |
| Game | `POST encounters/{id}/action`, `POST sessions/{id}/award`, `POST sessions/{id}/log`, `POST characters/{id}/rest` |
| Handouts | `PUT handouts/{id}/file` (raw image body, 15 MB, png/jpeg/gif/webp/svg), `GET handouts/{id}/file`, `DELETE handouts/{id}` (also removes the file) |
| Projection | `GET/PUT projection`: in memory, `seq` increments on every PUT, the players' window polls it |
| Import/export | `GET export?campaign_id`, `GET export/{kind}/{id}?format=json\|md`, `POST import?mode=`, `POST import/markdown` |

Behaviour around the routes that must carry over:

- Errors are `{"detail": "..."}`: not found → 404, bad input → 400. FastAPI's own validation errors are 422 with a list; the UI handles both (`frontend/src/api.ts`).
- Writes (`POST/PUT/DELETE` on `/api/`) with an `Origin` whose host differs from `Host` get 403. Bodies over 20 MB get 413.
- Any other path serves a file from the built UI, else `index.html`.

## 2. Architecture

```
questodian/
  frontend/          React UI, unchanged apart from the native bridge (below)
  crates/core/       library: dice, rules, srd, schemas, entity, repo, db, game, transfer, server (axum Router)
  crates/cli/        `questodian` binary: serve, roll, calc, srd, export, import
  src-tauri/         the desktop/mobile app; depends on core
  src/questodian/    Python, until it's retired
```

All logic lives in `core`, which knows nothing about Tauri. That keeps it testable
with plain `cargo test`, and lets the CLI and the optional web server reuse it.

**How the UI reaches the API inside Tauri.** The UI calls `fetch("/api/...")` with
relative URLs. The plan is to keep that unchanged:

- **Option A (preferred): custom protocol, no port.** Tauri serves the app through
  its own URL scheme and hands each request to the axum `Router` in-process (axum
  routers can be called without a TCP listener). Nothing listens on the network, and
  it should work the same on mobile. Confirmed in the docs:
  `Builder::register_asynchronous_uri_scheme_protocol` exists in Tauri 2.12.5, and
  axum's `Router` is a tower `Service` that can be called with `oneshot` (as in
  axum's own testing example). The page's origin differs by OS:
  `questodian://localhost` on macOS, iOS and Linux, and `http://questodian.localhost`
  on Windows and Android. The write guard must accept the app's own origin on each
  of them. Not yet tried on a phone.
- **Option B: localhost server**, like today's pywebview build, via the official
  `tauri-plugin-localhost`. Its docs warn it "brings considerable security risks"
  and say "If in doubt, use the default custom protocol implementation".

Either way, the **players' screen** (other devices on the network) needs a real
HTTP
server. That stays an opt-in `serve --host 0.0.0.0`, like today, using the same
`Router`.

**Native bridge.** One UI change: `frontend/src/dm/docs2.tsx:295` calls
`window.pywebview.api.project(url)` to open the players' window. It gets a Tauri
equivalent (open or reuse a second window at that URL), keeping `window.open` as
the browser fallback.

## 3. Database and migrations

Facts from a database made by the current app (checked 2026-11-16):

- Alembic stamps the version in `alembic_version` (currently `0002`).
- JSON columns are declared `JSON` and stored as TEXT (`typeof()` is `text`).
  SeaORM 2.0.4 reads and writes these as `serde_json::Value` (tested on a scratch
  database). One catch: a column declared `JSON` has numeric affinity in SQLite,
  so a bare number (`3`) is stored as INTEGER and SeaORM then fails to read it.
  Every JSON column here holds an object or a list, so this shouldn't occur. A test
  should still scan the fixture databases for non-text JSON values.
- SQLite is compiled into the binary (sqlx's `sqlite` feature turns on
  `libsqlite3-sys/bundled`), so users need no system SQLite. Only one
  `libsqlite3-sys` version may be in the build, so no second SQLite crate.
- Booleans are stored as integers 0/1. Timestamps are ISO strings like
  `2026-11-16T12:00:00+00:00`. Ids are 12 hex characters, but any
  `[A-Za-z0-9_-]{1,64}` is valid (imports keep their ids).
- Columns added later sit at the end of the table (`campaigns.edition`), so read
  columns by name only.
- Every connection must run `PRAGMA foreign_keys = ON`, or deleting a campaign
  stops cascading. The app also sets `journal_mode = WAL`.

**Migration ownership.**

1. **While both apps exist:** Alembic stays the only place schema changes are
   written. The Rust app reads `alembic_version`. If it's the revision Rust was
   built for, it opens the database. If it's older, it tells the user to open it
   once with the current app. If it's newer or unknown, it refuses and changes
   nothing.
2. **After the switch:** migrations move to Rust and keep writing
   `alembic_version` (`0003`, `0004`, …). That way one table says which schema a
   file has, whichever app made it. The backup-before-migrating behaviour
   (`questodian.db.backup-<rev>-<time>`) moves too. We don't use
   sea-orm-migration, which tracks versions in its own `seaql_migrations` table,
   so there's never two sources of truth.
3. A database from before Alembic existed (tables, no `alembic_version`) is
   treated as `0001`, as `migrate.py` does now.

`data-guardian` reviews this part against real older databases before it ships.

## 4. Parity traps

Places where a straightforward Rust port would behave differently:

1. **Pydantic converts input; serde doesn't.** Today `"xp": "300"` saves as `300`,
   and `"inspiration": "yes"` saves as `true` (checked). `null` strings become `""`,
   and `tags` given as `"a, b"` becomes `["a", "b"]`. Every field needs a default,
   unknown keys must be ignored, and the same conversions must apply. The plan is a
   small set of lenient deserializers shared by all schemas, with tests built from
   what Pydantic actually does.
2. **422 vs 400.** FastAPI rejects some bad bodies before our code runs. Rust
   should return the same status for the same input, or we decide on purpose to
   change it.
3. **Update is a shallow merge** of top-level fields over the stored record, then
   full validation and link checks (`repo.update`).
4. **List order** is per kind, using case-insensitive `lower()` sorts. SQLite's
   `lower()` only folds ASCII, so use SQL, not Rust's Unicode lowercase.
5. **Numbers in JSON.** Dice totals are ints when whole. Projection `zoom/x/y` are
   floats. Python prints `1.0` where serde may print `1`; check the UI doesn't care.
6. **Random numbers differ.** Rust can't reproduce Python's random sequence, so
   dice tests check structure and bounds, not exact rolls.
7. **Export JSON** must import into the Python app and the other way round.
   Formatting can differ (Python writes `", "`), but content can't.
8. **Imports are one transaction:** a bad file changes nothing, including image
   files written to the handouts folder.

## 5. Improving while porting

Parity is about behaviour (responses, database, exports), not code shape. Where
there's a more efficient or simpler way, the Rust version uses it, as long as it
reads at least as clearly as the Python. Readable beats clever. Candidates seen so
far:

- **Encounter actions** (`game.encounter_action`, one 140-line `if/elif`): an
  `enum Action` deserialized from the request, with one small function per action.
  The compiler then checks every action is handled.
- **Dice parser**: one recursive-descent parser reading characters directly,
  instead of regex tokenizing then parsing. Fewer passes, and errors can point at
  the exact position.
- **SRD**: load each edition once into a struct with an id → entry map, so
  `srd/entry` is a lookup instead of a scan. Search keeps its ranking but filters
  by category before anything else.
- **Records**: one table describing each kind (table, ordering, filters, links),
  instead of `match` statements spread across `repo.py`.
- **Lenient input** (section 4, item 1): written once as reusable deserializers,
  not per field.
- **Rules data**: tables as `const` arrays, checked against the SRD by
  `rules-lawyer` as they're written (an audit of the current Python and UI rules
  is running now).

Each deliberate change is listed in the PR that makes it.

## 6. How parity gets checked

1. **Same API tests, both backends.** Make `tests/test_api.py` (and the API parts of
   `test_transfer.py`, `test_repo.py`) runnable against a base URL. Run the suite
   against Python, then against the Rust server. Green on both means the routes
   agree.
2. **Golden diff for deterministic endpoints.** A script sends the same requests to
   both servers and diffs the JSON: `meta`, SRD categories, search, and entry (both
   editions, a few hundred queries and filters), `derive`, `slots`,
   `encounter-budget`, Markdown exports.
3. **Real databases.** Check in small fixture databases made by the Python app at
   revisions `0001` (pre-Alembic too) and `0002`, with data in every table. Rust
   tests open them, read every record and compare to the Python export of the same
   file.
4. **Rust unit tests** for each ported Python test (dice, rules, game, transfer).
5. **UI.** `ui-tester` clicks through the main flows in the Tauri build.

## 7. Milestones

Each one ends green on `cargo test`, the existing `make check`, and its parity checks.

| # | Milestone | Done when |
|---|---|---|
| M0 | **Scaffold.** Cargo workspace, `src-tauri` showing the current built UI, CI job running `cargo test` and `cargo clippy` | `cargo tauri dev` opens the UI; CI green |
| M1 | **No-database endpoints.** dice, rules, SRD, `meta`, `roll`, `stats`, `derive`, `slots`, `encounter-budget`, SRD routes, error shape, write guard, UI serving | Golden diff identical; Rules & SRD and Dice pages work in Tauri |
| M2 | **Database and records.** open/check `alembic_version`, pragmas, entities, lenient schemas, generic CRUD for all 11 kinds | API tests for records pass on Rust; fixture databases read back identically |
| M3 | **Game logic.** encounter actions, awards, log, rests, PC HP sync | `game` tests ported and passing; DM screen flows pass `ui-tester` |
| M4 | **Handouts and players' window.** upload/serve/delete files, projection, native second window, opt-in LAN server | Players' screen works from a second window and from another device |
| M5 | **Import/export.** bundle v1/v2, modes, id remapping, images, Markdown | Round trip Python→Rust→Python identical; `data-guardian` review |
| M6 | **CLI.** `serve`, `roll`, `calc`, `srd`, `export`, `import` | Same output as the Python CLI for the same commands |
| M7 | **Release.** `tauri-action` matrix (Windows, macOS Intel/ARM, Linux x64/ARM on Ubuntu 22.04), migrations move to Rust, Python retired | Installers tested on each OS |

Mobile (iOS/Android) comes after M7.

## 8. Decisions

Made on 2026-11-16 while the user was away. Each is easy to revisit.

1. **Option A** (custom protocol, no port) for the app window. The opt-in
   `serve` command still runs a real HTTP server for the players' screen and
   browser use.
2. **Cargo workspace at the repo root**, next to `frontend/` and `src/`:
   `crates/core`, `crates/cli`, `src-tauri`.
3. **Keep the 400/422 split** as FastAPI does it. It's parity by default, and the
   UI already handles both.
4. **Keep `questodian` as its own CLI binary**, without WebKit, so it runs on servers
   and in CI.

## 9. Verified versions and APIs

Checked on 2026-11-16 against crates.io, npm, docs.rs, v2.tauri.app and
sea-ql.org. Local toolchain: rustc 1.100.0, which meets every minimum below.

| Package | Latest stable | Minimum Rust |
|---|---|---|
| tauri / tauri-build | 2.12.5 / 2.7.4 | 1.90 |
| @tauri-apps/cli, @tauri-apps/api (npm) | 2.12.4 | |
| axum | 0.8.9 | 1.80 |
| tokio | 1.54.1 | |
| tower / tower-http | 0.5.3 / 0.7.1 | |
| sea-orm | 2.0.4 (uses sqlx 0.9) | 1.94 |
| sqlx | 0.9.0 | 1.94 |
| serde / serde_json | 1.0.229 / 1.0.154 | |
| flate2 | 1.1.11 | |
| rand | 0.10.4 | |
| uuid | 1.29.2 | |
| chrono | 0.4.45 | |
| clap | 4.6.7 | |
| thiserror / anyhow | 2.0.21 / 1.0.104 | |
| rust-embed | 8.14.0 | |
| mime_guess | 2.0.5 | |

Tauri 3 is in alpha (3.0.0-alpha.6); stay on 2.x. Keep the Rust `tauri` crate and
the `@tauri-apps/*` packages on the same minor version.

Other confirmed details:

- **Second window:** `WebviewWindowBuilder::new(app, label, url).build()` from Rust,
  which needs no extra permission. On Windows it deadlocks if called from a
  synchronous command, so the command opening the players' window must be `async`.
  From JS, `new WebviewWindow(...)` needs `core:webview:allow-create-webview-window`.
  Opening it from Rust is simpler.

Still unverified: the custom protocol and bundled SQLite on Android/iOS (not built
for those yet), and `sea-orm-cli generate entity` against our database (not run
yet; M2 does this).

## 10. Changes made during the port

Decided while working through the milestones; each is easy to revisit.

- **sqlx instead of SeaORM.** The typed layer is `schemas.rs`, and records travel as
  JSON, so SeaORM entities would only have converted rows to JSON and back. One
  generic `repo.rs` on sqlx reads each table's declared column types (`JSON`,
  `BOOLEAN`, ...) and needs no per-table code. SQLite is still compiled in.
- **Migrations:** new databases run `migrations/0002_schema.sql`, captured from a
  database the Python app made. Revision 0001 (and pre-Alembic) databases are backed
  up with `VACUUM INTO`, then upgraded by `migrations/0001_to_0002.sql` plus moving
  session encounters in Rust; `sessions.encounter` is removed with `ALTER TABLE ...
  DROP COLUMN` rather than a table rebuild (same columns, keys and indexes; tested).
- **Dice maths uses `f64`.** Results are the same except for whole numbers of 10^15
  and above, which Python prints exactly and Rust prints as `1e+15`.
- **Bugs fixed in both apps** while comparing them: `0^-1` crashed Python (500);
  a handout's stored file name could point outside the handouts folder.

### Deliberate differences from the Python app

- No `/docs` or `/openapi.json` (FastAPI's generated API docs).
- Error *messages* for invalid input are worded differently; status codes match.
- `min()`/`max()` with one argument return it (Python crashed).

## 11. Status (2026-11-16)

| Milestone | Status |
|---|---|
| M0 desktop shell | Done. Loads the UI and API through `questodian://` (checked under WSLg); release `.deb` builds (8.8 MB, depends on `libwebkit2gtk-4.1-0, libgtk-3-0`). Icons are placeholders. |
| M1 no-database endpoints | Done; matches Python. |
| M2 database and records | Done; reviewed by `data-guardian` and its fixes applied. |
| M3 game logic | Done; matches Python. |
| M4 handouts and players' window | Done. The desktop players' window opens via `POST /desktop/players`; not yet clicked through in the desktop app itself. |
| M5 import/export | Done; byte-identical Markdown, and each backend imports the other's backups. |
| M6 CLI | Done: `serve`, `roll`, `calc`, `srd`, `export`, `import`. |
| M7 release | Workflows written (`ci.yml` Rust job with parity, `release.yml` with tauri-action); not yet run on GitHub. Python not retired. |

`scripts/parity.py`: 1254 requests, no differences. `ui-tester` ran the main flows
through the real UI on the Rust server: all working.

### Since the first push (same day)

- Real app icon in every format (`src-tauri/icons/source.svg` → `tauri icon`).
- `questodian.lock`: a database is never upgraded while another copy has it open.
- Rules audit: lair XP (#11), grouped initiative (#12), tiefling size and the build
  script (#16), warlock Pact Magic auto-fill (#3), all in both apps.
- UI: sheet suggestions follow a relinked campaign; AC box fits three digits; phone
  header layout. The Data page counts and phone scrolling issues didn't reproduce.
- Dependabot for Rust, npm and Actions; the release workflow runs every CI check
  first and smoke-tests the Linux build.

### Follow-ups

- Run the Release workflow once; macOS signing and notarisation; Windows SmartScreen.
- Mobile (iOS/Android) builds (needs the Android SDK/NDK and Xcode).
- Rules audit still open: monster XP values that match upstream but may not match
  the official SRD (#16; needs the SRD PDF to check); 2024 Surprise.
- Python's 0002 migration isn't atomic: if it fails halfway, both apps refuse the
  file until it's repaired by hand. Moot once Python is retired.
- Retire the Python app after the first Rust release (see the plan in the chat log:
  move the SRD data and UI build output out of `src/questodian`, then delete it).
