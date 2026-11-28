// Running sync: moves files between the cloud folder and the core's sync steps
// (crates/core/src/sync.rs, /api/sync/...), one sync at a time, and keeps a status to show.
//
// When: on opening the app, a little after a change, every few minutes, and when the app goes
// into the background. Offline, or signed out, nothing happens and changes wait.

import { useSyncExternalStore } from "react";
import { googleDrive } from "./drive";
import type { Folder } from "./folder";
import { currentGoogleToken, signInToGoogle, signOutOfGoogle } from "./google";

const API = `${import.meta.env.BASE_URL}api/`;
const SETTINGS_KEY = "questodian-sync";
/** After a change, wait this long for more before syncing. */
const AFTER_CHANGE_MS = 15_000;
/** And sync this often anyway, to get other devices' changes. */
const EVERY_MS = 5 * 60_000;

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

function settings(): Settings | null {
  try {
    return JSON.parse(localStorage.getItem(SETTINGS_KEY) || "null") as Settings | null;
  } catch {
    return null;
  }
}

function saveSettings(value: Settings | null) {
  try {
    if (value) localStorage.setItem(SETTINGS_KEY, JSON.stringify(value));
    else localStorage.removeItem(SETTINGS_KEY);
  } catch {
    // private browsing: sync stays on while the page is open
  }
  connected = value;
}

let connected: Settings | null = settings();
let status: SyncStatus = { phase: connected ? "idle" : "off" };
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
  const data = text ? JSON.parse(text) : null;
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
  if (connected?.provider !== "google") return null;
  if (!currentGoogleToken()) return null;
  return googleDrive(() => currentGoogleToken() || "");
}

/** One sync: fetch and apply other devices' changes, then send this device's. */
async function syncOnce(cloud: Folder): Promise<void> {
  let listing = await cloud.list();
  const { files: wanted } = await call<{ files: string[] }>("POST", "sync/wanted", { listing });
  const decoder = new TextDecoder();
  const files = [];
  for (const name of wanted) files.push(JSON.parse(decoder.decode(await cloud.get(name))));
  let received = 0;
  let skipped: string[] = [];
  if (files.length) {
    const { images } = await call<{ images: string[] }>("POST", "sync/images-needed", { files });
    for (const name of images) {
      if (!listing.includes(name)) continue; // not sent yet: that record is skipped this time
      const res = await fetch(`${API}sync/images/${encodeURIComponent(name)}`, { method: "PUT", body: await cloud.get(name) });
      if (!res.ok) throw new Error(`keeping ${name}: ${res.status}`);
    }
    const applied = await call<Applied>("POST", "sync/apply", { files });
    received = applied.changed;
    skipped = applied.skipped;
    if (applied.changed) window.dispatchEvent(new Event("questodian:synced"));
  }
  for (;;) {
    const out = await call<Outgoing | null>("POST", "sync/outgoing", { listing });
    if (!out) break;
    for (const image of out.images) {
      if (listing.includes(image.name)) continue;
      const res = await fetch(`${API}handouts/${encodeURIComponent(image.handout)}/file`);
      if (!res.ok) continue; // the image went meanwhile; the next change to the handout sends it
      await cloud.put(image.name, await res.blob());
      listing = [...listing, image.name];
    }
    await cloud.put(out.name, new Blob([JSON.stringify(out.file)], { type: "application/json" }));
    listing = [...listing, out.name];
    await call("POST", "sync/sent", { name: out.name });
  }
  const counts = await call<{ waiting: number; replaced: number }>("GET", "sync");
  update({ phase: "idle", last: Date.now(), received, skipped, waiting: counts.waiting, replaced: counts.replaced, error: undefined });
}

let running: Promise<void> | null = null;

/** Sync now, unless sync is off, offline, or needs a tap to sign in again (then "paused"). */
export function syncNow(): Promise<void> {
  if (running) return running;
  if (!connected) return Promise.resolve();
  if (!navigator.onLine) return Promise.resolve();
  const cloud = folder();
  if (!cloud) {
    update({ phase: "paused" });
    return Promise.resolve();
  }
  update({ phase: "syncing" });
  running = (navigator.locks ? navigator.locks.request("questodian-sync", () => syncOnce(cloud)) : syncOnce(cloud))
    .catch((e: Error) => update({ phase: currentGoogleToken() ? "error" : "paused", error: e.message }))
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
  window.addEventListener("questodian:changed", () => soon(AFTER_CHANGE_MS));
  window.addEventListener("online", () => soon(1000));
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) void syncNow();
    else soon(1000);
  });
  setInterval(() => void syncNow(), EVERY_MS);
  if (connected) void call<{ waiting: number; replaced: number }>("GET", "sync").then((c) => update({ waiting: c.waiting, replaced: c.replaced })).catch(() => {});
  soon(2000);
}
