# Questodian

A D&D campaign manager and DM screen: campaigns, sessions, scenes, NPCs, encounters, character sheets and the SRD rules reference, for both the 2014 rules (SRD 5.1) and the 2024 rules (SRD 5.2.1). It runs locally and offline, as a desktop app or in the browser.

## Stack

React + TypeScript UI (`frontend/`, Vite) on a Rust backend, in a Cargo workspace:

- `crates/core`: dice, rules, the SRD, schemas, the SQLite repo (rusqlite), migrations, game logic, import/export and the axum router.
- `crates/cli`: the `questodian` command (`serve`, `roll`, `calc`, `srd`, `export`, `import`).
- `crates/web`: the core in WebAssembly for the browser version (`make wasm`), answering the API in a Web Worker with SQLite in browser storage (OPFS).
- `src-tauri`: the Tauri 2 desktop app, serving the router to its window through its `questodian://` scheme.

The first version of Questodian was Python (FastAPI, SQLAlchemy, Alembic); it was retired after the Rust port reached parity. Its databases, backups and exports must keep working: the schema version stays in `alembic_version`, and the lenient input rules in `schemas.rs` match what it accepted. Background and decisions: `docs/rust-port.md`. Next: an iPad version as an offline web app (PWA) on the `pwa` branch, with `ipad` (Tauri mobile) kept as the alternative.

## Commands

`make` lists everything. The main ones:

- `make install`: frontend and Tauri CLI dependencies (Rust crates download on the first build).
- `make dev`: API and UI dev servers with hot reload.
- `make check`: Rust format check, lint and tests; the frontend typecheck and tests. Run this before committing.
- `make run` builds the UI and serves the app in the browser; `make app` opens the desktop app.
- `make desktop` builds the desktop app and its installers for this OS.
- Releases: bump the version (on a branch: `main` takes PRs only), merge, then push a `v*` tag matching `src-tauri/tauri.conf.json`. Steps and update signing: `docs/dev/releasing.md`.

## Rules

### Ask first

- **Never run `sudo` (or doas/pkexec/su) or push to a remote without the user's explicit approval for that specific action.** That includes `git push` in any form and GitHub CLI commands that change GitHub (`gh pr create/merge`, releases, `gh api` …). A hook in `.claude/settings.json` enforces this. Don't try to get around it. Local commits are fine.
- Never modify the user's real data folder (`~/.local/share/questodian`, `%APPDATA%\Questodian`, `~/Library/Application Support/Questodian`) without asking, and back it up first. Test on scratch copies.
- Don't move or rewrite files under an app that's running. Check first.

### No hallucinations

- Don't state anything as fact that you haven't verified in this session: read the code, run the command, check the data, or fetch the official docs. If you can't verify something, say "unverified" and what you'd need. "I don't know" is a fine answer; a confident guess is not.
- Never invent APIs, config options, crate/package versions, file paths or command flags. Check them (use `docs-checker` for external libraries).
- Never say something works, passes or builds unless you ran it and saw the result. Quote the real output. Report failures as failures.
- D&D rules come only from the bundled SRD data (`data/srd*.json.gz`), for the right edition, never from memory. Use `rules-lawyer` for any rule, number or stat block. If the SRD doesn't cover it, say so. The app ships SRD content (CC BY 4.0) and the user's own homebrew. The one exception, by the user's choice: the character builder's rules data (`data/rules-2024.json.gz`) includes some 2024 backgrounds and origin feats from outside the SRD, carried over from their own character sheet. They're marked `"srd": false` in the data and tagged "not SRD" on the Rules data page (not in the builder's lists). Don't add other non-SRD content without asking. No Wizards of the Coast product identity.

### Project conventions

- The app is called Questodian, and has always been. Don't add notes or compatibility code for any earlier name.
- Schema changes always come with a migration and a test that upgrades a database containing real data: add the upgrade to `crates/core/src/db.rs` (bump `HEAD`, keep writing `alembic_version`), and add a fixture database at the old version to `crates/core/tests/fixtures/` (see `docs/dev/database.md`).
- Match the surrounding code's style and comment density. Keep user-facing text plain and specific.
- The documentation is in `docs/` (`docs/README.md` is its contents page): `guide/` for people using the app, `dev/` for working on it. Update the page a change affects in the same PR.

## Agents (`.claude/agents/`)

Use these to check work independently before reporting it as done:

| Agent | Use it when |
|---|---|
| `rules-lawyer` | Any D&D rule, number, stat block or encounter maths the code or content depends on |
| `code-reviewer` | Before committing a non-trivial change |
| `verifier` | Before saying anything works, and before every commit |
| `ui-tester` | After any UI change, to see it working in a real browser |
| `data-guardian` | Any change to the schema, migrations, data folder, backups or import/export |
| `docs-checker` | Before relying on a library API, version or platform behaviour you haven't confirmed |
