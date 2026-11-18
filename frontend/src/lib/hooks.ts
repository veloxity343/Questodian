import { useCallback, useEffect, useRef, useState } from "react";
import { readPref, writePref, zoomAround } from "./util";

/** The latest value of each pref, in case localStorage is unavailable. */
const prefs = new Map<string, unknown>();

/** State that persists in this browser's localStorage, shared by every component using the same key. */
export function usePref<T>(key: string, fallback: T) {
  const [value, setValue] = useState<T>(() => (prefs.has(key) ? prefs.get(key) : readPref(key, fallback)) as T);
  useEffect(() => {
    const changed = (e: Event) => setValue((e as CustomEvent<T>).detail);
    window.addEventListener(`pref:${key}`, changed);
    return () => window.removeEventListener(`pref:${key}`, changed);
  }, [key]);
  const set = useCallback(
    (v: T | ((prev: T) => T)) => {
      const prev = (prefs.has(key) ? prefs.get(key) : readPref(key, fallback)) as T;
      const next = typeof v === "function" ? (v as (p: T) => T)(prev) : v;
      prefs.set(key, next);
      writePref(key, next);
      // Tell the other components using this pref (e.g. the top bar's ☰ and the DM screen).
      window.dispatchEvent(new CustomEvent(`pref:${key}`, { detail: next }));
    },
    [key],
  );
  return [value, set] as const;
}

/** Load async data; `reload` re-runs the loader. Errors surface via `error`. */
export function useLoad<T>(load: () => Promise<T>, deps: unknown[]) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const [tick, setTick] = useState(0);
  useEffect(() => {
    let live = true;
    setError(null);
    load().then(
      (d) => live && setData(d),
      (e: Error) => live && setError(e),
    );
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, tick]);
  const reload = useCallback(() => setTick((t) => t + 1), []);
  return { data, setData, error, reload };
}

export function useDebounced<A extends unknown[]>(fn: (...args: A) => void, ms: number) {
  const fnRef = useRef(fn);
  fnRef.current = fn;
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  return useCallback(
    (...args: A) => {
      clearTimeout(timer.current);
      timer.current = setTimeout(() => fnRef.current(...args), ms);
    },
    [ms],
  );
}

export type SaveStatus = "" | "Saving…" | "Saved" | "Save failed";

/**
 * An editable copy of a server record that saves itself.
 *
 * `edit(fn)` applies a mutation to a deep copy and schedules a debounced save;
 * `flush()` saves immediately (call it before server actions or navigation);
 * pending edits are also flushed on unmount.
 */
export function useDraft<T>(
  initial: T | null,
  save: (doc: T) => Promise<unknown>,
  { delay = 600, onError }: { delay?: number; onError?: (e: Error) => void } = {},
) {
  const [doc, setDocState] = useState<T | null>(initial);
  const [status, setStatus] = useState<SaveStatus>("");
  const docRef = useRef(doc);
  const pending = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const saveRef = useRef(save);
  saveRef.current = save;
  const errRef = useRef(onError);
  errRef.current = onError;

  const flush = useCallback(async () => {
    clearTimeout(timer.current);
    if (!pending.current || !docRef.current) return;
    pending.current = false;
    try {
      await saveRef.current(docRef.current);
      setStatus("Saved");
    } catch (e) {
      setStatus("Save failed");
      errRef.current?.(e as Error);
    }
  }, []);

  /** Replace the draft without saving (e.g. with a fresh copy from the server). */
  const setDoc = useCallback((next: T | null) => {
    docRef.current = next;
    setDocState(next);
  }, []);

  const edit = useCallback(
    (fn: (draft: T) => void) => {
      if (!docRef.current) return;
      const next = structuredClone(docRef.current);
      fn(next);
      docRef.current = next;
      setDocState(next);
      pending.current = true;
      setStatus("Saving…");
      clearTimeout(timer.current);
      timer.current = setTimeout(flush, delay);
    },
    [delay, flush],
  );

  /** Drop any unsaved edits (e.g. right before deleting the record). */
  const discard = useCallback(() => {
    clearTimeout(timer.current);
    pending.current = false;
  }, []);

  useEffect(() => () => void flush(), [flush]);

  return { doc, setDoc, edit, flush, discard, status };
}

/**
 * Drag things into a new order: the DM screen's tabs, a page's panels. Each item carries
 * `data-reorder={id}`; `start(id, e)` from its (or its handle's) pointerdown. Past a few pixels it's
 * a drag, and the click that ends it is swallowed. Pointer events rather than HTML drag and drop,
 * which some browsers won't start from a button and desktop webviews can take over.
 *
 * With `lane` (items of one size in a row, like tabs), `onMove(id, target)` runs live as the
 * pointer comes over each item; the pointer only counts across, and near either end the lane
 * scrolls along. Without it (items of mixed sizes, which reflow as they move), `over` names the
 * item under the pointer and `onMove` runs once, on release. Escape cancels; with a lane, the
 * live moves are undone by `onCancel`.
 */
export function useDragReorder(
  onMove: (id: string, target: string) => void,
  { lane, onCancel }: { lane?: { current: HTMLElement | null }; onCancel?: () => void } = {},
) {
  const [dragging, setDragging] = useState<string | null>(null);
  const [over, setOver] = useState<string | null>(null);
  const press = useRef<{ id: string; x: number; y: number; moved: boolean; over: string; cancelled?: boolean } | null>(null);
  const moveRef = useRef(onMove);
  moveRef.current = onMove;
  const cancelRef = useRef(onCancel);
  cancelRef.current = onCancel;

  useEffect(() => {
    const onPointerMove = (e: PointerEvent) => {
      const p = press.current;
      if (!p || p.cancelled) return;
      if (!p.moved && Math.hypot(e.clientX - p.x, e.clientY - p.y) < 6) return;
      if (!p.moved) {
        p.moved = true;
        setDragging(p.id);
      }
      let { clientX: x, clientY: y } = e;
      const row = lane?.current;
      if (row) {
        const box = row.getBoundingClientRect();
        if (x < box.left + 16) row.scrollLeft -= 12;
        else if (x > box.right - 16) row.scrollLeft += 12;
        x = Math.min(box.right - 2, Math.max(box.left + 2, x));
        y = box.top + box.height / 2;
      }
      const under = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-reorder]")?.dataset.reorder ?? "";
      if (under === p.over) return;
      p.over = under;
      if (!row) setOver(under && under !== p.id ? under : null);
      else if (under && under !== p.id) moveRef.current(p.id, under);
    };
    const onUp = (e: PointerEvent) => {
      const p = press.current;
      press.current = null;
      if (!p?.moved) return;
      setDragging(null);
      setOver(null);
      if (!p.cancelled && !lane?.current && e.type === "pointerup" && p.over && p.over !== p.id) moveRef.current(p.id, p.over);
      // The release would also click what's under it; swallow that one click.
      const swallow = (e: MouseEvent) => { e.stopPropagation(); e.preventDefault(); };
      window.addEventListener("click", swallow, { capture: true, once: true });
      setTimeout(() => window.removeEventListener("click", swallow, { capture: true }), 0);
    };
    const onKey = (e: KeyboardEvent) => {
      const p = press.current;
      if (e.key !== "Escape" || !p?.moved || p.cancelled) return;
      // Stay pressed until release, so that release's click is still swallowed.
      p.cancelled = true;
      setDragging(null);
      setOver(null);
      if (lane?.current) cancelRef.current?.();
    };
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      window.removeEventListener("keydown", onKey);
    };
  }, [lane]);

  const start = useCallback((id: string, e: { button: number; clientX: number; clientY: number }) => {
    if (e.button === 0) press.current = { id, x: e.clientX, y: e.clientY, moved: false, over: id };
  }, []);
  return { dragging, over, start };
}

/**
 * Wheel to zoom around the cursor and drag to pan, for anything drawn with
 * `translate(x, y) scale(zoom)`: handout images and maps. `start` begins a pan from a pointerdown,
 * `move` follows it (false when not panning), `end` stops it.
 */
export function usePanZoom<V extends { zoom: number; x: number; y: number }>(view: V, onView: ((v: V) => void) | undefined, limits: [number, number]) {
  const from = useRef<{ x: number; y: number } | null>(null);
  return {
    onWheel: (e: { clientX: number; clientY: number; deltaY: number; deltaMode: number; ctrlKey: boolean; currentTarget: Element }) =>
      onView?.(zoomAround(view, e, e.currentTarget.getBoundingClientRect(), limits)),
    start: (e: { clientX: number; clientY: number }) => { from.current = { x: e.clientX - view.x, y: e.clientY - view.y }; },
    move: (e: { clientX: number; clientY: number }) => {
      if (!from.current) return false;
      onView?.({ ...view, x: e.clientX - from.current.x, y: e.clientY - from.current.y });
      return true;
    },
    end: () => { from.current = null; },
  };
}
