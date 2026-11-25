// Runs the Rust core inside a Web Worker, as the real app would: the page posts requests, the
// worker answers them through the router.
import init, { request } from "./pkg/pwa_proof.js";
const ready = init();
self.onmessage = async ({ data }) => {
  await ready;
  try {
    self.postMessage({ id: data.id, ok: true, body: await request(data.method, data.path) });
  } catch (e) {
    self.postMessage({ id: data.id, ok: false, body: String(e) });
  }
};
