// Runs the Rust core inside a dedicated Web Worker, as the real app would (OPFS storage needs one):
// the page posts requests, the worker answers them through the router.
import init, { open, request } from "./pkg/pwa_proof.js";
const ready = init().then(() => open());
self.onmessage = async ({ data }) => {
  try {
    const sqlite = await ready;
    const body = data.path === "/sqlite" ? sqlite : await request(data.method, data.path, data.body ?? null);
    self.postMessage({ id: data.id, ok: true, body });
  } catch (e) {
    self.postMessage({ id: data.id, ok: false, body: String(e) });
  }
};
