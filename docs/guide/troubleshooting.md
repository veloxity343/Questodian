# Troubleshooting and known issues

[Documentation](../README.md) › Troubleshooting and known issues

## The app won't open the first time

The builds aren't code-signed yet. See
[Getting started: First launch](getting-started.md#first-launch) for the one-time step on Windows,
macOS and Linux.

## "It was made by a newer version of Questodian"

A newer version upgraded your database, and the version you're running is older, so it leaves the
database untouched. Use the newer version. To go back to the older version for good, restore the
backup the upgrade saved: see [Data and backups](data-and-backups.md#automatic-backups).

## An export won't import

Files exported by a newer version can't be imported into an older one. Update the copy you're
importing into first.

## The browser shows "The UI hasn't been built yet"

`questodian serve` didn't find the built UI. Run `make run` from the source folder, or see
[Command line](command-line.md#serve).

## Two copies of Questodian at once

Two copies can use the same data folder; they share it safely. If one needs to upgrade the
database, the other waits until it's done.

## An update didn't install

Nothing was changed. Try again from **Settings**, or download the new version from the release
page. On Linux, only the AppImage is known to update itself: see [Updates](updates.md).

## Known issues

- **Some numbers in the bundled SRD 5.2.1 (2024) class tables are wrong.** They came from a
  conversion error, and show in the Library and the reference panel. Confirmed so far:
  - the Wizard's **Prepared Spells** for levels 14–20 (shown 17, 18, 18, 19, 20, 21, 22; the SRD
    says 18, 19, 21, 22, 23, 24, 25);
  - the Druid's **Cantrips** at level 16 (shown 5; the SRD says 4).

  A review also flagged some weapon entries; they haven't been rechecked. The character builder
  works from its own rules data, not these tables, so built characters aren't affected.
- **Touchpad pinch-to-zoom** on maps and handout images hasn't been tested on every system. Two-finger
  scrolling zooms.
- **The ⠿ drag in Arrange** has been tested in a browser but not yet in every desktop window. The
  **↑ ↓** buttons move panels too.

Found something else? Open an issue on
[GitHub](https://github.com/veloxity343/Questodian/issues).
