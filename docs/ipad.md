# Questodian on an iPad

iPad apps can only be built on a Mac. With a free Apple ID, Xcode installs the app on your
own iPad over a cable, and Apple lets it run for **7 days**; after that, reconnect the iPad
and install it again (step 5). Your data stays on the iPad between installs.

A paid Apple Developer Program membership ($99/year) lifts the 7-day limit and allows
installing over the air with TestFlight; the build steps are the same.

## 1. Set up the Mac (once)

1. **Xcode**: install it from the Mac App Store, then open it once and let it install its
   components. Check with `xcodebuild -version` in Terminal.
2. **Homebrew**: follow the one-line install on https://brew.sh.
3. **CocoaPods**: `brew install cocoapods`
4. **Node.js**: `brew install node`
5. **Rust**, with the iOS targets:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
   ```
   (Open a new Terminal window after installing Rust.)
6. **Your Apple ID in Xcode**: Xcode → Settings → Accounts → **+** → Apple ID. It appears
   with a "(Personal Team)".

## 2. Get the code (once)

```bash
git clone https://github.com/veloxity343/Questodian.git
cd Questodian
npm install
npm install --prefix frontend
npx tauri ios init
```

`tauri ios init` creates the Xcode project in `src-tauri/gen/apple` (not stored in git;
run it again if you delete that folder).

## 3. Prepare the iPad (once)

1. Connect it to the Mac with a cable, unlock it, and tap **Trust** when it asks.
2. Turn on **Developer Mode**: Settings → Privacy & Security → Developer Mode (this option
   appears after Xcode has seen the iPad), then restart when asked.

## 4. Build and install

```bash
npm run build --prefix frontend   # the UI is compiled into the app
npx tauri ios dev --open
```

This opens the project in Xcode. Then, in Xcode:

1. Select the **Questodian** project in the left sidebar, then **Signing & Capabilities**,
   and set **Team** to your Personal Team.
2. At the top, choose your iPad as the destination.
3. Press **Run** (▶).

The first time, the iPad refuses to open the app until you trust it: Settings → General →
VPN & Device Management → your Apple ID → **Trust**. Then open Questodian.

## 5. Every 7 days, or after an update

```bash
git pull
npm install --prefix frontend
npm run build --prefix frontend
npx tauri ios dev --open
```

and press **Run** in Xcode with the iPad connected.

## What's different on the iPad (for now)

- **Show to players** isn't available: iPad apps can't open a second window. It says so
  when tapped.
- **Exports** (backups, Markdown) and **imports** may not work yet: iPad apps handle files
  differently from browsers. Please report what happens.
- Keyboard shortcut hints are hidden on touch screens; with a keyboard attached, **Ctrl K**
  and **R** still work.

## If something goes wrong

- **"development team" errors** from `tauri ios init` or `tauri ios dev`: Tauri reads your
  team from the `APPLE_DEVELOPMENT_TEAM` environment variable. Once Xcode has made your
  signing certificate (after step 4.1, or Xcode → Settings → Accounts → Manage
  Certificates → **+** → Apple Development), this prints your team ID after `OU=`:
  ```bash
  security find-certificate -c "Apple Development" -p | openssl x509 -noout -subject
  ```
  Then run `export APPLE_DEVELOPMENT_TEAM=<that ID>` and try again in the same Terminal.
- **Free Apple ID limits**: up to 3 apps installed this way per device, and the 7-day
  expiry above.
- Anything else: copy the error from Terminal or Xcode and report it.
