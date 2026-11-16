# Questodian

A D&D campaign manager and DM screen: campaigns, sessions, scenes, NPCs, encounters, character sheets and the SRD rules reference, for both the 2014 rules (SRD 5.1) and the 2024 rules (SRD 5.2.1). It runs locally and offline, as a desktop app or in the browser.

## Stack

- **Now:** React + TypeScript UI (`frontend/`, Vite), Python backend (`src/questodian/`: FastAPI, SQLAlchemy on SQLite, Alembic migrations), desktop build with pywebview + PyInstaller (`packaging/`).
- **Planned:** Tauri 2 with the backend rewritten in Rust (axum, SeaORM on SQLite, same schema so existing databases open unchanged). The React UI stays. Keep an optional built-in web server (for the players' handout screen and browser use). Desktop first, then mobile (iOS/Android) via Tauri. The Python version keeps working until the Rust one reaches parity.

## Commands

`make` lists everything. The main ones:

- `make install`: Python and frontend dependencies.
- `make dev`: API and UI dev servers with hot reload.
- `make check`: backend tests and frontend typecheck. Run this before committing.
- `make run` builds the UI and serves the app; `make app` opens it in a window.
- `make migration m="..."` after changing models.
- `make package` builds the wheel; `make desktop` builds the desktop app.

## Rules

### Ask first

- **Never run `sudo` (or doas/pkexec/su) or push to a remote without the user's explicit approval for that specific action.** That includes `git push` in any form and GitHub CLI commands that change GitHub (`gh pr create/merge`, releases, `gh api` …). A hook in `.claude/settings.json` enforces this. Don't try to get around it. Local commits are fine.
- Never modify the user's real data folder (`~/.local/share/questodian`, `%APPDATA%\Questodian`, `~/Library/Application Support/Questodian`) without asking, and back it up first. Test on scratch copies.
- Don't move or rewrite files under an app that's running. Check first.

### No hallucinations

- Don't state anything as fact that you haven't verified in this session: read the code, run the command, check the data, or fetch the official docs. If you can't verify something, say "unverified" and what you'd need. "I don't know" is a fine answer; a confident guess is not.
- Never invent APIs, config options, crate/package versions, file paths or command flags. Check them (use `docs-checker` for external libraries).
- Never say something works, passes or builds unless you ran it and saw the result. Quote the real output. Report failures as failures.
- D&D rules come only from the bundled SRD data (`src/questodian/data/srd*.json.gz`), for the right edition, never from memory. Use `rules-lawyer` for any rule, number or stat block. If the SRD doesn't cover it, say so. The app ships only SRD content (CC BY 4.0) plus the user's own homebrew. No Wizards of the Coast product identity.

### Project conventions

- The app is called Questodian, and has always been. Don't add notes or compatibility code for any earlier name.
- Schema changes always come with a migration and a test that upgrades a database containing real data.
- Match the surrounding code's style and comment density. Keep user-facing text plain and specific.

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
