// A page of panels you can arrange: drag to reorder, set each one's width, drag its height,
// collapse it, or hide it. The layout is kept per page on this device.

import { useEffect, useRef, useState, type DragEvent, type ReactNode } from "react";
import { usePref } from "../lib/hooks";
import { Splitter } from "./splitter";

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

/** `order` with `x` moved next to `target`: after it, or before it. Hidden panels keep their places. */
export function placed(order: string[], x: string, target: string, after: boolean): string[] {
  const next = order.filter((y) => y !== x);
  const at = next.indexOf(target);
  if (at < 0) return order;
  next.splice(after ? at + 1 : at, 0, x);
  return next;
}

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
  const move = (x: string, target: string | undefined, after: boolean) =>
    target && change((l) => ({ ...l, order: placed(l.order, x, target, after) }));
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
            <Tile key={x} item={item} width={layout.width[x] ?? item.width ?? "half"} height={layout.height[x]}
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
              onMove={(dir) => move(x, shown[i + dir], dir > 0)}
              onDragStart={() => setDragging(x)}
              onDragEnd={() => setDragging("")}
              onDropHere={() => dragging && dragging !== x && move(dragging, x, order.indexOf(dragging) < order.indexOf(x))} />
          );
        })}
      </div>
    </div>
  );
}

function Tile({ item, width, height, collapsed, arranging, dragging, onCollapse, onHide, onWidth, onHeight, onMove, onDragStart, onDragEnd, onDropHere }: {
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
  const tile = useRef<HTMLDivElement>(null);
  const body = useRef<HTMLDivElement>(null);
  const [span, setSpan] = useState(1);
  const [empty, setEmpty] = useState(false);
  const [over, setOver] = useState(false);

  // Masonry: a tile spans as many grid rows as it is tall (its body's padding is the gap below).
  useEffect(() => {
    const el = tile.current;
    if (!el || !body.current) return;
    const measure = () => {
      setEmpty(!body.current?.firstElementChild);
      setSpan(Math.max(1, Math.ceil(el.getBoundingClientRect().height / ROW)));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(body.current);
    return () => observer.disconnect();
  }, []);

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
    <div ref={tile} className={`tile w-${width} ${collapsed ? "collapsed" : ""} ${dragging ? "dragging" : ""} ${over ? "drop-here" : ""} ${empty ? "empty" : ""}`}
      style={{ gridRowEnd: `span ${span}` }} {...drop}>
      <div ref={body} className="tile-body" style={fixed ? { height, overflowY: "auto" } : undefined}>
        {item.node}
      </div>
      <div className="tile-tools">
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
        <Splitter className="height-handle" axis="y" min={80} start={() => body.current?.getBoundingClientRect().height ?? 200}
          onChange={onHeight} onReset={() => onHeight(null)} title="Drag to set the height; double-click to fit the content" label={`${item.title} height`} />
      )}
    </div>
  );
}
