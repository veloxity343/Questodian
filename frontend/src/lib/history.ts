// Undo and redo: what something was before each change, and what undoing took away.

export interface History<T> {
  past: T[];
  future: T[];
}

export const emptyHistory = <T>(): History<T> => ({ past: [], future: [] });

/** Remember `before` as the state to go back to; a new change clears what could be redone. */
export function remember<T>(h: History<T>, before: T, limit = 100): History<T> {
  return { past: [...h.past, before].slice(-limit), future: [] };
}

/** Step back from `now`, or null when there's nothing to undo. */
export function undo<T>(h: History<T>, now: T): { history: History<T>; value: T } | null {
  if (!h.past.length) return null;
  return { history: { past: h.past.slice(0, -1), future: [now, ...h.future] }, value: h.past[h.past.length - 1] };
}

/** Step forward again from `now`, or null when there's nothing to redo. */
export function redo<T>(h: History<T>, now: T): { history: History<T>; value: T } | null {
  if (!h.future.length) return null;
  return { history: { past: [...h.past, now], future: h.future.slice(1) }, value: h.future[0] };
}
