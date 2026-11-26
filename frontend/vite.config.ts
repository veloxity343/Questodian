import { createHash } from "node:crypto";
import { copyFileSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

// `npm run dev` serves the UI on :5173 and forwards /api to the Rust server on :8765
// (`questodian serve`). `npm run build` writes the production bundle to dist/, which the
// desktop app compiles in and `questodian serve` serves.
//
// `npm run build:web` (`make pwa`) builds the web version into dist-web/: the same UI with the
// core running in the browser (src/web/), installable and working offline. QUESTODIAN_BASE is
// where it will be served, e.g. /Questodian/ for GitHub Pages (default /).
export default defineConfig(({ mode }) => {
  const web = mode === "web";
  const base = web ? `${(process.env.QUESTODIAN_BASE || "/").replace(/\/*$/, "")}/` : "/";
  // Ties a page to its service worker: a page from another build reloads into the worker's (boot.ts).
  const build = new Date().toISOString();
  return {
    base,
    define: web ? { __QUESTODIAN_BUILD__: JSON.stringify(build) } : {},
    plugins: [react(), ...(web ? [webApp(build)] : [])],
    resolve: web ? { alias: { "questodian-web": resolve(__dirname, "../target/web-pkg/questodian_web.js") } } : {},
    // The web version's icons and manifest.
    publicDir: web ? "web-public" : "public",
    worker: { format: "es" },
    server: {
      // Keep the browser's Host header (the string shorthand would rewrite it), so the
      // API's same-origin check for writes sees matching Origin and Host.
      proxy: { "/api": { target: "http://127.0.0.1:8765", changeOrigin: false } },
    },
    build: {
      outDir: web ? "dist-web" : "dist",
      emptyOutDir: true,
    },
  };
});

/** The web version's extras: the manifest in the page, the service worker with every file of
 *  the build to keep offline, and 404.html (GitHub Pages' answer for a deep link). */
function webApp(build: string): Plugin {
  let outDir = "";
  let base = "/";
  return {
    name: "questodian-web-app",
    configResolved(config) {
      outDir = resolve(config.root, config.build.outDir);
      base = config.base;
    },
    transformIndexHtml() {
      return [
        { tag: "link", attrs: { rel: "manifest", href: `${base}manifest.webmanifest` }, injectTo: "head" },
        { tag: "link", attrs: { rel: "apple-touch-icon", href: `${base}apple-touch-icon.png` }, injectTo: "head" },
        { tag: "meta", attrs: { name: "theme-color", content: "#0e131b" }, injectTo: "head" },
      ];
    },
    closeBundle() {
      const files = walk(outDir).map((f) => relative(outDir, f).split("\\").join("/"));
      const hash = createHash("sha256");
      for (const f of files.sort()) hash.update(f).update(readFileSync(join(outDir, f)));
      const precache = { version: hash.digest("hex").slice(0, 12), build, files: ["", ...files] };
      const template = readFileSync(resolve(__dirname, "src/web/sw.js"), "utf8");
      const marker = /\/\* PRECACHE \*\/ \{[^}]*\}/;
      if (!marker.test(template)) throw new Error("src/web/sw.js: PRECACHE marker not found");
      writeFileSync(join(outDir, "sw.js"), template.replace(marker, JSON.stringify(precache)));
      copyFileSync(join(outDir, "index.html"), join(outDir, "404.html"));
    },
  };
}

function walk(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? walk(path) : [path];
  });
}
