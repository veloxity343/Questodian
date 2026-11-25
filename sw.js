// The web version's service worker (see boot.ts). Built into sw.js at the top of the site by
// vite.config.ts, which fills in PRECACHE: every file of the build, and a version that changes
// with them.
//
// - Keeps the app's files, so the app opens offline. A new build installs alongside and takes
//   over once every window of the old one has closed, so open windows never mix versions.
// - Answers `/api/...` by passing the request to the host window (the one with the database
//   open), which answers it from its worker.

const PRECACHE = {"version":"5364afa9686d","build":"2026-11-25T23:55:57.161Z","files":["","apple-touch-icon.png","assets/alegreya-latin-400-italic-BXpar-rJ.woff2","assets/alegreya-latin-600-normal-BeiX5Wwc.woff2","assets/alegreya-latin-700-normal-Vpz_43wJ.woff2","assets/alegreya-latin-ext-400-italic-Cf8WgXZg.woff2","assets/alegreya-latin-ext-600-normal-CDdhKDsc.woff2","assets/alegreya-latin-ext-700-normal-CEFih5UF.woff2","assets/alegreya-sans-latin-400-italic-rmlAbObI.woff2","assets/alegreya-sans-latin-400-normal-DtLlO8BS.woff2","assets/alegreya-sans-latin-500-normal-DS713u5J.woff2","assets/alegreya-sans-latin-700-normal-DEbcy2sF.woff2","assets/alegreya-sans-latin-ext-400-italic-DX574ivH.woff2","assets/alegreya-sans-latin-ext-400-normal-D7N6b6bd.woff2","assets/alegreya-sans-latin-ext-500-normal-TH5u60nj.woff2","assets/alegreya-sans-latin-ext-700-normal-HuHd3-Mg.woff2","assets/boot-CKeLApK0.js","assets/db-worker-BTao5ARp.js","assets/index-4EAFeJKz.css","assets/index-C35x8oc3.js","assets/questodian_web_bg-BXJHG9cS.wasm","icon-192.png","icon-512.png","icon.svg","index.html","manifest.webmanifest"]};
const CACHE = `questodian-${PRECACHE.version}`;
const SCOPE = self.registration.scope; // e.g. https://example.github.io/Questodian/
const API = new URL("api/", SCOPE).href;
/** How long a request waits for a window to open the database. */
const HOST_WAIT = 15000;
/** How long a write waits for its answer (an import of a large backup takes a while). */
const REPLY_WAIT = 120000;
/** How long a read waits before it's asked again (of whichever window hosts by then). */
const READ_WAIT = 30000;

self.addEventListener("install", (event) => {
  event.waitUntil(
    // Fresh from the network: the HTTP cache may still hold the previous build's index.html.
    caches.open(CACHE).then((cache) => cache.addAll(PRECACHE.files.map((f) => new Request(new URL(f, SCOPE), { cache: "reload" })))),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k.startsWith("questodian-") && k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const url = event.request.url;
  if (url.startsWith(API)) {
    // Only the app's own pages may write. A form on another website that posts here arrives as
    // a navigation; the desktop app refuses those by their Origin, and so does this.
    const write = event.request.method !== "GET" && event.request.method !== "HEAD";
    event.respondWith(write && event.request.mode === "navigate" ? problem(403, "cross-origin request refused") : relay(event.request));
  } else if (event.request.method === "GET" && url.startsWith(SCOPE)) {
    event.respondWith(fromCache(event.request));
  }
});

/** The app's own files from the cache; any page of the app is index.html (the router shows it). */
async function fromCache(request) {
  const cache = await caches.open(CACHE);
  if (request.mode === "navigate") {
    return (await cache.match(SCOPE)) || (await cache.match(new URL("index.html", SCOPE).href)) || fetch(request);
  }
  return (await cache.match(request, { ignoreSearch: true })) || fetch(request);
}

// --------------------------------------------------------------------------- the host window

let hostId = null;
let hostError = null;
const hostWaiters = new Set();

self.addEventListener("message", (event) => {
  const data = event.data || {};
  if (data.type === "host") {
    hostId = event.source.id;
    hostError = null;
  } else if (data.type === "host-failed") {
    hostError = data.error;
  } else if (data.type === "claim") {
    event.waitUntil(self.clients.claim());
    return;
  } else if (data.type === "build") {
    event.ports[0]?.postMessage(PRECACHE.build);
    return;
  } else {
    return;
  }
  for (const wake of hostWaiters) wake();
});

/** The host window, asking the open windows if it isn't known (this worker may have been
 *  stopped and started again since). Null if none answers in time. */
async function findHost() {
  if (hostId) {
    const client = await self.clients.get(hostId);
    if (client) return client;
    hostId = null;
  }
  const deadline = Date.now() + HOST_WAIT;
  while (!hostId && !hostError && Date.now() < deadline) {
    for (const client of await self.clients.matchAll({ type: "window" })) client.postMessage({ type: "who-hosts" });
    await new Promise((resolve) => {
      const wake = () => resolve();
      hostWaiters.add(wake);
      setTimeout(() => {
        hostWaiters.delete(wake);
        resolve();
      }, 1000);
    });
  }
  return hostId ? (await self.clients.get(hostId)) || null : null;
}

function problem(status, detail) {
  return new Response(JSON.stringify({ detail }), { status, headers: { "content-type": "application/json" } });
}

async function relay(request) {
  const body = request.method === "GET" || request.method === "HEAD" ? undefined : await request.arrayBuffer();
  const headers = [];
  const type = request.headers.get("content-type");
  if (type) headers.push(["content-type", type]);
  const path = "/" + request.url.slice(SCOPE.length); // the API's own address: /api/...
  const read = request.method === "GET" || request.method === "HEAD";
  for (let attempt = 0; attempt < 3; attempt++) {
    const host = await findHost();
    if (!host) {
      return problem(
        503,
        hostError ? `Questodian couldn't open its data in this browser: ${hostError}` : "Questodian's data isn't open yet. Try again in a moment.",
      );
    }
    const reply = await new Promise((resolve) => {
      const channel = new MessageChannel();
      channel.port1.onmessage = (e) => resolve(e.data);
      // No answer (the window closed mid-request, say): a read is asked again; a write is
      // reported rather than sent again, since it may have happened.
      setTimeout(() => resolve(read ? { notHost: true } : { timedOut: true }), read ? READ_WAIT : REPLY_WAIT);
      // A copy of the body each try, since sending it hands it over.
      const copy = body && body.slice(0);
      host.postMessage({ type: "api", method: request.method, url: path, headers, body: copy }, [channel.port2, ...(copy ? [copy] : [])]);
    });
    if (reply.timedOut) return problem(504, "Questodian didn't answer in time. Check that your change was saved.");
    if (reply.notHost) {
      hostId = null;
      continue;
    }
    const empty = reply.status === 204 || reply.status === 304 || request.method === "HEAD";
    return new Response(empty ? null : reply.body, { status: reply.status, headers: reply.headers });
  }
  return problem(503, "Questodian's data isn't open yet. Try again in a moment.");
}
