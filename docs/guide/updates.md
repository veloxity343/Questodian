# Updates

[Documentation](../README.md) › Updates

## Checking for new versions

The desktop app can check GitHub for a new version once a day.

- **First launch:** it asks whether to check. You can change your answer any time in **Settings**
  (⚙): **Check for updates once a day**, or **Check now** to check straight away.
- **How often:** once a day while it's on, or 3 hours after a check that failed (for example, when
  you were offline).
- **What's sent:** one request to GitHub's "latest release" address. Nothing about you or your
  data is sent.
- **When there's one:** a new version gets one pop-up. After that, Settings keeps offering it until
  you update.

In a browser (`questodian serve`) there's no update check: get new versions from the
[Releases](https://github.com/veloxity343/Questodian/releases) page.

## Installing an update

From version 0.2.1, **Update** does it all:
1. downloads the new version;
2. checks it was signed with Questodian's key, and refuses it if not;
3. installs it, and restarts the app.

Your data isn't touched by the update. A new version that changes the database first saves a
backup of it: see [Data and backups](data-and-backups.md).

- **0.2.0 and earlier** can't install updates. **Download** opens the release page; install the
  new version by hand, once.
- **Windows:** the installer closes the app while it updates.
- **Linux:** the AppImage updates itself. If you installed the `.deb` or `.rpm` and **Update** says
  the update couldn't be installed, download it from the release page instead. Tauri's
  documentation only describes updating the AppImage.
- **macOS:** whether an app that updated itself opens without macOS's security prompt hasn't been
  tested yet. If it's blocked, follow the first-launch steps in
  [Getting started](getting-started.md#first-launch).

If an update fails, nothing is changed: try again, or download it from the release page.
