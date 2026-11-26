// Backups in the web version, whose data lives only in this browser until it's saved elsewhere.

import { usePref } from "../lib/hooks";
import { saveAsDownload, sharesFiles } from "../lib/util";

/** Days after a full backup before the Data link reminds you. */
export const BACKUP_EVERY_DAYS = 7;

/** When this browser last saved a full backup (ISO time), "" if never. */
export const useLastBackup = () => usePref<string>("web.lastBackup", "");

/** Whole days since `iso`, or null if never. */
export function daysSince(iso: string): number | null {
  const then = Date.parse(iso);
  return Number.isNaN(then) ? null : Math.floor((Date.now() - then) / 86_400_000);
}

/** Whether the web version should remind you to back up. */
export function backupDue(lastBackup: string, hasData: boolean): boolean {
  if (import.meta.env.MODE !== "web" || !hasData) return false;
  const days = daysSince(lastBackup);
  return days === null || days >= BACKUP_EVERY_DAYS;
}

/** What became of a file being saved: saved, cancelled, or waiting for another tap (the browser
 *  only opens the Share sheet straight after a tap, and preparing the file can take longer). */
export type Saving = "saved" | "cancelled" | "tap-again";

/** Save `text` as `filename`, shared or downloaded (as `download` does), and say what happened.
 *  A share that fails any other way falls back to a download. */
export async function saveFile(filename: string, text: string): Promise<Saving> {
  if (sharesFiles()) {
    const file = new File([text], filename, { type: "application/json" });
    try {
      await navigator.share({ files: [file] });
      return "saved";
    } catch (e) {
      const name = (e as Error).name;
      if (name === "AbortError") return "cancelled";
      if (name === "NotAllowedError") return "tap-again";
    }
  }
  saveAsDownload(filename, new Blob([text], { type: "application/json" }));
  return "saved";
}
