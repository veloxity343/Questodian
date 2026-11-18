// A page of panels you can arrange: drag to reorder, set each one's width, drag its height,
// collapse it, or hide it. The layout is kept per page on this device.

import { useEffect, useRef, useState, type DragEvent, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";
import { usePref } from "../lib/hooks";

export type Width = "third" | "half" | "two-thirds" | "full";
const WIDTHS: [Width, string][] = [["third", "⅓"], ["half", "½"], ["two-thirds", "⅔"], ["full", "Full"]];

export interface BoardItem {
  id: string;
  /** Shown in Arrange mode and in the Hidden bar. */
  title: string;
  width?: Width;
  node: ReactNode;
}

interface Layout {
  order: string[];
  hidden: string[];
  collapsed: string[];
  width: Record<string, Width>;
  /** Fixed heights in pixels; absent means as tall as the content. */
  height: Record<string, number>;
}

const EMPTY: Layout = { order: [], hidden: [], collapsed: [], width: {}, height: {} };
/** The grid's row unit; each card spans as many as its height needs. */
const ROW = 4;
const GAP = 16;

export function Board({ id, items }: { id: string; items: BoardItem[] }) {
  const [stored, setLayout] = usePref<Layout>(`layout.${id}`, EMPTY);
  const layout = { ...EMPTY, ...stored };
  const [arranging, setArranging] = useState(false);
  const [dragging, setDragging] = useState("");

  // The saved order first (for items that still exist), then any new ones where the page puts them.
  const known = new Set(items.map((i) => i.id));
  const order = [...layout.order.filter((x) => known.has(x)), ...items.map((i) => i.id).filter((x) => !layout.order.includes(x))];
  const byId = new Map(items.map((i) => [i.id, i]));
  const shown = order.filter((x) => !layout.hidden.includes(x));
  const hidden = order.filter((x) => layout.hidden.includes(x));

  const change = (fn: (l: Layout) => Layout) => setLayout((prev) => fn({ ...EMPTY, ...prev, order }));
  const toggle = (list: "hidden" | "collapsed", x: string) =>
    change((l) => ({ ...l, [list]: l[list].includes(x) ? l[list].filter((y) => y !== x) : [...l[list], x] }));
  const move = (x: string, to: number) =>
    change((l) => {
      const next = l.order.filter((y) => y !== x);
      next.splice(Math.max(0, Math.min(next.length, to)), 0, x);
      return { ...l, order: next };
    });
  const changed = JSON.stringify(stored) !== JSON.stringify(EMPTY);

  return (
    <div className={`board ${arranging ? "arranging" : ""}`}>
      <div className="board-bar row gap wrap">
        {hidden.length > 0 && (
          <span className="row gap-sm wrap">
            <span className="muted small">Hidden:</span>
            {hidden.map((x) => (
              <button type="button" key={x} className="chip-btn" title="Show it again" onClick={() => toggle("hidden", x)}>{byId.get(x)?.title} ↺</button>
            ))}
          </span>
        )}
        <span className="grow" />
        {arranging && changed && (
          <button type="button" className="btn small" onClick={() => setLayout(EMPTY)}>Reset layout</button>
        )}
        <button type="button" className={`btn small ${arranging ? "primary" : ""}`} onClick={() => setArranging(!arranging)}
          title="Move, resize and hide the panels on this page">
          {arranging ? "Done arranging" : "Arrange"}
        </button>
      </div>
      <div className="board-grid">
        {shown.map((x, i) => {
          const item = byId.get(x)!;
          return (
            <Card key={x} item={item} width={layout.width[x] ?? item.width ?? "half"} height={layout.height[x]}
              collapsed={layout.collapsed.includes(x)} arranging={arranging} dragging={dragging === x}
              onCollapse={() => toggle("collapsed", x)}
              onHide={() => toggle("hidden", x)}
              onWidth={(w) => change((l) => ({ ...l, width: { ...l.width, [x]: w } }))}
              onHeight={(h) => change((l) => {
                const height = { ...l.height };
                if (h === null) delete height[x];
                else height[x] = h;
                return { ...l, height };
              })}
              onMove={(dir) => move(x, i + dir)}
              onDragStart={() => setDragging(x)}
              onDragEnd={() => setDragging("")}
              onDropHere={() => dragging && dragging !== x && move(dragging, order.indexOf(x))} />
          );
        })}
      </div>
    </div>
  );
}

function Card({ item, width, height, collapsed, arranging, dragging, onCollapse, onHide, onWidth, onHeight, onMove, onDragStart, onDragEnd, onDropHere }: {
  item: BoardItem;
  width: Width;
  height?: number;
  collapsed: boolean;
  arranging: boolean;
  dragging: boolean;
  onCollapse: () => void;
  onHide: () => void;
  onWidth: (w: Width) => void;
  onHeight: (h: number | null) => void;
  onMove: (dir: -1 | 1) => void;
  onDragStart: () => void;
  onDragEnd: () => void;
  onDropHere: () => void;
}) {
  const body = useRef<HTMLDivElement>(null);
  const [span, setSpan] = useState(1);
  const [empty, setEmpty] = useState(false);
  const [over, setOver] = useState(false);

  // Masonry: a card spans as many grid rows as its content is tall.
  useEffect(() => {
    const el = body.current;
    if (!el) return;
    const measure = () => {
      const h = el.getBoundingClientRect().height;
      setEmpty(h === 0 && !el.firstElementChild);
      setSpan(Math.max(1, Math.ceil((h + GAP) / ROW)));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const resize = useRef<{ y: number; h: number } | null>(null);
  const fixed = !collapsed && height !== undefined;
  const drop = {
    onDragOver: (e: DragEvent) => {
      if (!arranging) return;
      e.preventDefault();
      setOver(true);
    },
    onDragLeave: () => setOver(false),
    onDrop: (e: DragEvent) => {
      e.preventDefault();
      setOver(false);
      onDropHere();
    },
  };

  return (
    <div className={`card w-${width} ${collapsed ? "collapsed" : ""} ${dragging ? "dragging" : ""} ${over ? "drop-here" : ""} ${empty ? "empty" : ""}`}
      style={{ gridRowEnd: `span ${span}` }} {...drop}>
      <div ref={body} className="card-body" style={fixed ? { height, overflowY: "auto" } : undefined}>
        {item.node}
      </div>
      <div className="card-tools">
        {arranging && (
          <>
            <span className="drag-handle" draggable title="Drag to move" aria-label={`Move ${item.title}`}
              onDragStart={(e) => { e.dataTransfer.setData("text/plain", item.id); e.dataTransfer.effectAllowed = "move"; onDragStart(); }}
              onDragEnd={onDragEnd}>⠿</span>
            <button type="button" className="icon-btn" aria-label={`Move ${item.title} up`} title="Move earlier" onClick={() => onMove(-1)}>↑</button>
            <button type="button" className="icon-btn" aria-label={`Move ${item.title} down`} title="Move later" onClick={() => onMove(1)}>↓</button>
            <select className="small-select" aria-label={`${item.title} width`} value={width} onChange={(e) => onWidth(e.target.value as Width)}>
              {WIDTHS.map(([w, label]) => <option key={w} value={w}>{label}</option>)}
            </select>
            <button type="button" className="icon-btn" aria-label={`Hide ${item.title}`} title="Hide (bring it back from the Hidden bar)" onClick={onHide}>✕</button>
          </>
        )}
        <button type="button" className="icon-btn collapse-btn" aria-expanded={!collapsed} aria-label={`${collapsed ? "Expand" : "Collapse"} ${item.title}`}
          title={collapsed ? "Expand" : "Collapse"} onClick={onCollapse}>{collapsed ? "▸" : "▾"}</button>
      </div>
      {arranging && !collapsed && (
        <div className="height-handle" role="separator" aria-orientation="horizontal" title="Drag to set the height; double-click to fit the content"
          onPointerDown={(e: ReactPointerEvent) => {
            resize.current = { y: e.clientY, h: body.current?.getBoundingClientRect().height ?? 200 };
            (e.target as HTMLElement).setPointerCapture(e.pointerId);
          }}
          onPointerMove={(e: ReactPointerEvent) => {
            if (resize.current) onHeight(Math.max(80, Math.round(resize.current.h + e.clientY - resize.current.y)));
          }}
          onPointerUp={() => (resize.current = null)}
          onDoubleClick={() => onHeight(null)} />
      )}
    </div>
  );
}
