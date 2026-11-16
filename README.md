# Questodian

An all-in-one table manager for 5e tabletop games that runs on your own computer:
a DM screen for running sessions, campaign prep, encounters, character sheets,
the System Reference Document for both rule sets, your own handbook, and a dice
roller. Fully offline: one local database file, no account, nothing to connect.

- **Rules:** each campaign uses either the 2014 rules (SRD 5.1) or the 2024 rules (SRD 5.2.1)
- **Desktop app:** Windows, macOS and Linux, built with [Tauri 2](https://v2.tauri.app/); it can also run as a local web server for any browser
- **Backend:** Rust: [axum](https://github.com/tokio-rs/axum) for the API, [sqlx](https://github.com/launchbadge/sqlx) on SQLite
- **Frontend:** [React](https://react.dev/) + TypeScript, built with [Vite](https://vite.dev/)

## Install

**Desktop app (Windows, macOS, Linux):** download the installer for your system from the
[Releases](https://github.com/veloxity343/Questodian/releases) page (Windows `.msi` or `.exe`,
macOS `.dmg`, Linux `.deb`, `.rpm` or `.AppImage`) and install it. The builds aren't code-signed
yet, so the first launch needs one extra step:

- **Windows:** SmartScreen says "Windows protected your PC". Click **More info**, then **Run anyway**.
- **macOS:** move *Questodian.app* to Applications and open it. If macOS won't open it, go to
  **System Settings → Privacy & Security** and click **Open Anyway**. If it says the app is
  "damaged", run `xattr -cr "/Applications/Questodian.app"` once in Terminal.

**From source (any OS):** you need [Rust](https://rustup.rs/) and [Node.js](https://nodejs.org/);
on Linux, also the WebKitGTK packages [Tauri lists](https://v2.tauri.app/start/prerequisites/)
for the desktop app.

```sh
make install     # npm install (Rust crates download on the first build)
make run         # build the UI and open it in your browser at http://localhost:8765
make app         # or: open it in its own window
```

`make run` is `questodian serve`, the command-line app. Other options:

```sh
cargo run -p questodian-cli -- serve --port 9000 --no-browser
cargo run -p questodian-cli -- serve --host 0.0.0.0   # let devices on your network connect (e.g. a TV for handouts)
cargo run -p questodian-cli -- --data-dir ./my-table  # store the database somewhere specific
cargo install --path crates/cli                       # install `questodian` on your PATH
```

Your data is one SQLite file, `questodian.db`, in the usual place for your OS
(`%APPDATA%\Questodian` on Windows, `~/Library/Application Support/Questodian` on macOS,
`~/.local/share/questodian` on Linux), or `$QUESTODIAN_DATA` / `--data-dir`. Handout
images sit next to it in `handouts/`. Before a new version changes the database, a
copy is saved next to it (`questodian.db.backup-…`). Databases and backups from the
first, Python version of Questodian open unchanged.

## The DM screen

Three columns, built for running a session:

- **Left, the navigator:** sessions, scenes (grouped by status), NPCs, notes, handouts (by folder),
  homebrew monsters, trackers and the party; and a separate list of encounters.
- **Middle, the workspace:** everything opens as a tab, and reads and edits in the same layout.
  - **Scenes:** summary, boxed *read-aloud* text, DM notes, and a status (planned, active, completed, skipped).
  - **NPCs:** looks, voice, wants, secret, what they know, and a linked stat block.
  - **Encounters:** party size × level with the 2024 difficulty (low / moderate / high), initiative,
    a "now" banner, HP pills (damage, heal, temp), conditions, hidden combatants, Roll HP.
  - **Sessions:** their scenes in order, trackers (clocks like "hours until the ritual"), encounters, prep,
    recap, a timestamped log, loot that goes straight into inventories, and XP.
  - **Handouts:** images with zoom and pan; **Show to players** opens a second window (or any browser on
    your network at `/project`) that follows what you show.
  - Homebrew stat blocks, notes (general, session, quest, pin board) and the campaign overview.
- **Right, the reference panel:** the rules for your campaign's edition (monsters, spells, items, rules
  and more, plus your homebrew), a **View** tab for whatever you clicked, quick notes, and a dice tray
  with history. Attacks and damage in any stat block or note roll when clicked: `+6`, `2d8+3`.

Type `[[Captain Vane]]` in any notes to link to a record (or `[[spell:Fireball]]`, `[[monster:Zombie]]` for
the rules); typing `[[` suggests names. **Ctrl K** searches everything: records, the rules, and quick actions.

## Everything else

| Area | What it does |
|---|---|
| **Characters** | Full sheet: multiclassing, abilities and saves, skills (proficiency, expertise, half, Jack of All Trades), HP, hit dice, death saves, conditions, exhaustion, attacks with to-hit/damage/crit rolls, spellcasting (DC, attack, slots from class levels, spells from the SRD), class resources, inventory with carrying weight, coins, features, personality. Short and long rests. Click any number to roll it. |
| **Library** | The whole SRD for the campaign's edition: SRD 5.1 (319 spells, 334 monsters) or SRD 5.2.1 (339 spells, 341 monsters, species, weapon mastery…), with filters. |
| **Handbook** | Your own Markdown documents (house rules, lore, random tables…), with templates and live preview. |
| **Tools** | Dice pool builder, macros, roll history, a calculator that understands dice, and quick calculators (ability modifiers, point buy, encounter XP, XP to level, coin splitting, spell DC). Press **R** for the quick roller. |
| **Data** | A full backup, or one campaign with everything linked to it (handout images included), as JSON. Single records export as JSON or Markdown. Imports accept these and `.md` files in *overwrite*, *skip* or *copy* mode, in one transaction: a bad file changes nothing. |

### Dice syntax

```
d20  3d6+2  2d20kh1 (advantage)  2d20kl1 (disadvantage)  adv+5  dis
4d6dl1 (drop lowest)  3d6! (exploding)  2d6ro2 (reroll ≤2 once)  2d6r1
d%  4dF  (1d8+3)*2  floor(8d6/2)  mod(16)  prof(5)
```

## Command line

```sh
questodian roll 2d20kh1+5 8d6 -v
questodian calc "mod(18) + prof(9)"
questodian srd fireball
questodian srd -c monsters dragon
questodian export backup.json [--campaign ID]
questodian import backup.json house-rules.md --mode skip
```

(From a checkout without installing it: `cargo run -p questodian-cli -- roll d20`.)

## Development

Common tasks are in the `Makefile` (run `make` to list them):

```sh
make install     # npm install (first time)
make dev         # API on :8765 + UI on http://localhost:5173, hot reload
make check       # Rust format check, lint and tests, and the frontend typecheck
make app         # the desktop app (debug build)
make desktop     # the desktop app and its installers for this OS, into target/release/bundle/
```

`make dev` runs both servers with `concurrently`: one Ctrl+C stops both, and if
either exits the other is stopped too. `make api` / `make web` run them on their own.

### Releases

Releases come from the **Release** workflow: push a version tag that matches the version in
`src-tauri/tauri.conf.json` (`git tag v0.2.0 && git push origin v0.2.0`), or run it from the
Actions tab. It runs every CI check, creates a draft Release, builds Windows, macOS (Apple
Silicon and Intel) and Linux on GitHub, and attaches the installers to the draft for you to
publish.

### Continuous integration

GitHub Actions (`.github/workflows/ci.yml`) runs on every push to `main` and on pull requests:
the Rust format check, lint and tests; the frontend typecheck; then a release build of the
`questodian` command, which is started to check the UI, SRD search and saving work. The binary
is attached to each run.

### Layout

- `crates/core/`: everything but the windows, shared by the desktop app and the command line
  - `server.rs`: the HTTP API (axum) and serving the built UI
  - `db.rs`, `migrations/`: opening the database, backups and schema upgrades
  - `repo.rs`, `kinds.rs`: reading and writing records, the same way for every kind
  - `schemas.rs`: every record and request body, with defaults and lenient input
  - `game.rs`: initiative tracker, rewards and rests
  - `rules.rs`: 5e numbers and derived character stats, per edition
  - `dice.rs`: dice and arithmetic parser/evaluator
  - `srd.rs`: the bundled SRD, loading and search
  - `transfer.rs`: import and export (JSON bundles, Markdown)
- `crates/cli/`: the `questodian` command (`serve`, `roll`, `calc`, `srd`, `export`, `import`)
- `src-tauri/`: the desktop app; it serves the UI and API to its window through its own
  `questodian://` address, with nothing listening on the network
- `frontend/src/`: the React UI (`dm/` is the DM screen, `views/` the other pages, `context/` app-wide state)
- `data/`: the bundled SRD 5.1 and 5.2.1, regenerated by `scripts/build_srd.py` and
  `scripts/build_srd_2024.py` from [5e-bits/5e-database](https://github.com/5e-bits/5e-database)
  (the 2024 rules prose comes from a Markdown copy of the SRD 5.2.1)

Nested sheet data (inventory, spells, the encounter, etc.) is stored in JSON
columns; links between records are real foreign keys (deleting a campaign deletes
its sessions and unlinks characters and handbook entries).

### Database migrations

The app upgrades the database when it opens it, so users never run anything by hand. The
schema version is kept in an `alembic_version` table (the first, Python version managed its
schema with Alembic), so every database ever made still opens. To change the schema:

1. Add the upgrade to `crates/core/src/db.rs` and bump `HEAD`. It runs in one transaction,
   after a backup, with foreign keys off so rebuilding a table doesn't cascade-delete.
2. Add a fixture database at the old version to `crates/core/tests/fixtures/` (a SQL dump made
   by the current release) and check in `crates/core/tests/database.rs` that it upgrades with
   every record intact.

## License and attribution

Questodian is released under the [MIT License](LICENSE).

This work includes material from the System Reference Document 5.2.1 ("SRD 5.2.1") by
Wizards of the Coast LLC, available at https://www.dndbeyond.com/srd. The SRD 5.2.1 is
licensed under the Creative Commons Attribution 4.0 International License, available at
https://creativecommons.org/licenses/by/4.0/legalcode.

This work includes material taken from the System Reference Document 5.1
("SRD 5.1") by Wizards of the Coast LLC and available at
https://dnd.wizards.com/resources/systems-reference-document. The SRD 5.1 is
licensed under the Creative Commons Attribution 4.0 International License
available at https://creativecommons.org/licenses/by/4.0/legalcode.

The SRD data was converted from [5e-bits/5e-database](https://github.com/5e-bits/5e-database) (MIT).
