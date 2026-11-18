# Updates

The desktop app checks GitHub for a newer release.

- On first launch it asks whether to check. The answer, the last check and the newest version
  found are kept in `updates.json` in the app's config folder (not the data folder).
- While checking is on, it checks once a day, or 3 hours after a check that failed. Settings
  (⚙ in the top bar) turns it on or off and has **Check now**.
- A check is one HTTPS `HEAD` request to `https://github.com/veloxity343/Questodian/releases/latest`.
  GitHub answers with a redirect to the latest release's page; the version is read from that
  address, which must be exactly `.../releases/tag/vX.Y.Z` of this repository. Nothing about the
  user is sent, and the reply's body isn't read.
- A newer version gets one pop-up; Settings keeps offering it until the app is updated.
- In a browser (`questodian serve`) there's no update check: get new versions from Releases.

Code: `src-tauri/src/updates.rs` (the check and its schedule), `src-tauri/src/main.rs`
(`/desktop/updates` routes), `frontend/src/context/updates.tsx` and `views/settings.tsx`.

## Installing updates in place

Release builds include Tauri's updater (the `updater` cargo feature) and are signed with the
updater key, so **Update** downloads the new version, checks its signature against the public key
in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`), installs it and restarts. Builds made
without the feature (`make app`, `make desktop`) open the release page instead.

- **The key.** The private key and its password are the repository's Actions secrets
  `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; the owner keeps a copy.
  Losing it (or its password) means installed copies can't update in place any more: they'd need
  a new key in `tauri.conf.json` and one more download by hand.
- **The release.** The Release workflow builds with `--features updater` and
  `--config src-tauri/tauri.release.conf.json`, which turns on `createUpdaterArtifacts` (kept out
  of the main config so local builds don't need the private key). Each build adds its platform to
  the release's `latest.json`; the last job checks all four are there, since the builds write it
  at the same time. If that check fails, re-run the build job for the missing platform before
  publishing.
- **Finding updates.** The app reads
  `https://github.com/veloxity343/Questodian/releases/latest/download/latest.json`, which only
  points at a published release, so a draft is never offered.

The first release built this way is the first that can update itself; copies from before it
update by downloading by hand once.

Notes from the Tauri docs: on Linux the updater replaces an AppImage (a `.deb` or `.rpm` install
updates by downloading by hand); on Windows the installer closes the app itself. Whether an
ad-hoc signed macOS app updated in place opens without the Security prompt is untested.
