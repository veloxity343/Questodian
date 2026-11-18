# Questodian

An all-in-one table manager for 5e tabletop games that runs on your own computer:
a DM screen for running sessions, campaign prep, encounters, maps, character sheets with a
character builder, the System Reference Document for both rule sets, your own handbook, and a dice
roller. Fully offline: one local database file, no account, nothing to connect.

- **Rules:** each campaign uses either the 2014 rules (SRD 5.1) or the 2024 rules (SRD 5.2.1)
- **Desktop app:** Windows, macOS and Linux, built with [Tauri 2](https://v2.tauri.app/), and it
  updates itself; it can also run as a local web server for any browser
- **Backend:** Rust: [axum](https://github.com/tokio-rs/axum) for the API, [sqlx](https://github.com/launchbadge/sqlx) on SQLite
- **Frontend:** [React](https://react.dev/) + TypeScript, built with [Vite](https://vite.dev/)

## Install

Download the installer for your computer from the
[latest release](https://github.com/veloxity343/Questodian/releases/latest). You need one file:

| Your computer | Download |
|---|---|
| Mac with Apple Silicon (M1 or later) | `…_aarch64.dmg` |
| Mac with an Intel processor | `…_x64.dmg` |
| Windows | `…_x64-setup.exe` (or the `.msi`) |
| Linux | `…_amd64.AppImage`, or the `.deb` / `.rpm` |

The `.sig`, `.app.tar.gz` and `latest.json` files are for the app's own updates. The builds aren't
code-signed yet, so the first launch needs one extra step: see
[Getting started](docs/guide/getting-started.md#first-launch).

To run it from the source code instead, see [Development](docs/dev/development.md).

## Documentation

The full documentation is in [docs/](docs/README.md).

**Using Questodian**
- [Getting started](docs/guide/getting-started.md): downloading, the first launch, where your data
  lives, running it in a browser
- [The DM screen](docs/guide/dm-screen.md): the navigator, tabs, encounters, sessions, linking
  notes, Ctrl K search
- [Maps and handouts](docs/guide/maps-and-handouts.md): drawing maps, showing them to players,
  exporting
- [Characters](docs/guide/characters.md): the character builder, sheets filled in by hand, rules
  data
- [Dice](docs/guide/dice.md): the quick roll, the full roller, dice syntax
- [Library, Handbook, Tools and Settings](docs/guide/other-pages.md), including arranging any page
- [Data and backups](docs/guide/data-and-backups.md) · [Updates](docs/guide/updates.md) ·
  [Command line](docs/guide/command-line.md) ·
  [Troubleshooting and known issues](docs/guide/troubleshooting.md)

**Working on Questodian**
- [Development](docs/dev/development.md): setting up, `make` tasks, the code's layout, tests, CI
- [Database and migrations](docs/dev/database.md) · [Releasing](docs/dev/releasing.md) ·
  [Bundled data](docs/dev/data.md)

## Quick start for developers

```sh
make install     # npm install (Rust crates download on the first build)
make dev         # API on :8765 + UI on http://localhost:5173, hot reload
make check       # Rust format check, lint and tests; the frontend typecheck and tests
make app         # the desktop app (debug build)
```

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

The character builder's rules data also has 16 backgrounds and 6 origin feats that aren't in
the SRD. They're tagged "not SRD" on the Rules data page, and the licence above doesn't cover them.
