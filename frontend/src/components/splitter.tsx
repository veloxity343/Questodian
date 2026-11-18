// A handle you drag to change a size: the DM screen's columns or a panel's height.

import { useRef, type PointerEvent as ReactPointerEvent } from "react";

export function Splitter({ axis, start, onChange, min = 0, max = Infinity, invert = false, onRelease, onReset, className = "", label, title }: {
  /** "x" changes a width (drag sideways), "y" a height (drag up and down). */
  axis: "x" | "y";
  /** The size when a drag starts. */
  start: () => number;
  onChange: (size: number) => void;
  min?: number;
  /** The largest size; a function when it depends on the room there is at the time. */
  max?: number | (() => number);
  /** The size grows as the pointer moves left or up (a handle on the far side of what it sizes). */
  invert?: boolean;
  /** Called with the final size when the drag ends. */
  onRelease?: (size: number) => void;
  /** Double-click. */
  onReset?: () => void;
  className?: string;
  label?: string;
  title?: string;
}) {
  const drag = useRef<{ at: number; size: number; last: number } | null>(null);
  const pos = (e: ReactPointerEvent) => (axis === "x" ? e.clientX : e.clientY);
  return (
    <div className={className} role="separator" aria-orientation={axis === "x" ? "vertical" : "horizontal"} aria-label={label} title={title}
      onPointerDown={(e) => {
        const size = start();
        drag.current = { at: pos(e), size, last: size };
        e.currentTarget.setPointerCapture(e.pointerId);
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d) return;
        const top = typeof max === "function" ? max() : max;
        d.last = Math.max(min, Math.min(top, Math.round(d.size + (pos(e) - d.at) * (invert ? -1 : 1))));
        onChange(d.last);
      }}
      onPointerUp={() => {
        if (drag.current) onRelease?.(drag.current.last);
        drag.current = null;
      }}
      onDoubleClick={onReset} />
  );
}
