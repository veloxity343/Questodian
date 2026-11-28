// Running sync: moves files between the cloud folder and the core's sync steps
// (crates/core/src/sync.rs, /api/sync/...), one sync at a time across every window, and keeps a
// status to show.
//
// When: on opening the app, a little after a change, every few minutes, and when the app goes
// into the background. Offline, or signed out, nothing happens and changes wait.

import { useSyncExternalStore } from "react";
import { DriveError, googleDrive } from "./drive";
import type { Folder } from "./folder";
import { currentGoogleToken, forgetGoogleToken, signInToGoogle, signOutOfGoogle } from "./google";

const API = `${import.meta.env.BASE_URL}api/`;
const SETTINGS_KEY = "questodian-sync";
/** After a change, wait this long for more before syncing. */
const AFTER_CHANGE_MS = 15_000;
/** And sync this often anyway, to get other devices' changes. */
const EVERY_MS = 5 * 60_000;
/** Change files are applied in batches of about this size (the API takes up to 20 MB a request). */
const BATCH_BYTES = 8 * 1024 * 1024;
/** Windows tell each other when a sync brought changes, or sync was connected or disconnected. */
const channel = typeof BroadcastChannel === "undefined" ? null : new BroadcastChannel("questodian-sync");

export type Phase = "off" | "idle" | "syncing" | "paused" | "error";

export interface SyncStatus {
  phase: Phase;
  /** When the last sync finished, if one has. */
  last?: number;
  /** Records changed here by the last sync. */
  received?: number;
  /** Changes not yet sent. */
  waiting?: number;
  /** Versions replaced by another device's, kept to restore. */
  replaced?: number;
  error?: string;
  /** Records that couldn't be written by the last sync, and why. */
  skipped?: string[];
}

interface Settings {
  provider: "google";
}

/** Where settings are kept if the browser won't keep them (private browsing, storage full). */
let memorySettings: Settings | null = null;
let stored = true;

/** Whether sync is connected, read fresh each time: another window may have changed it. */
function settings(): Settings | null {
  if (!stored) return memorySettings;
  try {
    return JSON.parse(localStorage.getItem(SETTINGS_KEY) || "null") as Settings | null;
  } catch {
    return memorySettings;
  }
}

function saveSettings(value: Settings | null) {
  memorySettings = value;
  try {
    if (value) localStorage.setItem(SETTINGS_KEY, JSON.stringify(value));
    else localStorage.removeItem(SETTINGS_KEY);
    stored = true;
  } catch {
    stored = false; // kept in memory instead
  }
  channel?.postMessage("settings");
}

let status: SyncStatus = { phase: settings() ? "idle" : "off" };
const listeners = new Set<() => void>();

function update(change: Partial<SyncStatus>) {
  status = { ...status, ...change };
  for (const l of listeners) l();
}

/** The sync status, kept current. */
export function useSyncStatus(): SyncStatus {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => status,
  );
}

async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(API + path, {
    method,
    headers: body === undefined ? {} : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  let data: unknown = null;
  try {
    data = text ? JSON.parse(text) : null;
  } catch {
    data = { detail: text };
  }
  if (!res.ok) throw new Error((data as { detail?: string })?.detail || `${res.status}`);
  return data as T;
}

interface Applied {
  changed: number;
  older: number;
  skipped: string[];
}
interface Outgoing {
  name: string;
  file: unknown;
  images: { name: string; handout: string }[];
}

function folder(): Folder | null {
  if (settings()?.provider !== "google") return null;
  if (!currentGoogleToken()) return null;
  return googleDrive(() => currentGoogleToken() || "");
}

/** The name sync gives an image with these bytes: `image-<sha256>.<ext>`. */
async function hashOf(bytes: ArrayBuffer): Promise<string> {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return [...digest].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** Fetch and apply other devices' change files, in batches. Returns what came in. */
async function receive(cloud: Folder, listing: string[]): Promise<{ received: number; skipped: string[] }> {
  let received = 0;
  let skipped: string[] = [];
  // A record waiting for one from a later batch is applied in the next round.
  for (let round = 0; round < 3; round++) {
    const { files: wanted } = await call<{ files: string[] }>("POST", "sync/wanted", { listing });
    if (!wanted.length) break;
    const decoder = new TextDecoder();
    let progress = 0;
    skipped = [];
    for (let start = 0; start < wanted.length; ) {
      const batch: unknown[] = [];
      let size = 0;
      while (start < wanted.length && (batch.length === 0 || size < BATCH_BYTES)) {
        const bytes = await cloud.get(wanted[start++]);
        size += bytes.byteLength;
        batch.push(JSON.parse(decoder.decode(bytes)));
      }
      const { images } = await call<{ images: string[] }>("POST", "sync/images-needed", { files: batch });
      for (const name of images) {
        if (!listing.includes(name)) continue; // not in the folder: that record waits
        const res = await fetch(`${API}sync/images/${encodeURIComponent(name)}`, { method: "PUT", body: await cloud.get(name) });
        if (!res.ok) throw new Error(`keeping ${name}: ${res.status}`);
      }
      const applied = await call<Applied>("POST", "sync/apply", { files: batch });
      received += applied.changed;
      progress += applied.changed;
      skipped.push(...applied.skipped);
    }
    if (!skipped.length || !progress) break;
  }
  if (received) {
    window.dispatchEvent(new Event("questodian:synced"));
    channel?.postMessage("synced");
  }
  return { received, skipped };
}

/** Send this device's changes, each file after the images it uses. */
async function send(cloud: Folder, listing: string[]) {
  for (;;) {
    const out = await call<Outgoing | null>("POST", "sync/outgoing", { listing });
    if (!out) return;
    for (const image of out.images) {
      if (listing.includes(image.name)) continue;
      const res = await fetch(`${API}handouts/${encodeURIComponent(image.handout)}/file`);
      if (!res.ok) throw new Error(`couldn't read a handout's image (${res.status}): trying again next sync`);
      const bytes = await res.arrayBuffer();
      // Changed since: the change file is made again next sync, with the new image.
      if (!image.name.startsWith(`image-${await hashOf(bytes)}.`)) throw new Error("a handout's image changed while syncing: trying again");
      await cloud.put(image.name, new Blob([bytes], { type: res.headers.get("Content-Type") || "application/octet-stream" }));
      listing.push(image.name);
    }
    await cloud.put(out.name, new Blob([JSON.stringify(out.file)], { type: "application/json" }));
    listing.push(out.name);
    await call("POST", "sync/sent", { name: out.name });
  }
}

/** One sync: fetch and apply other devices' changes, then send this device's. */
async function syncOnce(cloud: Folder): Promise<void> {
  const listing = await cloud.list();
  const { received, skipped } = await receive(cloud, listing);
  await send(cloud, listing);
  const counts = await call<{ waiting: number; replaced: number }>("GET", "sync");
  const phase = settings() ? "idle" : "off"; // disconnected meanwhile
  update({ phase, last: Date.now(), received, skipped, waiting: counts.waiting, replaced: counts.replaced, error: undefined });
}

let running: Promise<void> | null = null;

/** Sync now, unless sync is off, offline, or needs a tap to sign in again (then "paused"). */
export function syncNow(): Promise<void> {
  if (running) return running;
  if (!settings()) {
    if (status.phase !== "off") update({ phase: "off" });
    return Promise.resolve();
  }
  if (!navigator.onLine) return Promise.resolve();
  const cloud = folder();
  if (!cloud) {
    update({ phase: "paused" });
    return Promise.resolve();
  }
  update({ phase: "syncing" });
  // One sync at a time across this browser's windows (where the browser can say so).
  running = (navigator.locks ? navigator.locks.request("questodian-sync", () => syncOnce(cloud)) : syncOnce(cloud))
    .catch((e: Error) => {
      if (e instanceof DriveError && e.status === 401) forgetGoogleToken(); // Google refused it: sign in again
      if (!settings()) update({ phase: "off", error: undefined }); // disconnected meanwhile
      else update({ phase: currentGoogleToken() ? "error" : "paused", error: e.message });
    })
    .finally(() => {
      running = null;
    });
  return running;
}

/** Connect Google Drive: call straight from a tap. */
export async function connectGoogleDrive() {
  await signInToGoogle(true);
  saveSettings({ provider: "google" });
  update({ phase: "idle", error: undefined });
  await syncNow();
}

/** After "paused": sign in again (from a tap) and carry on. */
export async function resumeSync() {
  await signInToGoogle(false);
  await syncNow();
}

/** Stop syncing on this device. What's here stays; the cloud folder is left as it is. */
export async function disconnect() {
  saveSettings(null);
  update({ phase: "off", error: undefined, skipped: undefined });
  await signOutOfGoogle();
}

let started = false;

/** Start syncing on a schedule (once, at startup). */
export function startSync() {
  if (started) return;
  started = true;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const soon = (ms: number) => {
    clearTimeout(timer);
    timer = setTimeout(() => void syncNow(), ms);
  };
  const counts = () =>
    void call<{ waiting: number; replaced: number }>("GET", "sync")
      .then((c) => update({ waiting: c.waiting, replaced: c.replaced }))
      .catch(() => {});
  if (channel) {
    channel.onmessage = (e) => {
      if (e.data === "synced") window.dispatchEvent(new Event("questodian:synced"));
      if (e.data === "settings") {
        update({ phase: settings() ? (status.phase === "off" ? "idle" : status.phase) : "off" });
        if (settings()) soon(1000);
      }
      counts();
    };
  }
  window.addEventListener("questodian:changed", () => soon(AFTER_CHANGE_MS));
  window.addEventListener("online", () => soon(1000));
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) void syncNow();
    else soon(1000);
  });
  setInterval(() => void syncNow(), EVERY_MS);
  if (settings()) counts();
  soon(2000);
}
