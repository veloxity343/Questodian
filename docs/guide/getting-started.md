# Getting started

[Documentation](../README.md) › Getting started

## Download

Get the installer for your computer from the
[Releases](https://github.com/veloxity343/Questodian/releases/latest) page. You only need one file:

| Your computer | Download |
|---|---|
| Mac with Apple Silicon (M1 or later) | `Questodian_<version>_aarch64.dmg` |
| Mac with an Intel processor | `Questodian_<version>_x64.dmg` |
| Windows | `Questodian_<version>_x64-setup.exe` (or the `.msi`, if you prefer an MSI installer) |
| Linux | `Questodian_<version>_amd64.AppImage` (updates itself), or the `.deb` / `.rpm` for your distribution |

To tell which Mac you have: Apple menu → **About This Mac**. It shows "Chip: Apple M…" or
"Processor: Intel…".

The other files on the release page are for the app's own updates: `.sig` signatures,
`.app.tar.gz` and `latest.json`. You don't need them.

## First launch

The builds aren't code-signed yet, so your system warns you the first time:

- **Windows:** SmartScreen says "Windows protected your PC". Click **More info**, then **Run anyway**.
- **macOS:** move *Questodian.app* to Applications and open it. If macOS won't open it, go to
  **System Settings → Privacy & Security** and click **Open Anyway**. If it says the app is
  "damaged", run this once in Terminal:

  ```bash
  xattr -cr "/Applications/Questodian.app"
  ```

- **Linux (AppImage):** make the file executable (`chmod +x Questodian_*.AppImage`), then run it.

The desktop app then asks whether it may check GitHub for new versions once a day. See
[Updates](updates.md).

## Your first campaign

1. Click the **Questodian** title (top left) to see your campaigns, and create one. Each campaign
   uses either the 2014 rules (SRD 5.1) or the 2024 rules (SRD 5.2.1); you can switch later from
   the campaign overview.
2. Pick the campaign in the **Campaign** menu at the top. The **DM Screen** opens on it: see
   [The DM screen](dm-screen.md).
3. Add the party on the **Characters** page: see [Characters](characters.md).

The top bar also has **Library** (the SRD), **Handbook** (your own documents), **Tools** (dice and
calculators) and **Data** (backups). ⚙ opens **Settings**, and ◐ switches between dark and light.

## Where your data lives

Everything is in one SQLite file, `questodian.db`:

| System | Folder |
|---|---|
| Windows | `%APPDATA%\Questodian` |
| macOS | `~/Library/Application Support/Questodian` |
| Linux | `~/.local/share/questodian` |

Handout images sit next to it in `handouts/`. Before a new version changes the database, it saves a
copy next to it (`questodian.db.backup-…`). See [Data and backups](data-and-backups.md).

To keep the data somewhere else, set `QUESTODIAN_DATA` to a folder, or start the command-line app
with `--data-dir`.

## In a browser instead

On an iPad, or to try it without installing anything, use [the web version](web-version.md).

The same app also runs as a local web server, for any browser. From a copy of the source code (you need
[Rust](https://rustup.rs/) and [Node.js](https://nodejs.org/)):

```bash
make install
```

```bash
make run
```

This opens http://localhost:8765. To let other devices on your network connect, such as a TV
showing handouts, start it with `--host 0.0.0.0` (see [Command line](command-line.md)). There's no
password, so only do this on a network you trust.
