# Questodian

An all-in-one Dungeons & Dragons table manager that runs on your own computer:
a DM screen for running sessions, campaign prep, encounters, character sheets,
the System Reference Document for both rule sets, your own handbook, and a dice
roller. Fully offline: one local database file, no account, nothing to connect.

- **Rules:** each campaign uses either the 2014 rules (SRD 5.1) or the 2024 rules (SRD 5.2.1)
- **Desktop app:** Windows and macOS builds open in their own window; anywhere else it runs in the browser
- **Backend:** Python, [FastAPI](https://fastapi.tiangolo.com/), [SQLAlchemy](https://www.sqlalchemy.org/) on SQLite, [Alembic](https://alembic.sqlalchemy.org/) migrations
- **Frontend:** [React](https://react.dev/) + TypeScript, built with [Vite](https://vite.dev/)

## Install

**Desktop app (Windows, macOS):** download the zip for your system from the
[Releases](https://github.com/veloxity343/questodian/releases) page, unzip it and run
*Questodian*. The builds aren't code-signed yet, so the first launch needs one extra step:

- **Windows:** SmartScreen says "Windows protected your PC". Click **More info**, then **Run anyway**.
- **macOS:** after moving *Questodian.app* to Applications, run `xattr -cr "/Applications/Questodian.app"` once in Terminal.

**From source (any OS):**

```sh
make install     # uv sync + npm install
make run         # build the UI and open it in your browser at http://localhost:8765
make app         # or: open it in its own window (Windows, macOS; Linux needs GTK or Qt)
```

Other options:

```sh
uv run questodian serve --port 9000 --no-browser
uv run questodian serve --host 0.0.0.0     # let devices on your network connect (e.g. a TV for handouts)
uv run questodian --data-dir ./my-table    # store the database somewhere specific
```

Your data is one SQLite file, `questodian.db`, in the usual place for your OS
(`%APPDATA%\Questodian` on Windows, `~/Library/Application Support/Questodian` on macOS,
`~/.local/share/questodian` on Linux), or `$QUESTODIAN_DATA` / `--data-dir`. Handout
images sit next to it in `handouts/`. Before a new version changes the database, a
copy is saved next to it (`questodian.db.backup-…`).

## The DM screen

Three columns, built for running a session:

- **Left, the navigator:** sessions, scenes (grouped by status), NPCs, notes, handouts (by folder),
  homebrew monsters, trackers and the party; and a separate list of encounters.
- **Middle, the workspace:** everything opens as a tab, and reads and edits in the same layout.
  - **Scenes:** summary, boxed *read-aloud* text, DM notes, and a status (planned, active, completed, skipped).
  - **NPCs:** looks, voice, wants, secret, what they know, and a linked stat block.
  - **Encounters:** party size × level with the 2024 difficulty (low / moderate / high), initiative,
    a "now" banner, HP pills (damage, heal, temp), conditions, hidden combatants, Roll HP.
  - **Sessions:** their scenes in order, trackers (clocks like "tolls of the bell"), encounters, prep,
    recap, a timestamped log, loot that goes straight into inventories, and XP.
  - **Handouts:** images with zoom and pan; **Show to players** opens a second window (or any browser on
    your network at `/project`) that follows what you show.
  - Homebrew stat blocks, notes (general, session, quest, pin board) and the campaign overview.
- **Right, the reference panel:** the rules for your campaign's edition (monsters, spells, items, rules
  and more, plus your homebrew), a **View** tab for whatever you clicked, quick notes, and a dice tray
  with history. Attacks and damage in any stat block or note roll when clicked: `+6`, `2d8+3`.

Type `[[Ada Brae]]` in any notes to link to a record (or `[[spell:Fireball]]`, `[[monster:Zombie]]` for
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
uv run questodian roll 2d20kh1+5 8d6 -v
uv run questodian calc "mod(18) + prof(9)"
uv run questodian srd fireball
uv run questodian srd -c monsters dragon
uv run questodian export backup.json [--campaign ID]
uv run questodian import backup.json house-rules.md --mode skip
```

## Development

Common tasks are in the `Makefile` (run `make` to list them):

```sh
make install     # uv sync + npm install (first time)
make dev         # API on :8765 + UI on http://localhost:5173, hot reload
make check       # backend tests + frontend typecheck
make migration m="describe the change"   # after editing models.py
make package     # build the UI, then the wheel and sdist into dist/
```

`make dev` runs both servers with `concurrently`: one Ctrl+C stops both, and if
either exits the other is stopped too. `make api` / `make web` run them on their
own. Interactive API docs are at http://localhost:8765/docs.

### Building a release

The Python package ships the compiled UI, so the frontend is built before the
wheel. `make package` does both and fails if the UI didn't make it into the
wheel. By hand:

```sh
cd frontend && npm run build && cd ..   # writes src/questodian/web/
uv build                                # dist/questodian-*.whl and .tar.gz
```

### Desktop builds

`make desktop` builds the app for the OS you're on into `dist-desktop/`. The **Desktop app**
workflow builds Windows and macOS on GitHub: run it from the Actions tab, or push a version tag
(`git tag v0.2.0 && git push origin v0.2.0`) to also publish a Release with both zips.

### Continuous integration

GitHub Actions (`.github/workflows/ci.yml`) runs on every push to `main` and on
pull requests: backend tests, frontend typecheck, then `make package`, which
installs the wheel into a clean environment, starts it, and checks the UI, SRD
search and saving all work. The built wheel and sdist are attached to each run
as a `dist` artifact.

### Layout

- `src/questodian/app.py`: FastAPI app, routes and static UI serving
- `src/questodian/desktop.py`: the desktop window (pywebview); `packaging/` has the PyInstaller recipe
- `src/questodian/models.py`: SQLAlchemy tables (campaigns, sessions, characters, handbook)
- `src/questodian/schemas.py`: Pydantic schemas for API payloads and exports
- `src/questodian/db.py`, `repo.py`: engine/session setup and record access
- `src/questodian/migrations/`, `migrate.py`: Alembic migrations, applied on startup
- `src/questodian/game.py`: initiative tracker, rewards and rests
- `src/questodian/dice.py`: dice and arithmetic parser/evaluator (no `eval`)
- `src/questodian/rules.py`: 5e numbers and derived character stats
- `src/questodian/transfer.py`: import and export (JSON bundles, Markdown)
- `frontend/src/`: the React UI (`dm/` is the DM screen, `views/` the other pages, `context/` app-wide state)
- `scripts/build_srd.py`, `scripts/build_srd_2024.py`: regenerate the bundled SRD 5.1 and 5.2.1 data
  from [5e-bits/5e-database](https://github.com/5e-bits/5e-database) (the 2024 rules prose comes from
  a Markdown copy of the SRD 5.2.1)

Nested sheet data (inventory, spells, the encounter, etc.) is stored in JSON
columns; links between records are real foreign keys (deleting a campaign deletes
its sessions and unlinks characters and handbook entries).

### Database migrations

The schema is managed with [Alembic](https://alembic.sqlalchemy.org/). The app
upgrades the database to the latest migration every time it starts, so users
never run anything by hand. After changing `models.py`:

```sh
uv run alembic revision --autogenerate -m "add character portrait"
# review the new file in src/questodian/migrations/versions/, then:
uv run pytest                  # fails if models and migrations disagree
```

The `alembic` command targets your normal database (`$QUESTODIAN_DATA` or
`~/.local/share/questodian/questodian.db`); add `-x db=path/to/file.db` to use
another. Other useful commands: `alembic current`, `alembic history`,
`alembic check`, `alembic downgrade -1`.

SQLite can't alter columns in place, so migrations run in Alembic's batch mode
(tables are rebuilt), with foreign keys switched off meanwhile so rebuilding
`campaigns` doesn't cascade-delete sessions.

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
