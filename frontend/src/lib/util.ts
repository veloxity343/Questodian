// Small formatting and browser helpers shared by every view.

import type { Ability } from "../types";

export const ABILITIES: Ability[] = ["str", "dex", "con", "int", "wis", "cha"];
export const ABILITY_NAMES: Record<Ability, string> = {
  str: "Strength", dex: "Dexterity", con: "Constitution", int: "Intelligence", wis: "Wisdom", cha: "Charisma",
};

export const signed = (n: number) => (n >= 0 ? `+${n}` : `${n}`);
export const modOf = (score: number | string) => Math.floor((Number(score || 10) - 10) / 2);
export const profBonus = (level: number) => 2 + Math.floor((Math.min(Math.max(level, 1), 20) - 1) / 4);
export const titleCase = (s: string) => String(s).replace(/[-_]/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
export const slug = (s: string, fallback = "export") =>
  String(s).replace(/\W+/g, "-").replace(/^-|-$/g, "").toLowerCase() || fallback;
export const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
export const today = () => new Date().toISOString().slice(0, 10);

export function fmtDate(iso: string) {
  if (!iso) return "";
  const d = new Date(iso);
  return isNaN(d.getTime()) ? iso : d.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

export function hpLevel(pct: number) {
  return pct > 50 ? "ok" : pct > 25 ? "warn" : "bad";
}

export const d20 = (bonus: number) => `d20${signed(bonus)}`;

/**
 * Zoom a pan-and-zoom view by one wheel step, keeping the point under the pointer still.
 * `x`/`y` are offsets from the centre of `box`, so the same view looks the same in a window of
 * another size (the players' screen mirrors the DM's).
 */
export function zoomAround<V extends { zoom: number; x: number; y: number }>(
  view: V,
  wheel: { clientX: number; clientY: number; deltaY: number },
  box: DOMRect,
  [min, max]: [number, number],
): V {
  const cx = wheel.clientX - box.left - box.width / 2;
  const cy = wheel.clientY - box.top - box.height / 2;
  const zoom = Math.max(min, Math.min(max, view.zoom * (wheel.deltaY < 0 ? 1.15 : 1 / 1.15)));
  const k = zoom / view.zoom;
  return { ...view, zoom, x: cx - (cx - view.x) * k, y: cy - (cy - view.y) * k };
}

/** Save text or a file the browser already has (a PNG, say) as a download. */
export function download(filename: string, content: string | Blob, type = "application/json") {
  const blob = typeof content === "string" ? new Blob([content], { type }) : content;
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = filename;
  document.body.append(a);
  a.click();
  setTimeout(() => {
    URL.revokeObjectURL(a.href);
    a.remove();
  }, 500);
}

export const downloadJson = (filename: string, data: unknown) => download(filename, JSON.stringify(data, null, 2));

export interface PickedFile {
  name: string;
  text: string;
}

export function pickFiles({ accept = "", multiple = false } = {}): Promise<PickedFile[]> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = accept;
    input.multiple = multiple;
    input.style.display = "none";
    input.addEventListener("change", async () => {
      const files = await Promise.all([...(input.files || [])].map(async (f) => ({ name: f.name, text: await f.text() })));
      input.remove();
      resolve(files);
    });
    document.body.append(input);
    input.click();
  });
}

// ------------------------------------------------------------------ browser-local preferences

const KEY = (k: string) => `questodian.${k}`;

export function readPref<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(KEY(key));
    return v === null ? fallback : (JSON.parse(v) as T);
  } catch {
    return fallback;
  }
}

export function writePref(key: string, value: unknown) {
  try {
    localStorage.setItem(KEY(key), JSON.stringify(value));
  } catch {
    /* storage unavailable: preferences just won't persist */
  }
}

/** `order` with `x` moved next to `target`: after it, or before it. Others keep their places
 *  (a page's hidden panels too). Used to rearrange panels and the DM screen's tabs. */
export function placed<T>(order: T[], x: T, target: T, after: boolean): T[] {
  const next = order.filter((y) => y !== x);
  const at = next.indexOf(target);
  if (at < 0) return order;
  next.splice(after ? at + 1 : at, 0, x);
  return next;
}
