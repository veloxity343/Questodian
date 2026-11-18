# Releasing

[Documentation](../README.md) › Releasing

Releases are built on GitHub by the **Release** workflow (`.github/workflows/release.yml`), from a
version tag.

## Steps

1. **Bump the version** in all of these (they must match):
   - `Cargo.toml` (`[workspace.package] version`), then run `cargo update -w` so `Cargo.lock` follows;
   - `src-tauri/tauri.conf.json` (`version`);
   - `frontend/package.json` and `frontend/package-lock.json` (the top-level `version` entries).

   `main` only takes pull requests, so do this on a branch. The easiest way is on the feature
   branch itself, before it's merged; otherwise a small PR of its own.
2. **Merge** to `main`, with the checks passing.
3. **Tag** the merge commit and push the tag:

   ```bash
   git tag -a v0.2.2 origin/main -m "Questodian 0.2.2"
   ```

   ```bash
   git push origin v0.2.2
   ```

   The tag must match the version in `tauri.conf.json`, or the workflow stops. You can also start
   the workflow from the Actions tab.
4. **Wait for the workflow:**
   1. it runs every CI check;
   2. it creates a draft release;
   3. it builds the desktop app for Windows x64, macOS (Apple Silicon and Intel) and Linux x64, and
      attaches each one's installers to the draft;
   4. it checks `latest.json` (see below).

   If a build job is cancelled or fails, for instance because no macOS machine was free, use
   **Re-run failed jobs**. The re-run uploads into the same draft.
5. **Check the draft:** every installer is there, each update file has its `.sig`, and the last job
   (**Check latest.json lists every platform**) passed.
6. **Write the release notes** and **publish**, with **Set as the latest release** ticked. Installed
   copies only see published releases, so a draft is never offered as an update.

## Update signing

Release builds include Tauri's updater (the `updater` cargo feature), so installed copies can
install updates themselves. Each update is signed, and the app checks the signature against the
public key in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`) before installing it.

- **The private key and its password** are the repository's Actions secrets
  `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The project owner keeps a
  copy. If either is lost, installed copies can't update themselves any more: they'd need a new key
  in `tauri.conf.json` and one more download by hand.
- **The build:** the workflow passes `--features updater --config src-tauri/tauri.release.conf.json`
  to the build. That extra config turns on `createUpdaterArtifacts`, the signed update files; it's
  kept out of the main config so local builds (`make desktop`) don't need the private key.
- **`latest.json`** is what installed copies read, from
  `https://github.com/veloxity343/Questodian/releases/latest/download/latest.json`. Each build job
  adds its platform to the release's copy. The four jobs write it at the same time, with nothing to
  stop one overwriting another, so the workflow's last job checks that `darwin-aarch64`,
  `darwin-x86_64`, `linux-x86_64` and `windows-x86_64` are all there. If one is missing, re-run that
  platform's build job before publishing. The job needs `contents: write`, because GitHub only shows
  a draft release to tokens that can write.

Builds without the `updater` feature (`make app`, `make desktop`) still check for updates, but
**Update** opens the release page instead.

### Making a new key

Only if the key is lost or compromised. Copies with the old key then need one more download by hand.

```bash
npm run tauri signer generate -- -w ~/.tauri/questodian.key
```

Run it from the repository folder (after `make install`). Put the new `.pub` file's contents in
`plugins.updater.pubkey`, and replace both secrets.

## The update check

How the app looks for new versions is in [Updates](../guide/updates.md). In the code:
- `src-tauri/src/updates.rs` checks, and keeps the schedule in `updates.json` in the app's config
  folder (not the data folder);
- `src-tauri/src/main.rs` serves the `/desktop/updates` routes and installs;
- `frontend/src/context/updates.tsx` and `views/settings.tsx` show it.

A check is one HTTPS `HEAD` request to `https://github.com/veloxity343/Questodian/releases/latest`.
GitHub answers with a redirect to the latest release's page, and the version is read from that
address, which must be exactly `…/releases/tag/vX.Y.Z` of this repository. Nothing about the user
is sent, and the reply's body isn't read.
