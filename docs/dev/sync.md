# Sync

[Documentation](../README.md) › Sync

Sync goes through a folder in the user's own cloud storage (Google Drive's hidden app folder;
WebDAV next), with no server of ours. The engine is `crates/core/src/sync.rs`; the page's side is
`frontend/src/sync/`.

## The folder

Nothing in the folder is ever changed once written, so no device can overwrite another's work,
whatever the cloud allows (Google Drive has no conditional writes: tried with the Drive test):
- `changes-<device>-<seq>.json`: a device's changes, numbered. Each holds whole records (or a
  deletion), each with a stamp: when the change was made, in milliseconds.
- `image-<sha256>.<ext>`: handout images, named by their content, sent once.

- `backup-<date>-<time>[-before-restore].json.gz`: cloud backups (`frontend/src/sync/backups.ts`):
  the full export, gzipped, with each handout's image as `sync_image` (an image file above)
  instead of `file_b64`. Made once a day when something changed, after a sync's sends; kept for
  the last 14 days that have one and one a week for 8 weeks before. The only files ever removed.
  Restoring re-embeds the images and imports with `mode=replace` (`transfer.rs`), which removes
  what the backup doesn't have; the triggers note it all, so sync carries the restore.

## Which change wins

Per record, the latest stamp, then the device id. A change is always stamped later than the
version it was made to, whatever the clock says, so an edit made after seeing another wins. The
losing version goes to `sync_replaced` for 30 days.

A record replaced or deleted by another device's change is kept in `sync_replaced` too.

Deleting a record takes along what its foreign keys cascade to (a campaign's sessions, say).
Applying another device's deletion keeps those changed after it (here, or in a later version from
another device), unlinked, and sends them on so every device keeps them; a change made here
before the deletion goes, and is kept to restore. Links that are only cleared (`ON DELETE SET
NULL`) are left to each device. A record linking to one that hasn't arrived yet waits: its file is
applied again next sync (`sync_applied` holds the files applied).

When the folder has none of a device's files (its first sync, or a new folder), it sends every
record: those never synced as changes, and the rest again as they are, under the stamp they have
and the device that made them (so devices that have them ignore them), unless that device has
files in the folder.

## Locally (schema 0006)

Triggers on every record table note changes in `sync_outbox`, except while sync itself writes
(the `applying` key in `sync_state`). `sync_stamps` holds the version of each record this device
has. See [Database and migrations](database.md).

## The page's part

The core does no networking: the page holds the cloud's sign-in and moves the files. One sync is:
1. List the folder; `POST /api/sync/wanted` gives the change files to fetch.
2. `POST /api/sync/images-needed` with them; fetch those images and `PUT /api/sync/images/<name>`.
3. `POST /api/sync/apply` with the files.
   Apply files in batches (a request takes up to 20 MB); a record waiting for one in a later
   batch is applied when its file is applied again.
4. `POST /api/sync/outgoing` with the current listing: a file to send and its images. Send the
   images the folder lacks (checking each one's bytes still match its name), then the file, then
   `POST /api/sync/sent`. If an image can't be sent, stop: the change file goes next time. Repeat until it returns
   null. Pass the listing as it is after each send: a listing without any of this device's files
   means a new folder, and everything is queued again.

## Tests

`crates/core/tests/sync.rs` (devices through an in-memory folder: conflicts, deletions,
cascades, images, clocks, files split and interleaved) and `crates/core/tests/api.rs` (the same
over HTTP). `crates/web/smoke/sync.py` runs two browsers through the web version, with Google's
sign-in and Drive stood in for.
