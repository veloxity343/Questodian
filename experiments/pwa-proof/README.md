# PWA proof

Can Questodian's Rust core run inside a browser, so the app works on an iPad with no computer?
This crate tests the pieces, outside the main workspace.

1. **axum in WebAssembly:** done. An axum 0.8.9 `Router` (no default features) compiles to
   `wasm32-unknown-unknown` and answers requests inside a Web Worker.
2. **SQLite in WebAssembly, stored in the browser (OPFS):** next. Needs `rusqlite` 0.43 (its
   default `ffi-sqlite-wasm-rs` feature) and `sqlite-wasm-vfs` 0.3 (`sahpool`), which compile
   SQLite's C code, so they need clang with the WebAssembly target.

Build and try it:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130
cargo build --target wasm32-unknown-unknown --release
wasm-bindgen --target web --out-dir web/pkg target/wasm32-unknown-unknown/release/pwa_proof.wasm
python3 -m http.server 8850 --directory web
```

Then open http://localhost:8850.
