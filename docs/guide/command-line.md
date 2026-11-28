# Command line

[Documentation](../README.md) › Command line

`questodian` is the command-line app: it runs Questodian as a local web server, and has a few tools
of its own. The installers on the release page are for the desktop app; to get the command, build
it from the source code (you need [Rust](https://rustup.rs/) and [Node.js](https://nodejs.org/)):

```bash
make install
```

```bash
cargo install --path crates/cli
```

Or run it from the source folder without installing: `cargo run -p questodian-cli -- roll d20`.

Every command takes `--data-dir <folder>` (or the `QUESTODIAN_DATA` environment variable) to use a
database somewhere other than the usual place.

## serve

Starts the web app and opens it in your browser (this is what runs if you give no command).

The command doesn't contain the UI itself: it serves the built UI from `frontend/dist` in the
folder you run it from. Run it from the source folder after building the UI (`make run` does
both), or point `--web-dir` at a built `frontend/dist`. Without the UI, the page says so, and the
API still works.

| Option | What it does |
|---|---|
| `--port <port>` | Listen on another port (default 8765) |
| `--host 0.0.0.0` | Let other devices on your network connect, such as a TV showing handouts. There's no password: only do this on a network you trust |
| `--no-browser` | Don't open a browser window |
| `--web-dir <folder>` | Serve the built UI from this folder (default `frontend/dist`) |

## roll and calc

```bash
questodian roll 2d20kh1+5 8d6 -v
```

```bash
questodian calc "mod(18) + prof(9)"
```

`roll` takes one or more expressions; `-v` shows each die, and `-n 5` rolls five times. `calc` is a
calculator that understands dice. The syntax is in [Dice](dice.md#dice-syntax).

## srd

```bash
questodian srd fireball
```

```bash
questodian srd -c monsters dragon
```

Searches the SRD 5.1 (2014 rules). `-c` limits it to a category (spells, monsters, rules,
conditions…), `-n` sets how many results (default 20), and `-s` prints the best match in full.

## export and import

```bash
questodian export backup.json
```

```bash
questodian import backup.json house-rules.md --mode skip
```

`export` writes everything, or with `--campaign <id>` one campaign and its linked records; without a
file name it prints to the screen. If any records can't be read, `export` leaves them out and prints
a warning naming each one on stderr (see [Records that can't be read](data-and-backups.md#records-that-cant-be-read));
it still writes the file, but exits with 2 instead of 0 so a backup script can tell it's
incomplete (1 means the export failed). `import` warns the same way about a file that left records
out, and exits with 0 if the import itself worked.
`import` takes JSON exports and Markdown files, in `overwrite`
(the default), `skip` or `copy` mode. See [Data and backups](data-and-backups.md) for what these
mean.
