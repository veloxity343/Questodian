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

Without more setup, **Update** opens the release page and you download the new version
yourself. To have the app download, verify and install it, then restart, releases must be
signed with an updater key (Tauri's updater plugin checks every download against it). This is
set up once:

1. Make the key pair on your own computer, and keep the private key safe. Losing it (or its
   password) means installed copies can't update in place any more.

   ```bash
   npm run tauri signer generate -- -w ~/.tauri/questodian.key
   ```

2. In the repository's settings on GitHub, add two Actions secrets:
   `TAURI_SIGNING_PRIVATE_KEY` (the contents of `~/.tauri/questodian.key`) and
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (its password).

3. Then the code changes, which need the public key (`~/.tauri/questodian.key.pub`):
   - `src-tauri/tauri.conf.json`: `"bundle": { "createUpdaterArtifacts": true }` and
     `"plugins": { "updater": { "pubkey": "<contents of the .pub file>", "endpoints":
     ["https://github.com/veloxity343/Questodian/releases/latest/download/latest.json"] } }`
   - build the desktop app with `--features updater` (the release workflow's tauri-action
     `args`), with the two secrets in that step's `env`, and `tagName` set so `latest.json`
     points at this release's files.

   After the first release made this way, check that its `latest.json` lists all four
   platforms. Copies from before that release still update by downloading by hand.

Notes from the Tauri docs: on Linux the updater replaces an AppImage (a `.deb` or `.rpm` install
updates by downloading by hand); on Windows the installer closes the app itself. Whether an
ad-hoc signed macOS app updated in place opens without the Security prompt is untested.
