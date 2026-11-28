# Data and backups

[Documentation](../README.md) › Data and backups

Everything you make is in one file, `questodian.db`, with handout images next to it in
`handouts/`. Where that is: see [Getting started](getting-started.md#where-your-data-lives).

## Exporting

On the **Data** page:

- **⬇ Full backup (all data):** everything, as one `.questodian.json` file. It includes your
  changes to the character builder's rules data.
- **⬇ Export campaign:** one campaign with everything linked to it, handout images included.

Single characters, sessions and handbook entries export as JSON or Markdown from their own pages.

## Importing

The **Data** page imports:
- Questodian backups and campaign exports;
- single-record exports (`.json`);
- Markdown files (`.md`), which become handbook entries.

Choose how records already in your data are treated:

| Mode | Use it for |
|---|---|
| **Overwrite** | Restoring a backup: records with the same id are replaced |
| **Skip** | Adding what's new and keeping your existing records |
| **Copy** | Sharing between DMs: everything comes in as new records |

With sync on, backups are also kept in your Google Drive and restored from the Sync panel (see
[Sync](sync.md#backups-in-google-drive)).

Each import happens all at once: if a file has a problem, nothing changes. Rules-data edits that
no longer fit the builder are skipped, and the import says how many.

A file exported by a newer version of Questodian can't be imported into an older one: update the
older copy first. For example, 0.1.x can't read 0.2.0 exports.

## Automatic backups

When a new version of Questodian needs to change the database, it first saves a copy next to it,
named `questodian.db.backup-<old version>-<date and time>`. Imports and everyday changes don't make
one, so use **Full backup** before anything you might want to undo.

To go back to one of these copies:
1. Quit Questodian.
2. Move `questodian.db` somewhere safe, along with `questodian.db-wal` and `questodian.db-shm` if
   they're there.
3. Copy the backup file into the folder as `questodian.db`.

Opening an older database in the newer version upgrades it again.

## Moving to another computer

Either export a **Full backup** and import it on the other computer in **Overwrite** mode, or quit
Questodian and copy the whole data folder (`questodian.db` and `handouts/`) across.

Databases, backups and exports from the first, Python version of Questodian open unchanged.
