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

## Which change wins

Per record, the latest stamp, then the device id. A change is always stamped later than the
version it was made to, whatever the clock says, so an edit made after seeing another wins. The
losing version goes to `sync_replaced` for 30 days.

Deleting a record that others link to (a campaign) keeps the records in it that were changed after
the deletion, unlinked, and sends them on. A record linking to one that hasn't arrived yet waits:
its file is applied again next sync (`sync_applied` holds the files applied).

## Locally (schema 0006)

Triggers on every record table note changes in `sync_outbox`, except while sync itself writes
(the `applying` key in `sync_state`). `sync_stamps` holds the version of each record this device
has. See [Database and migrations](database.md).

## The page's part

The core does no networking: the page holds the cloud's sign-in and moves the files. One sync is:
1. List the folder; `POST /api/sync/wanted` gives the change files to fetch.
2. `POST /api/sync/images-needed` with them; fetch those images and `PUT /api/sync/images/<name>`.
3. `POST /api/sync/apply` with the files.
4. `POST /api/sync/outgoing` with the current listing: a file to send and its images. Send the
   images the folder lacks, then the file, then `POST /api/sync/sent`. Repeat until it returns
   null. Pass the listing as it is after each send: a listing without any of this device's files
   means a new folder, and everything is queued again.

## Tests

`crates/core/tests/sync.rs` (devices through an in-memory folder: conflicts, deletions,
cascades, images, clocks, files split and interleaved) and `crates/core/tests/api.rs` (the same
over HTTP). `crates/web/smoke/sync.py` runs two browsers through the web version, with Google's
sign-in and Drive stood in for.
