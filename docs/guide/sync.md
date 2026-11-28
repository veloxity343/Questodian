# Sync

[Documentation](../README.md) › Sync

Sync keeps your devices up to date with each other through your own Google Drive. There's no
Questodian account and no Questodian server: each device reads and writes a folder in your Drive.
For now it's in the [web version](web-version.md) (the desktop app comes next).

## Connecting

On the **Data** page, **Sync** → **Connect Google Drive**, and sign in to Google. Google asks
whether Questodian may use its own folder in your Drive: allow it. Questodian can see only that
folder, which is hidden (it doesn't appear in your Drive's files), and nothing else in your Drive.

Do the same on each device, with the same Google account. The first sync sends everything on that
device; the others then get it.

## When it syncs

When the app opens, a few seconds after you change something, every few minutes, and when you
switch away from it. **Sync now** syncs straight away.

Without internet everything works as before: changes wait on the device and go when it's back
online.

**Every hour**, Google asks the web version to sign in again. Sync then shows **Paused**: tap
**Carry on syncing**. A Google window opens and closes by itself; nothing to click. Changes made
meanwhile wait, and nothing is lost.

## When two devices change the same thing

If the same record (a campaign, an NPC, a note…) was changed on two devices before they synced,
the later change wins on every device. The other version isn't lost: it's kept for 30 days (the
Sync panel says how many), and so is anything deleted on another device. An edit made after a
record was deleted on another device brings it back; deleting a campaign on one device while you
edit one of its sessions on another keeps that session, out of the campaign.

A page you have open (a character sheet, the DM screen) shows the other device's changes when
they arrive, unless you're in the middle of changing it: then your change is the later one.

## Backups in Google Drive

With sync connected, a full backup is saved to the same hidden folder once a day when something
changed (and the first time you connect). The last 14 days' backups are kept, and one a week for
the 8 weeks before; older ones are removed. **Back up now** saves one straight away.

**Restore** (next to each backup in the Sync panel) makes that device's data exactly what the
backup holds: records made since are removed, changed ones go back. Sync then takes it to your
other devices. Just before restoring, what's there is backed up, so a restore can be undone by
restoring that one ("before a restore").

Handout images are kept once in the folder and shared by every backup, so a day's backup is
small unless something big changed.

## Disconnecting

**Disconnect** stops syncing on that device. Everything on it stays; so does the folder in your
Drive, for your other devices.

A deletion syncs to every device, like any change: to undo one, restore a backup from before it.
The Data page's **⬇ Full backup** still works as before, for a copy outside Google Drive.
