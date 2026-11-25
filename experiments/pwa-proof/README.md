# PWA proof

Can Questodian's Rust core run inside a browser, so the app works on an iPad with no computer?
This crate tests the pieces, outside the main workspace. The real version is now `crates/web`.

1. **axum in WebAssembly:** done. An axum 0.8.9 `Router` (no default features) compiles to
   `wasm32-unknown-unknown` and answers requests inside a Web Worker.
2. **SQLite in WebAssembly, stored in the browser (OPFS):** done. `rusqlite` 0.43 (its default
   `ffi-sqlite-wasm-rs` feature) with `sqlite-wasm-vfs` 0.3 (`sahpool`): SQLite 3.54.1 in the
   worker, records written through the router, still there after reloads and in a new tab.
   Building it compiles SQLite's C code, so it needs clang (`sudo apt install clang llvm`).
3. **On an iPad:** done. Hosted on GitHub Pages (OPFS needs HTTPS), the page ran in Safari on an
   iPad and the campaigns list kept growing across reloads.

What it showed:
- The `.wasm` is 2.2 MB, 0.78 MB gzipped, before any size optimisation.
- Only one tab can have the database open (OPFS sync access handles are exclusive): a second tab
  gets `NoModificationAllowedError` until the first closes. The app will need to say so plainly.
- The sahpool VFS doesn't do WAL, so the browser database uses SQLite's default journal.

Build and try it:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130
cargo build --target wasm32-unknown-unknown --release
wasm-bindgen --target web --out-dir web/pkg target/wasm32-unknown-unknown/release/pwa_proof.wasm
python3 -m http.server 8850 --directory web
```

Then open http://localhost:8850.
