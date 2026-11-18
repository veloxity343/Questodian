# Database and migrations

[Documentation](../README.md) › Database and migrations

Everything is in one SQLite file, `questodian.db`, in the data folder (see
[Getting started](../guide/getting-started.md#where-your-data-lives)). The code is in
`crates/core/src/db.rs`.

## Schema versions

The schema version is kept in an `alembic_version` table. The first, Python version of Questodian
managed its schema with Alembic, and keeping the same table means every database ever made still
opens. Keep writing the version there.

| Version | What it added |
|---|---|
| (none) | Databases from before migrations existed: treated as `0001` |
| `0002` | The DM screen's tables (scenes, NPCs, notes, handouts, trackers, monsters, encounters) and each campaign's rules edition; sessions' encounters moved into `encounters`. Part of this upgrade is in `db.rs` rather than SQL |
| `0003` | Characters' builder choices (`characters.build`) and the user's rules-data edits (`rule_overrides`) |
| `0004` | Drawn maps on handouts (`handouts.map`) |

When the app opens a database:
- **Older than it knows:** it saves a backup (`questodian.db.backup-<old version>-<time>`, made with
  `VACUUM INTO`, so the WAL is included), then upgrades it in one transaction, with foreign keys off
  so rebuilding a table doesn't cascade-delete.
- **Newer than it knows,** or not a Questodian database: it refuses to open it, and leaves it
  untouched.

Every running copy of the app shares a lock on `questodian.lock` in the data folder. An upgrade
holds it alone, so a second copy waits for the upgrade to finish rather than reading a half-changed
schema. On filesystems without file locks (some network drives), it carries on without the lock.

## Changing the schema

Schema changes always come with a migration and a test that upgrades a database with real data in
it:

1. Write the upgrade as `crates/core/src/migrations/<from>_to_<to>.sql`.
2. In `db.rs`, add it to `STEPS`, add the new version to `KNOWN`, and bump `HEAD`.
3. Make a fixture: with the **current release**, make a database with real records in it (every
   kind the change touches). Dump it to `crates/core/tests/fixtures/v<old version>.sql`, and write
   what each record should look like after upgrading to `v<old version>.expected.json`.
4. Add it to `crates/core/tests/database.rs`. It should upgrade with every record intact, and match
   the schema of a database made new at `HEAD`.
5. If exports change to carry the new data, bump `VERSION` in `transfer.rs`. Older versions then
   refuse the new files instead of importing them without that data.

## Export format

Exports (`transfer.rs`) are JSON bundles with a format `VERSION`:

| Version | What changed |
|---|---|
| 1 | A session holds its encounter |
| 2 | DM screen records, encounters on their own |
| 3 | Characters' builder choices, rules-data edits and drawn maps |

The app imports any version up to its own, and refuses newer ones.
