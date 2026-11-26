import init, { open, request } from "./pkg/questodian_web.js";
const ready = init().then(() => open());
onmessage = async ({ data: { id, method, url, headers, body } }) => {
  try {
    const version = await ready;
    if (url === "/sqlite") return postMessage({ id, ok: true, status: 200, headers: "[]", body: new TextEncoder().encode(version) });
    const r = await request(method, url, JSON.stringify(headers || []), body);
    postMessage({ id, ok: true, status: r.status, headers: r.headers, body: r.body });
  } catch (e) { postMessage({ id, ok: false, error: String(e && e.message || e) }); }
};
