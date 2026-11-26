// The web version (`make pwa`): Questodian's core runs in this browser instead of on a server.
//
// - `db-worker.ts` runs the core (WebAssembly, crates/web) with the database in the browser's
//   own storage. Only one worker can have that open, so one window is the host.
// - `sw.js`, the service worker, answers every `/api/...` request (fetches, images, the players'
//   window) by passing it to the host window, which passes it to its worker. It also keeps the
//   app's files, so the app opens offline.
// - Every window queues for the "host" Web Lock; the one holding it opens the database. When
//   that window closes, the next in the queue takes over.

const BASE = import.meta.env.BASE_URL;

interface WorkerReply {
  id: number;
  ok: boolean;
  error?: string;
  status?: number;
  headers?: [string, string][];
  body?: Uint8Array;
}

let worker: Worker | null = null;
const waiting = new Map<number, { worker: Worker; resolve: (reply: WorkerReply) => void }>();
let nextId = 0;

function ask(w: Worker, message: Record<string, unknown>, transfer: Transferable[] = []): Promise<WorkerReply> {
  const id = nextId++;
  return new Promise((resolve) => {
    waiting.set(id, { worker: w, resolve });
    w.postMessage({ ...message, id }, transfer);
  });
}

function startWorker(): Worker {
  const w = new Worker(new URL("./db-worker.ts", import.meta.url), { type: "module" });
  w.onmessage = ({ data }: MessageEvent<WorkerReply>) => {
    waiting.get(data.id)?.resolve(data);
    waiting.delete(data.id);
  };
  // The worker couldn't load or crashed: whatever it was asked fails rather than waiting forever.
  const failed = (error: string) => {
    for (const [id, entry] of waiting) {
      if (entry.worker !== w) continue;
      entry.resolve({ id, ok: false, error });
      waiting.delete(id);
    }
  };
  w.onerror = (e) => failed(e.message || "the database worker stopped");
  w.onmessageerror = () => failed("a message to the database worker couldn't be read");
  return w;
}

const tellServiceWorker = (message: Record<string, unknown>) => navigator.serviceWorker.controller?.postMessage(message);

/** Open the database in a worker of this window. A window that just closed may still be letting
 *  go of the storage, so a few tries. */
async function host(): Promise<boolean> {
  let error = "";
  for (let attempt = 0; attempt < 5; attempt++) {
    const w = startWorker();
    const reply = await ask(w, { type: "open" });
    if (reply.ok) {
      worker = w;
      tellServiceWorker({ type: "host" });
      // Ask the browser not to clear the data when space runs low (it decides).
      void navigator.storage?.persist?.().catch(() => false);
      return true;
    }
    w.terminate();
    error = reply.error || "unknown error";
    await new Promise((r) => setTimeout(r, 400 * (attempt + 1)));
  }
  tellServiceWorker({ type: "host-failed", error });
  return false;
}

/** A request from the service worker: answer it if this window is the host. */
async function onServiceWorkerMessage(event: MessageEvent) {
  const data = event.data as { type: string; method?: string; url?: string; headers?: [string, string][]; body?: ArrayBuffer };
  if (data.type === "who-hosts") {
    if (worker) tellServiceWorker({ type: "host" });
    return;
  }
  if (data.type !== "api") return;
  const port = event.ports[0];
  if (!worker) {
    port.postMessage({ notHost: true });
    return;
  }
  const reply = await ask(worker, { type: "request", method: data.method, url: data.url, headers: data.headers, body: data.body },
    data.body ? [data.body] : []);
  if (!reply.ok) {
    port.postMessage({ status: 500, headers: [["content-type", "application/json"]], body: new TextEncoder().encode(JSON.stringify({ detail: reply.error })) });
    return;
  }
  port.postMessage({ status: reply.status, headers: reply.headers, body: reply.body }, reply.body ? [reply.body.buffer] : []);
}

/** Wait until the service worker controls this page (a first visit, or a reload that skipped it). */
async function controlled(registration: ServiceWorkerRegistration) {
  if (navigator.serviceWorker.controller) return;
  const changed = new Promise<void>((resolve) =>
    navigator.serviceWorker.addEventListener("controllerchange", () => resolve(), { once: true }));
  (registration.active || (await navigator.serviceWorker.ready).active)?.postMessage({ type: "claim" });
  await changed;
}

/** The first install downloads every file of the app; if that fails, say so instead of waiting. */
function installed(registration: ServiceWorkerRegistration): Promise<unknown> {
  const failed = new Promise((_, reject) => {
    const installing = registration.installing;
    installing?.addEventListener("statechange", () => {
      if (installing.state === "redundant" && !navigator.serviceWorker.controller) {
        reject(new Error("it couldn't download all of its files. Check the connection and reload"));
      }
    });
  });
  return Promise.race([navigator.serviceWorker.ready, failed]);
}

/** The build the service worker serves. */
function serviceWorkerBuild(): Promise<string> {
  return new Promise((resolve) => {
    const channel = new MessageChannel();
    channel.port1.onmessage = (e) => resolve(String(e.data));
    navigator.serviceWorker.controller?.postMessage({ type: "build" }, [channel.port2]);
    setTimeout(() => resolve(__QUESTODIAN_BUILD__), 3000); // an older worker that doesn't say
  });
}

export async function boot() {
  if (!("serviceWorker" in navigator)) throw new Error("it needs service workers, which this browser doesn't allow here");
  if (!navigator.storage?.getDirectory) throw new Error("it needs the browser's file storage (Safari 17 or later)");
  navigator.serviceWorker.addEventListener("message", (e) => void onServiceWorkerMessage(e));
  navigator.serviceWorker.startMessages();
  const registration = await navigator.serviceWorker.register(`${BASE}sw.js`, { scope: BASE });
  await installed(registration);
  await controlled(registration);
  // Every window runs the build its service worker serves, so an old and a new version never
  // share the database. A page from another build (say, loaded past the service worker)
  // reloads into the worker's; once only, in case something keeps them apart.
  if ((await serviceWorkerBuild()) !== __QUESTODIAN_BUILD__ && !sessionStorage.getItem("questodian-reloaded")) {
    sessionStorage.setItem("questodian-reloaded", "1");
    location.reload();
    await new Promise(() => {});
  }
  sessionStorage.removeItem("questodian-reloaded");
  // Host now, or once the window holding the lock closes. The app starts either way: until a
  // window hosts, the service worker holds API requests for a few seconds.
  if (navigator.locks) {
    void navigator.locks.request("questodian-host", async () => {
      if (await host()) await new Promise(() => {}); // hold the lock while this window is open
    });
  } else {
    void host();
  }
}
