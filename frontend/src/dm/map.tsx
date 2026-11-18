// Maps drawn in the app: a square or hex grid you paint with a legend, with walls and doors,
// text, markers, party tokens and fog. The same drawing shows on the players' screen, where
// fogged cells, secret doors and secret notes stay hidden.

import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode, type WheelEvent } from "react";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { api } from "../api";
import { Checkbox, NumInput, Select, TextInput, type Option } from "../components/ui";
import { useDialogs, useToast } from "../context/ui";
import {
  MARKER_ICONS, TOKEN_COLORS, cellAt, cellCenter, cellKey, cropTo, edgeAt, edgeLine, floodFill, hexPoints, mapSize, newId, vttWalls,
  type Edge, type GridType, type MapData, type MapToken,
} from "../lib/map";
import { download, slug, zoomAround } from "../lib/util";
import type { Character } from "../types";

export interface MapViewState { zoom: number; x: number; y: number }

// ------------------------------------------------------------------ drawing

/** How a map draws, kept inside its SVG so the screen, the players' window and every export match. */
const MAP_CSS = [
  ".map-grid{fill:none;stroke:rgba(0,0,0,.18);stroke-width:1;pointer-events:none}",
  ".map-edge{stroke-width:5;stroke-linecap:round}",
  ".map-edge.wall{stroke:#2a241e}",
  ".map-edge.door{stroke:#8a5a2b;stroke-dasharray:10 6}",
  ".map-edge.secret{stroke:#7a6cf0;stroke-dasharray:3 5}",
  ".fog{fill:rgba(20,20,30,.35);pointer-events:none}",
  ".fog.solid{fill:#0b0e13}",
  ".map-label{font-family:Alegreya,Georgia,serif;font-weight:600;text-anchor:middle;dominant-baseline:middle;paint-order:stroke;stroke:rgba(255,252,246,.85);stroke-width:.22em;stroke-linejoin:round}",
  ".map-label.secret,.map-marker.secret{opacity:.6}",
  ".map-marker text,.map-token text{text-anchor:middle;dominant-baseline:central}",
  ".map-marker-label{font-family:'Alegreya Sans',sans-serif;fill:#2a241e;paint-order:stroke;stroke:rgba(255,252,246,.85);stroke-width:.22em;stroke-linejoin:round}",
  ".map-token circle{stroke:#fff;stroke-width:2}",
  ".map-token text{fill:#fff;font-weight:700;font-family:'Alegreya Sans',sans-serif;pointer-events:none}",
].join("");

/** The map as SVG. `players` hides fog, secret doors and secret notes. */
function MapSvg({ map, players = false, svgRef, onPointerDown, onPointerMove, onPointerUp, onDoubleClick, style }: {
  map: MapData;
  players?: boolean;
  svgRef?: React.Ref<SVGSVGElement>;
  onPointerDown?: (e: ReactPointerEvent<SVGSVGElement>) => void;
  onPointerMove?: (e: ReactPointerEvent<SVGSVGElement>) => void;
  onPointerUp?: (e: ReactPointerEvent<SVGSVGElement>) => void;
  onDoubleClick?: (e: React.MouseEvent<SVGSVGElement>) => void;
  style?: React.CSSProperties;
}) {
  const { width, height } = mapSize(map);
  const { type, cols, rows, size } = map.grid;
  const colour = new Map(map.legend.map((l) => [l.id, l.color]));
  const revealed = new Set(map.fog.revealed);
  const cells: ReactNode[] = [];
  const fog: ReactNode[] = [];
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      // Only painted cells are drawn (pointer tools find cells by position, not by element).
      const key = cellKey(c, r);
      const fill = colour.get(map.cells[key] ?? "");
      const hidden = map.fog.on && !revealed.has(key);
      if (!fill && !hidden) continue;
      const shape = (props: React.SVGProps<SVGRectElement> & React.SVGProps<SVGPolygonElement>) =>
        type === "square" ? <rect key={key} x={c * size} y={r * size} width={size} height={size} {...props} /> : <polygon key={key} points={hexPoints(map, c, r)} {...props} />;
      if (fill) cells.push(shape({ fill }));
      if (hidden) fog.push(shape({ className: players ? "fog solid" : "fog" }));
    }
  }
  // On the players' screen, things under fog stay hidden. Labels sit anywhere (in cell units).
  const fogged = (c: number, r: number) => map.fog.on && !revealed.has(cellKey(c, r));
  const visible = (c: number, r: number) => !players || !fogged(c, r);
  const labelAt = (c: number, r: number) => ({ x: c * size, y: r * size });
  const labelVisible = (c: number, r: number) => {
    if (!players || !map.fog.on) return true;
    const under = cellAt(map, c * size, r * size);
    return !!under && !fogged(under.c, under.r);
  };
  return (
    <svg ref={svgRef} className="map-svg" viewBox={`0 0 ${width} ${height}`} width={width} height={height} style={style}
      onPointerDown={onPointerDown} onPointerMove={onPointerMove} onPointerUp={onPointerUp} onDoubleClick={onDoubleClick}>
      <style>{MAP_CSS}</style>
      <rect width={width} height={height} fill={map.background} />
      <g>{cells}</g>
      {Object.entries(map.walls).map(([key, kind]) => {
        const line = edgeLine(map, key);
        const [c, r] = key.split(",").map(Number);
        // A wall shows once a cell on either side of it is revealed.
        if (players && !visible(c, r) && !visible(c - (key.endsWith("w") ? 1 : 0), r - (key.endsWith("n") ? 1 : 0))) return null;
        const shown: Edge = players && kind === "secret" ? "wall" : kind;
        return <line key={key} {...line} className={`map-edge ${shown}`} />;
      })}
      {map.labels.filter((l) => !(players && l.secret) && labelVisible(l.c, l.r)).map((l) => {
        const p = labelAt(l.c, l.r);
        return <text key={l.id} x={p.x} y={p.y} className={`map-label ${l.secret ? "secret" : ""}`} fontSize={l.size} fill={l.color} data-item={`label:${l.id}`}>{l.text}</text>;
      })}
      {map.markers.filter((m) => !(players && m.secret) && visible(m.c, m.r)).map((m) => {
        const p = cellCenter(map, m.c, m.r);
        return (
          <g key={m.id} className={`map-marker ${m.secret ? "secret" : ""}`} data-item={`marker:${m.id}`}>
            <text x={p.x} y={p.y} fontSize={size * 0.6}>{m.icon}</text>
            {m.label && <text x={p.x} y={p.y + size * 0.55} className="map-marker-label" fontSize={Math.max(10, size * 0.28)}>{m.label}</text>}
            <title>{[m.label, players ? "" : m.note].filter(Boolean).join(": ")}</title>
          </g>
        );
      })}
      <g>{fog}</g>
      {map.tokens.map((t) => {
        const p = cellCenter(map, t.c, t.r);
        return (
          <g key={t.id} className="map-token" data-item={`token:${t.id}`}>
            <circle cx={p.x} cy={p.y} r={size * 0.38} fill={t.color} />
            <text x={p.x} y={p.y} fontSize={size * 0.34}>{initials(t.name)}</text>
            <title>{t.name}</title>
          </g>
        );
      })}
      {/* The grid lines last, so they show over the fills. */}
      {type === "square" && <GridLines cols={cols} rows={rows} size={size} />}
      {type === "hex" && Array.from({ length: rows }, (_, r) => Array.from({ length: cols }, (_, c) => (
        <polygon key={`g${c},${r}`} points={hexPoints(map, c, r)} className="map-grid" />
      )))}
    </svg>
  );
}

function GridLines({ cols, rows, size }: { cols: number; rows: number; size: number }) {
  const lines: ReactNode[] = [];
  for (let c = 0; c <= cols; c++) lines.push(<line key={`v${c}`} x1={c * size} y1={0} x2={c * size} y2={rows * size} className="map-grid" />);
  for (let r = 0; r <= rows; r++) lines.push(<line key={`h${r}`} x1={0} y1={r * size} x2={cols * size} y2={r * size} className="map-grid" />);
  return <g>{lines}</g>;
}

const initials = (name: unknown) => String(name ?? "").trim().split(/\s+/).map((w) => w[0] ?? "").join("").slice(0, 2).toUpperCase() || "?";

function MapLegend({ map }: { map: MapData }) {
  if (!map.show_legend || !map.legend.length) return null;
  return (
    <ul className="map-legend">
      {map.legend.map((l) => <li key={l.id}><span className="swatch" style={{ background: l.color }} />{l.label}</li>)}
    </ul>
  );
}

/** A map you can pan (drag) and zoom (wheel), mirroring the DM's view on the players' screen. */
function MapStage({ map, view, onView, players = false, children, tool }: {
  map: MapData;
  view: MapViewState;
  onView?: (v: MapViewState) => void;
  players?: boolean;
  /** Pointer handlers for the editor's tools. `down` says whether it used the press; if not, dragging pans. */
  tool?: { down: (p: Point, e: ReactPointerEvent) => boolean; move: (p: Point, e: ReactPointerEvent) => void; up: () => void; double?: (p: Point) => void };
  children?: ReactNode;
}) {
  const svg = useRef<SVGSVGElement>(null);
  const drag = useRef<{ x: number; y: number } | null>(null);
  const { width, height } = mapSize(map);
  // A point on the page, in the map's own px, and what's drawn there (found by position: while
  // a drag holds the pointer, events name the map rather than the token or marker under it).
  const toMap = (e: { clientX: number; clientY: number }): Point => {
    const rect = svg.current!.getBoundingClientRect();
    return {
      x: ((e.clientX - rect.left) / rect.width) * width,
      y: ((e.clientY - rect.top) / rect.height) * height,
      target: document.elementFromPoint(e.clientX, e.clientY) ?? undefined,
    };
  };
  const onWheel = (e: WheelEvent<HTMLDivElement>) => {
    onView?.(zoomAround(view, e, e.currentTarget.getBoundingClientRect(), [0.15, 6]));
  };
  // The middle and right buttons always pan; the left one does whatever the tool does, or pans.
  const panning = (e: ReactPointerEvent) => !tool || e.button !== 0 || !tool.down(toMap(e), e);
  return (
    <div className="map-stage" onWheel={onWheel} onContextMenu={(e) => e.preventDefault()}>
      <MapSvg map={map} players={players} svgRef={svg}
        style={{ transform: `translate(${view.x}px, ${view.y}px) scale(${view.zoom})` }}
        onPointerDown={(e) => {
          e.currentTarget.setPointerCapture(e.pointerId);
          if (panning(e)) drag.current = { x: e.clientX - view.x, y: e.clientY - view.y };
        }}
        onPointerMove={(e) => {
          if (drag.current) onView?.({ ...view, x: e.clientX - drag.current.x, y: e.clientY - drag.current.y });
          else if (e.buttons) tool?.move(toMap(e), e);
        }}
        onPointerUp={() => {
          drag.current = null;
          tool?.up();
        }}
        onDoubleClick={(e) => tool?.double?.(toMap(e))} />
      <MapLegend map={map} />
      {children}
    </div>
  );
}

export interface Point { x: number; y: number; target?: Element }

/** The size that fits the whole map in a box. */
function fitView(map: MapData, box: { width: number; height: number }): MapViewState {
  const { width, height } = mapSize(map);
  return { zoom: Math.max(0.15, Math.min(4, Math.min(box.width / width, box.height / height) * 0.95)), x: 0, y: 0 };
}

// ------------------------------------------------------------------ editor

type Tool = "move" | "paint" | "fill" | "erase" | "wall" | "text" | "marker" | "token" | "fog";
const TOOLS: [Tool, string, string][] = [
  ["move", "✋", "Move: drag the map, tokens, markers and text; double-click one to edit it"],
  ["paint", "🖌", "Paint cells with the chosen legend colour"],
  ["fill", "🪣", "Fill an area of the same colour (stops at walls)"],
  ["erase", "🧽", "Erase: colours, walls, text, markers or tokens"],
  ["wall", "▬", "Walls and doors on the lines between cells (square grids)"],
  ["text", "T", "Text: click to place a label"],
  ["marker", "📍", "Markers: click to drop the chosen symbol"],
  ["token", "●", "Tokens: click to place one for the party or anyone else"],
  ["fog", "☁", "Fog: reveal or hide cells for the players"],
];

export function MapEditor({ name, map, onChange, view, onView, characters }: {
  /** The map's title, for exported file names. */
  name: string;
  map: MapData;
  onChange: (next: MapData) => void;
  view: MapViewState;
  onView: (v: MapViewState) => void;
  characters: Character[];
}) {
  const { prompt, confirm } = useDialogs();
  const toast = useToast();
  const [tool, setTool] = useState<Tool>("paint");
  const [brush, setBrush] = useState(map.legend[0]?.id ?? "");
  const [edge, setEdge] = useState<Edge>("wall");
  const [icon, setIcon] = useState(MARKER_ICONS[0]);
  const [secret, setSecret] = useState(false);
  const [fogMode, setFogMode] = useState<"reveal" | "hide">("reveal");
  const [tokenFor, setTokenFor] = useState("");
  const [showGrid, setShowGrid] = useState(false);
  const stage = useRef<HTMLDivElement>(null);
  // While dragging: what's being moved, and the map as it was at the start (one edit per stroke).
  const moving = useRef<{ kind: string; id: string } | null>(null);
  const lastCell = useRef("");

  const set = (fn: (m: MapData) => void) => {
    const next = structuredClone(map);
    fn(next);
    onChange(next);
  };
  const cell = (p: Point) => cellAt(map, p.x, p.y);
  const itemAt = (p: Point) => {
    const el = p.target?.closest?.("[data-item]");
    const [kind, id] = (el?.getAttribute("data-item") ?? "").split(":");
    return kind ? { kind, id } : null;
  };

  const paintCell = (p: Point) => {
    const at = cell(p);
    if (!at) return;
    const key = cellKey(at.c, at.r);
    if (key === lastCell.current) return;
    lastCell.current = key;
    if (tool === "paint") set((m) => void (brush ? (m.cells[key] = brush) : delete m.cells[key]));
    if (tool === "fog") set((m) => {
      const shown = new Set(m.fog.revealed);
      if (fogMode === "reveal") shown.add(key);
      else shown.delete(key);
      m.fog.revealed = [...shown];
    });
    if (tool === "erase") set((m) => void delete m.cells[key]);
  };

  const down = async (p: Point) => {
    lastCell.current = "";
    const item = itemAt(p);
    const at = cell(p);
    switch (tool) {
      case "move":
        return;
      case "paint":
      case "fog":
        return paintCell(p);
      case "erase": {
        if (item) {
          return set((m) => {
            if (item.kind === "token") m.tokens = m.tokens.filter((t) => t.id !== item.id);
            if (item.kind === "marker") m.markers = m.markers.filter((t) => t.id !== item.id);
            if (item.kind === "label") m.labels = m.labels.filter((t) => t.id !== item.id);
          });
        }
        const e = edgeAt(map, p.x, p.y);
        if (e && map.walls[e] && nearLine(map, p, e)) return set((m) => void delete m.walls[e]);
        return paintCell(p);
      }
      case "fill":
        if (!at) return;
        return set((m) => {
          for (const key of floodFill(m, at.c, at.r)) {
            if (brush) m.cells[key] = brush;
            else delete m.cells[key];
          }
        });
      case "wall": {
        if (map.grid.type !== "square") return toast("Walls and doors are for square grids; on hexes, paint walls as cells", "error");
        const e = edgeAt(map, p.x, p.y);
        if (!e) return;
        lastCell.current = e;
        return set((m) => {
          if (m.walls[e] === edge) delete m.walls[e];
          else m.walls[e] = edge;
        });
      }
      case "text": {
        if (!at) return;
        const text = await prompt("Text for the map", { label: "Place" });
        if (!text?.trim()) return;
        return set((m) => void m.labels.push({ id: newId(), c: p.x / m.grid.size, r: p.y / m.grid.size, text: text.trim(), size: Math.round(m.grid.size * 0.45), color: "#2a241e", secret }));
      }
      case "marker":
        if (!at) return;
        return set((m) => void m.markers.push({ id: newId(), c: at.c, r: at.r, icon, label: "", note: "", secret }));
      case "token": {
        if (!at) return;
        const character = characters.find((c) => c.id === tokenFor);
        const name = character?.name ?? (await prompt("Who is this token?", { label: "Place" }))?.trim();
        if (!name) return;
        const color = TOKEN_COLORS[map.tokens.length % TOKEN_COLORS.length];
        return set((m) => void m.tokens.push({ id: newId(), c: at.c, r: at.r, name, color, character_id: character?.id ?? "" }));
      }
    }
  };

  const move = (p: Point) => {
    if (tool === "paint" || tool === "fog" || (tool === "erase" && !itemAt(p))) return paintCell(p);
    if (tool === "wall") {
      const e = edgeAt(map, p.x, p.y);
      if (e && e !== lastCell.current) {
        lastCell.current = e;
        set((m) => void (m.walls[e] = edge));
      }
      return;
    }
    if (tool === "move" && moving.current) {
      const at = cell(p);
      if (!at) return;
      const { kind, id } = moving.current;
      set((m) => {
        if (kind === "token") Object.assign(m.tokens.find((t) => t.id === id) ?? {}, at);
        if (kind === "marker") Object.assign(m.markers.find((t) => t.id === id) ?? {}, at);
        if (kind === "label") Object.assign(m.labels.find((t) => t.id === id) ?? {}, { c: p.x / m.grid.size, r: p.y / m.grid.size });
      });
    }
  };

  const double = async (p: Point) => {
    const item = itemAt(p);
    if (!item) return;
    if (item.kind === "label") {
      const label = map.labels.find((l) => l.id === item.id);
      const text = await prompt("Text", { label: "Save", value: label?.text });
      if (text !== null) set((m) => void Object.assign(m.labels.find((l) => l.id === item.id) ?? {}, { text }));
    }
    if (item.kind === "marker") {
      const marker = map.markers.find((l) => l.id === item.id);
      const label = await prompt("Marker name (shown under it)", { label: "Next", value: marker?.label });
      if (label === null) return;
      const note = await prompt("DM note (hover to read; players never see it)", { label: "Save", value: marker?.note });
      set((m) => void Object.assign(m.markers.find((l) => l.id === item.id) ?? {}, { label, note: note ?? marker?.note ?? "" }));
    }
    if (item.kind === "token") {
      const token = map.tokens.find((t) => t.id === item.id);
      const name = await prompt("Token name", { label: "Save", value: token?.name });
      if (name) set((m) => void Object.assign(m.tokens.find((t) => t.id === item.id) ?? {}, { name }));
    }
  };

  const fit = () => {
    const box = stage.current?.getBoundingClientRect();
    if (box) onView(fitView(map, box));
  };
  const exportAs = async (format: string) => {
    const file = slug(name, "map");
    try {
      if (format === "png-players") download(`${file}-players.png`, (await mapPng(map, true)).blob);
      if (format === "png-dm") download(`${file}-dm.png`, (await mapPng(map, false)).blob);
      if (format === "uvtt") {
        if (map.grid.type === "hex") toast("Universal VTT grids are square: the image is exported, but walls and doors only exist on square grids");
        download(`${file}.dd2vtt`, JSON.stringify(await universalVtt(map)), "application/json");
      }
      if (format === "svg") download(`${file}.svg`, mapSvgText(map, false), "image/svg+xml");
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };

  // A map opens fitted to the space, unless it's already been zoomed or moved.
  useEffect(() => {
    if (view.zoom === 1 && view.x === 0 && view.y === 0) fit();
  }, []);

  const party = characters.filter((c) => !map.tokens.some((t) => t.character_id === c.id));

  return (
    <div className="map-editor">
      <div className="map-toolbar row gap wrap" role="toolbar" aria-label="Map tools">
        {TOOLS.map(([t, symbol, title]) => (
          <button type="button" key={t} className={`tool-btn ${tool === t ? "on" : ""}`} title={title} aria-label={title} aria-pressed={tool === t}
            onClick={() => setTool(t)}>{symbol}</button>
        ))}
        <span className="grow" />
        <button type="button" className="btn small" onClick={fit}>Fit</button>
        <button type="button" className={`btn small ${showGrid ? "primary" : ""}`} onClick={() => setShowGrid(!showGrid)}>Grid…</button>
        <Select className="small-select" aria-label="Export the map" value="" onChange={(v) => v && void exportAs(v)} options={[
          ["", "⬇ Export…"],
          ["png-players", "PNG: what the players see (fog, no DM notes)"],
          ["png-dm", "PNG: the DM's view"],
          ["svg", "SVG (the DM's view)"],
          ["uvtt", "Universal VTT (.dd2vtt): walls and doors for Foundry and other VTTs"],
        ]} />
      </div>

      <div className="map-options row gap wrap">
        {(tool === "paint" || tool === "fill") && (
          <div className="row gap-sm wrap" role="radiogroup" aria-label="Paint with">
            {map.legend.map((l) => (
              <button type="button" key={l.id} role="radio" aria-checked={brush === l.id} className={`swatch-btn ${brush === l.id ? "on" : ""}`} onClick={() => setBrush(l.id)}>
                <span className="swatch" style={{ background: l.color }} />{l.label}
              </button>
            ))}
            <button type="button" role="radio" aria-checked={brush === ""} className={`swatch-btn ${brush === "" ? "on" : ""}`} onClick={() => setBrush("")}>
              <span className="swatch none" />Clear
            </button>
          </div>
        )}
        {tool === "wall" && (
          <div className="row gap-sm" role="radiogroup" aria-label="Edge">
            {([["wall", "Wall"], ["door", "Door"], ["secret", "Secret door"]] as [Edge, string][]).map(([k, label]) => (
              <button type="button" key={k} role="radio" aria-checked={edge === k} className={`chip-btn ${edge === k ? "active" : ""}`} onClick={() => setEdge(k)}>{label}</button>
            ))}
            <span className="muted small">Click or drag along the lines between cells; click one again to remove it. Players see secret doors as walls.</span>
          </div>
        )}
        {tool === "marker" && (
          <div className="row gap-sm wrap">
            {MARKER_ICONS.map((m) => (
              <button type="button" key={m} className={`icon-pick ${icon === m ? "on" : ""}`} aria-pressed={icon === m} onClick={() => setIcon(m)}>{m}</button>
            ))}
          </div>
        )}
        {(tool === "marker" || tool === "text") && (
          <label className="check small"><Checkbox checked={secret} onChange={setSecret} /> DM only (players don't see it)</label>
        )}
        {tool === "token" && (
          <label className="row gap-sm small">Token for
            <Select value={tokenFor} onChange={setTokenFor}
              options={[["", "Someone else (you'll name it)"], ...party.map((c): Option => [c.id, c.name])]} />
          </label>
        )}
        {tool === "fog" && (
          <div className="row gap-sm wrap">
            <label className="check small"><Checkbox checked={map.fog.on} onChange={(on) => set((m) => void (m.fog.on = on))} /> Fog on the players' screen</label>
            {(["reveal", "hide"] as const).map((k) => (
              <button type="button" key={k} className={`chip-btn ${fogMode === k ? "active" : ""}`} onClick={() => setFogMode(k)}>{k === "reveal" ? "Reveal" : "Hide"}</button>
            ))}
            <button type="button" className="btn small" onClick={() => set((m) => void (m.fog.revealed = []))}>Hide all</button>
            <button type="button" className="btn small" onClick={() => set((m) => void (m.fog.revealed = Array.from({ length: m.grid.rows }, (_, r) => Array.from({ length: m.grid.cols }, (_, c) => cellKey(c, r))).flat()))}>Reveal all</button>
          </div>
        )}
        {tool === "move" && <span className="muted small">Drag tokens, markers and text; drag empty space to pan; double-click one to rename it. The wheel zooms.</span>}
      </div>

      {showGrid && <GridSettings map={map} onChange={onChange} confirm={confirm} />}

      <div ref={stage}>
        <MapStage map={map} view={view} onView={onView}
          tool={{
            // The move tool grabs a token, marker or label under the pointer, and otherwise pans.
            down: (p) => {
              if (tool !== "move") {
                void down(p);
                return true;
              }
              moving.current = itemAt(p);
              return !!moving.current;
            },
            move, up: () => (moving.current = null), double: (p) => void double(p),
          }} />
      </div>

      <LegendEditor map={map} set={set} />
    </div>
  );
}

/** Whether a point is close to an edge's line (for erasing walls rather than the cell). */
function nearLine(map: MapData, p: Point, key: string) {
  const l = edgeLine(map, key);
  const d = l.x1 === l.x2 ? Math.abs(p.x - l.x1) : Math.abs(p.y - l.y1);
  return d < map.grid.size * 0.2;
}

function GridSettings({ map, onChange, confirm }: { map: MapData; onChange: (m: MapData) => void; confirm: (m: string, o?: { label?: string; danger?: boolean }) => Promise<boolean> }) {
  const resize = async (patch: Partial<MapData["grid"]>) => {
    const next = { ...map, grid: { ...map.grid, ...patch } };
    const shrinks = (patch.cols ?? map.grid.cols) < map.grid.cols || (patch.rows ?? map.grid.rows) < map.grid.rows;
    if (shrinks && !(await confirm("Make the grid smaller? Anything outside the new size is removed.", { label: "Shrink" }))) return;
    onChange(shrinks ? cropTo(next) : next);
  };
  const changeType = async (type: GridType) => {
    if (type === map.grid.type) return;
    const walls = Object.keys(map.walls).length;
    if (type === "hex" && walls && !(await confirm(`Switch to hexes? The ${walls} walls and doors are removed (they're for square grids).`, { label: "Switch" }))) return;
    onChange({ ...map, grid: { ...map.grid, type }, walls: type === "hex" ? {} : map.walls });
  };
  return (
    <div className="map-grid-settings row gap wrap">
      <label className="field"><span>Grid</span>
        <Select value={map.grid.type} options={[["square", "Squares"], ["hex", "Hexes"]]} onChange={(v) => void changeType(v as GridType)} />
      </label>
      <label className="field"><span>Columns</span><NumInput commit min={1} max={120} value={map.grid.cols} onChange={(v) => void resize({ cols: Math.max(1, Math.min(120, v)) })} /></label>
      <label className="field"><span>Rows</span><NumInput commit min={1} max={120} value={map.grid.rows} onChange={(v) => void resize({ rows: Math.max(1, Math.min(120, v)) })} /></label>
      <label className="field"><span>Cell size (px)</span><NumInput commit min={12} max={120} value={map.grid.size} onChange={(v) => onChange({ ...map, grid: { ...map.grid, size: Math.max(12, Math.min(120, v)) } })} /></label>
      <label className="field"><span>Background</span><input type="color" value={map.background} onChange={(e) => onChange({ ...map, background: e.target.value })} /></label>
      <label className="check small"><Checkbox checked={map.show_legend} onChange={(v) => onChange({ ...map, show_legend: v })} /> Show the legend</label>
      <label className="check small"><Checkbox checked={map.players_move} onChange={(v) => onChange({ ...map, players_move: v })} /> Players can move their tokens on their screen</label>
    </div>
  );
}

function LegendEditor({ map, set }: { map: MapData; set: (fn: (m: MapData) => void) => void }) {
  const used = new Map<string, number>();
  for (const id of Object.values(map.cells)) used.set(id, (used.get(id) ?? 0) + 1);
  return (
    <section className="doc-section map-legend-editor">
      <div className="eyebrow row gap">Legend
        <button type="button" className="link small" onClick={() => set((m) => void m.legend.push({ id: newId(), label: "New", color: "#a0a0a0" }))}>+ Colour</button>
      </div>
      <ul className="list">
        {map.legend.map((l, i) => (
          <li key={l.id} className="row gap">
            <input type="color" aria-label={`${l.label} colour`} value={l.color} onChange={(e) => set((m) => void (m.legend[i].color = e.target.value))} />
            <TextInput className="grow" aria-label="Legend label" value={l.label} onChange={(v) => set((m) => void (m.legend[i].label = v))} />
            <span className="muted small">{used.get(l.id) ?? 0} cells</span>
            <button type="button" className="icon-btn" aria-label={`Remove ${l.label}`} title="Remove (its cells become blank)"
              onClick={() => set((m) => {
                m.legend.splice(i, 1);
                for (const [k, v] of Object.entries(m.cells)) if (v === l.id) delete m.cells[k];
              })}>✕</button>
          </li>
        ))}
      </ul>
    </section>
  );
}

// ------------------------------------------------------------------ players moving tokens

/** On the players' screen: drag tokens (when the DM allows it); saved through the server. */
export function PlayersMap({ handoutId, map, view }: { handoutId: string; map: MapData; view: MapViewState }) {
  const [tokens, setTokens] = useState<MapToken[]>(map.tokens);
  const moving = useRef<string | null>(null);
  useEffect(() => setTokens(map.tokens), [map.tokens]);
  const shown = { ...map, tokens };
  const tool = map.players_move ? {
    down: (p: Point) => {
      const el = p.target?.closest?.("[data-item^='token:']");
      moving.current = el?.getAttribute("data-item")?.slice(6) ?? null;
      return !!moving.current;
    },
    move: (p: Point) => {
      const at = moving.current ? cellAt(map, p.x, p.y) : null;
      if (at) setTokens((ts) => ts.map((t) => (t.id === moving.current ? { ...t, ...at } : t)));
    },
    up: () => {
      const moved = tokens.find((t) => t.id === moving.current);
      if (moved) void api.moveTokens(handoutId, [{ id: moved.id, c: moved.c, r: moved.r }]).catch(() => {});
      moving.current = null;
    },
  } : undefined;
  return <MapStage map={shown} view={view} players tool={tool} />;
}



/**
 * While the DM has a map open that players may move tokens on, pick up their moves (made on
 * the players' screen and saved to the server) so the DM's next save doesn't undo them.
 */
export function MapTokensFromPlayers({ id, map, onTokens, children }: { id: string; map: MapData; onTokens: (tokens: MapToken[]) => void; children: ReactNode }) {
  const seen = useRef(JSON.stringify(map.tokens));
  const latest = useRef({ map, onTokens });
  latest.current = { map, onTokens };
  useEffect(() => {
    if (!map.players_move) return;
    const timer = setInterval(async () => {
      try {
        const saved = await api.get("handouts", id);
        const tokens = ((saved.map as unknown as MapData)?.tokens ?? []) as MapToken[];
        const text = JSON.stringify(tokens);
        // Only a change made elsewhere: not one this editor just saved.
        if (text !== seen.current && text !== JSON.stringify(latest.current.map.tokens)) latest.current.onTokens(tokens);
        seen.current = text;
      } catch {
        /* the next poll tries again */
      }
    }, 2000);
    return () => clearInterval(timer);
  }, [id, map.players_move]);
  return <>{children}</>;
}

// ------------------------------------------------------------------ export

/** The map drawn as a standalone SVG file, styles included. */
function mapSvgText(map: MapData, players: boolean): string {
  const holder = document.createElement("div");
  const root = createRoot(holder);
  flushSync(() => root.render(<MapSvg map={map} players={players} />));
  const svg = holder.querySelector("svg")!;
  svg.setAttribute("xmlns", "http://www.w3.org/2000/svg");
  const text = svg.outerHTML;
  root.unmount();
  return text;
}

/** The map as a PNG: twice the on-screen size, as long as that stays within what browsers can draw. */
async function mapPng(map: MapData, players: boolean): Promise<{ blob: Blob; scale: number }> {
  const { width, height } = mapSize(map);
  const scale = Math.max(0.25, Math.min(2, 8192 / Math.max(width, height)));
  const url = URL.createObjectURL(new Blob([mapSvgText(map, players)], { type: "image/svg+xml" }));
  try {
    const image = new Image();
    await new Promise<void>((done, fail) => {
      image.onload = () => done();
      image.onerror = () => fail(new Error("The map couldn't be drawn as an image"));
      image.src = url;
    });
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(width * scale);
    canvas.height = Math.round(height * scale);
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("This browser can't draw images");
    ctx.drawImage(image, 0, 0, canvas.width, canvas.height);
    const blob = await new Promise<Blob>((done, fail) => canvas.toBlob((b) => (b ? done(b) : fail(new Error("The map couldn't be saved as a PNG"))), "image/png"));
    return { blob, scale };
  } finally {
    URL.revokeObjectURL(url);
  }
}

/**
 * The map as a Universal VTT file (.dd2vtt), the format Dungeondraft exports and Foundry's
 * Universal Battlemap Importer, Arkenforge and others read: the picture, plus walls and doors
 * that block line of sight. Positions are in grid squares. The picture is what the players
 * see, without fog or tokens (the VTT has its own); secret doors go out as walls.
 */
async function universalVtt(map: MapData) {
  const picture = { ...map, fog: { ...map.fog, on: false }, tokens: [] };
  const { blob, scale } = await mapPng(picture, true);
  const { width, height } = mapSize(map);
  const size = map.grid.size;
  const { walls, portals } = vttWalls(map);
  return {
    format: 0.2,
    software: "Questodian",
    resolution: {
      map_origin: { x: 0, y: 0 },
      map_size: { x: width / size, y: height / size },
      pixels_per_grid: Math.round(size * scale),
    },
    line_of_sight: walls,
    objects_line_of_sight: [],
    portals,
    environment: { baked_lighting: true, ambient_light: "ffffffff" },
    lights: [],
    image: await base64(blob),
  };
}

/** A file's bytes as plain base64 (no "data:" prefix). */
function base64(blob: Blob): Promise<string> {
  return new Promise((done, fail) => {
    const reader = new FileReader();
    reader.onload = () => done(String(reader.result).split(",", 2)[1] ?? "");
    reader.onerror = () => fail(new Error("The map's picture couldn't be read"));
    reader.readAsDataURL(blob);
  });
}
