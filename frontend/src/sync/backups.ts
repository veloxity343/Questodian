// Cloud backups: with sync on, a full backup in the sync folder once a day when something changed,
// kept for a while (the last 14 days' and one a week for 8 weeks before), to restore from the Sync
// panel. A backup is the Data page's full backup, except that handout images point to the image
// files sync already keeps in the folder (`sync_image`) instead of each backup holding a copy.

import type { Folder } from "./folder";

const API = `${import.meta.env.BASE_URL}api/`;
const DAILY = 14;
const WEEKLY = 8;
/** Set when something changes here or arrives, cleared by a backup. */
const CHANGED_KEY = "questodian-changed-since-cloud-backup";

/** A backup file's name: `backup-<date>-<time>[-<what>].json[.gz]` (UTC; the time keeps two made
 *  the same day apart). */
const BACKUP = /^backup-(\d{4}-\d{2}-\d{2})(?:-(\d{6}))?(?:-[\w-]+)?\.json(?:\.gz)?$/;

export interface CloudBackup {
  name: string;
  /** Its day, YYYY-MM-DD (UTC). */
  date: string;
  /** When it was made, if its name says. */
  made?: Date;
}

export function backupsIn(listing: string[]): CloudBackup[] {
  return listing
    .map((name) => ({ name, match: BACKUP.exec(name) }))
    .filter((b) => b.match)
    .map((b) => {
      const [, date, time] = b.match!;
      const made = time ? new Date(`${date}T${time.slice(0, 2)}:${time.slice(2, 4)}:${time.slice(4)}Z`) : undefined;
      return { name: b.name, date, made };
    })
    .sort((a, b) => b.name.localeCompare(a.name));
}

/** The Monday of a date's week, to keep one backup a week. */
function week(date: string): string {
  const d = new Date(`${date}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() - ((d.getUTCDay() + 6) % 7));
  return d.toISOString().slice(0, 10);
}

/** The backups to remove: every backup from the last `DAILY` days that have one stays (a backup
 *  made before a restore among them); before those, the newest of each of `WEEKLY` weeks. */
export function backupsToRemove(backups: CloudBackup[]): string[] {
  const newestFirst = [...backups].sort((a, b) => b.date.localeCompare(a.date) || b.name.localeCompare(a.name));
  const days = [...new Set(newestFirst.map((b) => b.date))].slice(0, DAILY);
  const weeks = new Set(days.map(week)); // weeks with a backup kept already
  let weekly = 0;
  const remove: string[] = [];
  for (const b of newestFirst) {
    if (days.includes(b.date)) continue;
    if (!weeks.has(week(b.date)) && weekly < WEEKLY) {
      weeks.add(week(b.date));
      weekly++;
      continue;
    }
    remove.push(b.name);
  }
  return remove;
}

export function noteChanged() {
  try {
    localStorage.setItem(CHANGED_KEY, "1");
  } catch {
    // a backup is then made only once a day regardless
  }
}

function changedSinceBackup(): boolean {
  try {
    return localStorage.getItem(CHANGED_KEY) !== "0";
  } catch {
    return true;
  }
}

function noteBackedUp() {
  try {
    localStorage.setItem(CHANGED_KEY, "0");
  } catch {
    // see noteChanged
  }
}

const today = () => new Date().toISOString().slice(0, 10);

async function sha256(bytes: ArrayBuffer): Promise<string> {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return [...digest].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** The extension sync names an image with (crates/core/src/sync.rs, `image_name`). */
function extension(filename: string): string {
  const dot = filename.lastIndexOf(".");
  const ext = dot > 0 ? filename.slice(dot + 1).toLowerCase() : "";
  return ext && /^[a-z0-9]+$/.test(ext) ? ext : "png";
}

function fromBase64(text: string): Uint8Array<ArrayBuffer> {
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

async function toBase64(bytes: ArrayBuffer): Promise<string> {
  const url = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(new Blob([bytes]));
  });
  return url.slice(url.indexOf(",") + 1);
}

const compresses = typeof CompressionStream !== "undefined" && typeof DecompressionStream !== "undefined";

interface Bundle {
  collections: { handouts?: { filename?: string; file_b64?: string; sync_image?: string }[] };
}

/** Save a full backup to the folder now. `what` names one made for a reason ("before-restore"). */
export async function backUp(cloud: Folder, listing: string[], what = ""): Promise<string> {
  const res = await fetch(`${API}export`);
  if (!res.ok) throw new Error(`making the backup: ${res.status}`);
  const bundle = (await res.json()) as Bundle;
  for (const handout of bundle.collections.handouts ?? []) {
    if (!handout.file_b64) continue;
    const bytes = fromBase64(handout.file_b64);
    const name = `image-${await sha256(bytes.buffer)}.${extension(handout.filename || "")}`;
    if (!listing.includes(name)) {
      await cloud.put(name, new Blob([bytes]));
      listing.push(name);
    }
    delete handout.file_b64;
    handout.sync_image = name;
  }
  const json = new Blob([JSON.stringify(bundle)], { type: "application/json" });
  const time = new Date().toISOString().slice(11, 19).replaceAll(":", "");
  const base = `backup-${today()}-${time}${what ? `-${what}` : ""}`;
  const name = compresses ? `${base}.json.gz` : `${base}.json`;
  const body = compresses ? await new Response(json.stream().pipeThrough(new CompressionStream("gzip"))).blob() : json;
  await cloud.put(name, new Blob([body], { type: compresses ? "application/gzip" : "application/json" }));
  listing.push(name);
  noteBackedUp();
  return name;
}

/** During a sync: back up if there's no backup from today and something changed since the last,
 *  then remove backups past keeping. */
export async function backUpIfDue(cloud: Folder, listing: string[]) {
  const backups = backupsIn(listing);
  if (!backups.some((b) => b.date === today()) && (changedSinceBackup() || !backups.length)) await backUp(cloud, listing);
  for (const name of backupsToRemove(backupsIn(listing))) {
    await cloud.remove(name);
    listing.splice(listing.indexOf(name), 1);
  }
}

/** Make this device's data exactly what the backup holds (the current state is backed up first,
 *  so it can be undone). Sync then takes it to the other devices. */
export async function restore(cloud: Folder, name: string) {
  const listing = await cloud.list();
  await backUp(cloud, listing, "before-restore");
  let blob = new Blob([await cloud.get(name)]);
  if (name.endsWith(".gz")) {
    if (!compresses) throw new Error("this browser can't open compressed backups");
    blob = await new Response(blob.stream().pipeThrough(new DecompressionStream("gzip"))).blob();
  }
  const bundle = JSON.parse(await blob.text()) as Bundle;
  for (const handout of bundle.collections.handouts ?? []) {
    if (!handout.sync_image) continue;
    handout.file_b64 = await toBase64(await cloud.get(handout.sync_image));
    delete handout.sync_image;
  }
  const res = await fetch(`${API}import?mode=replace`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(bundle),
  });
  if (!res.ok) throw new Error(`restoring: ${((await res.json().catch(() => ({}))) as { detail?: string }).detail || res.status}`);
}
