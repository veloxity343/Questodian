import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// `npm run dev` serves the UI on :5173 and forwards /api to the Rust server on :8765
// (`questodian serve`). `npm run build` writes the production bundle to dist/, which the
// desktop app compiles in and `questodian serve` serves.
export default defineConfig({
  plugins: [react()],
  server: {
    // Keep the browser's Host header (the string shorthand would rewrite it), so the
    // API's same-origin check for writes sees matching Origin and Host.
    proxy: { "/api": { target: "http://127.0.0.1:8765", changeOrigin: false } },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});
