// "Check this browser" on the Data page: what the web version relies on, tried in this browser,
// so a problem on a device nobody can test from here (an iPad, say) shows up as a report. It
// adds one handout, shows it, and deletes it again; nothing else changes.

import { sharesFiles } from "../lib/util";

export interface Check {
  name: string;
  /** Passed, failed, or null for information only. */
  ok: boolean | null;
  detail?: string;
}

const API = `${import.meta.env.BASE_URL}api/`;
const PNG = Uint8Array.from(
  atob("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgAAAAAgABVKJPXQAAAABJRU5ErkJggg=="),
  (c) => c.charCodeAt(0),
);

interface WorkerReport {
  build: string;
  files: number;
  cached: number;
  seesAsker: boolean;
  windows: number;
  host: boolean;
}

function askServiceWorker(): Promise<WorkerReport | null> {
  return new Promise((resolve) => {
    const controller = navigator.serviceWorker?.controller;
    if (!controller) return resolve(null);
    const channel = new MessageChannel();
    channel.port1.onmessage = (e) => resolve(e.data as WorkerReport);
    controller.postMessage({ type: "check" }, [channel.port2]);
    setTimeout(() => resolve(null), 20000);
  });
}

async function call(method: string, path: string, body?: unknown, headers: Record<string, string> = {}) {
  const res = await fetch(API + path, {
    method,
    headers: body === undefined ? headers : { "Content-Type": "application/json", ...headers },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`${method} ${path}: ${res.status} ${(await res.text()).slice(0, 200)}`);
  return res.json();
}

function imageLoads(src: string): Promise<boolean> {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => resolve(img.naturalWidth === 1);
    img.onerror = () => resolve(false);
    img.src = src;
  });
}

const mb = (bytes = 0) => `${Math.round(bytes / 1e6)} MB`;

export async function checkThisBrowser(): Promise<Check[]> {
  const checks: Check[] = [];
  const add = (name: string, ok: boolean | null, detail?: string) => checks.push({ name, ok, detail });
  const attempt = async (name: string, run: () => Promise<string | void>) => {
    try {
      add(name, true, (await run()) || undefined);
    } catch (e) {
      add(name, false, (e as Error).message);
    }
  };

  add("Browser", null, navigator.userAgent);
  const homeScreen = matchMedia("(display-mode: standalone)").matches || (navigator as { standalone?: boolean }).standalone === true;
  add("Opened from the Home Screen", null, homeScreen ? "yes" : "no (in a browser tab)");

  add("The service worker runs this page", !!navigator.serviceWorker?.controller);
  const report = await askServiceWorker();
  if (!report) {
    add("The service worker answers", false, "no answer in 20 seconds");
  } else {
    add("Page and service worker are the same version", report.build === __QUESTODIAN_BUILD__, `${__QUESTODIAN_BUILD__} / ${report.build}`);
    add("Every file of the app is kept for offline use", report.cached >= report.files, `${report.cached} of ${report.files}`);
    add("The service worker sees this window", report.seesAsker);
    add("A window has the data open", report.host, `${report.windows} Questodian window(s) open`);
  }

  await attempt("The data opens", async () => `Questodian ${(await call("GET", "meta")).version}`);

  const persisted = await navigator.storage?.persisted?.().catch(() => false);
  add("The browser keeps this data when space runs low", persisted ? true : null, persisted ? "yes" : "not promised (the browser decides)");
  const estimate = await navigator.storage?.estimate?.().catch(() => null);
  if (estimate) add("Storage", null, `${mb(estimate.usage)} used of ${mb(estimate.quota)} allowed`);

  const made: string[] = [];
  try {
    await attempt("A save, its image, and reading them back", async () => {
      const handout = await call("POST", "handouts", { title: "Questodian self-test (deleted straight away)" });
      made.push(handout.id);
      const res = await fetch(`${API}handouts/${handout.id}/file`, { method: "PUT", headers: { "Content-Type": "image/png" }, body: PNG });
      if (!res.ok) throw new Error(`image upload: ${res.status}`);
      if ((await call("GET", `handouts/${handout.id}`)).title !== handout.title) throw new Error("read back differently");
      if (!(await imageLoads(`${API}handouts/${handout.id}/file?v=${Date.now()}`))) throw new Error("the image doesn't show");
    });
    await attempt("A save sent twice happens once", async () => {
      const id = crypto.randomUUID();
      const body = { title: "Questodian self-test (deleted straight away)" };
      const first = await call("POST", "handouts", body, { "x-questodian-save": id });
      made.push(first.id);
      const second = await call("POST", "handouts", body, { "x-questodian-save": id });
      if (second.id !== first.id) {
        made.push(second.id);
        throw new Error("it was saved twice");
      }
    });
  } finally {
    for (const id of made) await call("DELETE", `handouts/${id}`).catch(() => {});
  }

  add("Windows take turns with the data (Web Locks)", !!navigator.locks?.query);
  add("Windows can tell each other things (BroadcastChannel)", "BroadcastChannel" in self);
  add("Backups open the Share sheet (Save to Files)", null, sharesFiles() ? "yes" : "no: they download");
  return checks;
}

/** The checks as plain text, to copy and send. */
export function reportText(checks: Check[]): string {
  const mark = (c: Check) => (c.ok === null ? "-" : c.ok ? "ok" : "FAIL");
  return ["Questodian browser check", new Date().toISOString(), ...checks.map((c) => `${mark(c)}  ${c.name}${c.detail ? `: ${c.detail}` : ""}`)].join("\n");
}
