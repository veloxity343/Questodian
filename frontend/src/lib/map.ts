// Maps drawn in the app: the data a map handout keeps (`handout.map`), and the grid maths for
// square and hex grids. Drawing is in dm/map.tsx.

export type GridType = "square" | "hex";
export type Edge = "wall" | "door" | "secret";

export interface LegendEntry { id: string; label: string; color: string }
export interface MapLabel { id: string; c: number; r: number; text: string; size: number; color: string; secret?: boolean }
export interface MapMarker { id: string; c: number; r: number; icon: string; label: string; note: string; secret?: boolean }
export interface MapToken { id: string; c: number; r: number; name: string; color: string; character_id?: string }

export interface MapData {
  grid: { type: GridType; cols: number; rows: number; /** Cell width in px. */ size: number };
  background: string;
  legend: LegendEntry[];
  /** "col,row" → legend id. */
  cells: Record<string, string>;
  /** Square grids: "col,row,n" (top edge) or "col,row,w" (left edge) → what's there. */
  walls: Record<string, Edge>;
  labels: MapLabel[];
  markers: MapMarker[];
  tokens: MapToken[];
  /** With fog on, the players see only revealed cells. */
  fog: { on: boolean; revealed: string[] };
  /** Players can drag their tokens on the players' screen. */
  players_move: boolean;
  show_legend: boolean;
}

export const MARKER_ICONS = ["📍", "🚪", "🪜", "💰", "🗝️", "⚠️", "☠️", "🔥", "⛺", "🌲", "⛰️", "🏰", "🏠", "⛪", "⚓", "👤", "👹", "🐉", "❓", "⭐"];
export const TOKEN_COLORS = ["#e5574e", "#f0892a", "#e0c341", "#2fb877", "#3aa7d9", "#7a6cf0", "#d05ad0", "#dde2ea"];

export const newId = () => Math.random().toString(16).slice(2, 10);

/** A new map: a 24 × 18 square grid with a starter legend. */
export function blankMap(type: GridType = "square"): MapData {
  return {
    grid: { type, cols: 24, rows: 18, size: 40 },
    background: "#f1ead8",
    legend: [
      { id: "floor", label: "Floor", color: "#d9cfb6" },
      { id: "stone", label: "Wall / rock", color: "#5b5850" },
      { id: "water", label: "Water", color: "#4f8fc0" },
      { id: "grass", label: "Grass", color: "#7fae5a" },
      { id: "forest", label: "Forest", color: "#3e6b3a" },
      { id: "road", label: "Road", color: "#b08b5a" },
    ],
    cells: {},
    walls: {},
    labels: [],
    markers: [],
    tokens: [],
    fog: { on: false, revealed: [] },
    players_move: false,
    show_legend: true,
  };
}

/** Whether a handout is a drawn map (rather than an image). */
export const isMap = (map: unknown): map is MapData => !!map && typeof map === "object" && "grid" in (map as object);

/** A stored map with anything missing filled in. */
export function mapOf(stored: unknown): MapData {
  const base = blankMap();
  const m = (stored ?? {}) as Partial<MapData>;
  return {
    ...base,
    ...m,
    grid: { ...base.grid, ...m.grid },
    fog: { ...base.fog, ...m.fog },
    legend: m.legend ?? base.legend,
    cells: m.cells ?? {},
    walls: m.walls ?? {},
    labels: m.labels ?? [],
    markers: m.markers ?? [],
    tokens: m.tokens ?? [],
  };
}

export const cellKey = (c: number, r: number) => `${c},${r}`;

// ------------------------------------------------------------------ geometry

const SQRT3 = Math.sqrt(3);

/** Pointy-top hexes in odd-row offset: the radius that makes a hex `size` wide. */
const hexRadius = (size: number) => size / SQRT3;

/** The map's size in px. */
export function mapSize(m: MapData) {
  const { type, cols, rows, size } = m.grid;
  if (type === "square") return { width: cols * size, height: rows * size };
  const R = hexRadius(size);
  return { width: size * (cols + 0.5), height: R * (1.5 * rows + 0.5) };
}

/** A cell's centre in px. */
export function cellCenter(m: MapData, c: number, r: number) {
  const { type, size } = m.grid;
  if (type === "square") return { x: (c + 0.5) * size, y: (r + 0.5) * size };
  const R = hexRadius(size);
  return { x: size * (c + 0.5 + (r & 1) * 0.5), y: R + r * 1.5 * R };
}

/** The points of a hex cell, for an SVG polygon. */
export function hexPoints(m: MapData, c: number, r: number) {
  const { x, y } = cellCenter(m, c, r);
  const R = hexRadius(m.grid.size);
  return Array.from({ length: 6 }, (_, i) => {
    const a = (Math.PI / 180) * (60 * i - 30);
    return `${(x + R * Math.cos(a)).toFixed(2)},${(y + R * Math.sin(a)).toFixed(2)}`;
  }).join(" ");
}

/** The cell under a point in px, or null outside the grid. Fractional for labels and markers. */
export function cellAt(m: MapData, x: number, y: number): { c: number; r: number } | null {
  const { type, cols, rows, size } = m.grid;
  let c: number;
  let r: number;
  if (type === "square") {
    c = Math.floor(x / size);
    r = Math.floor(y / size);
  } else {
    const R = hexRadius(size);
    // Pixel to axial (pointy-top), round in cube coordinates, then axial to odd-r offset.
    const px = x - size / 2;
    const py = y - R;
    const q = ((SQRT3 / 3) * px - (1 / 3) * py) / R;
    const ar = ((2 / 3) * py) / R;
    let rx = Math.round(q);
    let rz = Math.round(ar);
    const ry = Math.round(-q - ar);
    const dx = Math.abs(rx - q), dy = Math.abs(ry - (-q - ar)), dz = Math.abs(rz - ar);
    if (dx > dy && dx > dz) rx = -ry - rz;
    else if (dy <= dz) rz = -rx - ry;
    r = rz;
    c = rx + (rz - (rz & 1)) / 2;
  }
  return c >= 0 && r >= 0 && c < cols && r < rows ? { c, r } : null;
}

/** The square-grid edge nearest a point: the top or left side of a cell. */
export function edgeAt(m: MapData, x: number, y: number): string | null {
  if (m.grid.type !== "square") return null;
  const s = m.grid.size;
  const fx = x / s, fy = y / s;
  const nearX = Math.round(fx), nearY = Math.round(fy);
  const dx = Math.abs(fx - nearX), dy = Math.abs(fy - nearY);
  // Closer to a vertical line (left edge of column nearX) or a horizontal one (top of row nearY).
  if (dx < dy) {
    const r = Math.floor(fy);
    return nearX >= 0 && nearX <= m.grid.cols && r >= 0 && r < m.grid.rows ? `${nearX},${r},w` : null;
  }
  const c = Math.floor(fx);
  return nearY >= 0 && nearY <= m.grid.rows && c >= 0 && c < m.grid.cols ? `${c},${nearY},n` : null;
}

/** The line of a square-grid edge, in px. */
export function edgeLine(m: MapData, key: string) {
  const [c, r, side] = key.split(",");
  const s = m.grid.size;
  const x = Number(c) * s, y = Number(r) * s;
  return side === "w" ? { x1: x, y1: y, x2: x, y2: y + s } : { x1: x, y1: y, x2: x + s, y2: y };
}

/** Neighbouring cells, for flood fill. */
function neighbours(m: MapData, c: number, r: number): [number, number][] {
  if (m.grid.type === "square") return [[c + 1, r], [c - 1, r], [c, r + 1], [c, r - 1]];
  const odd = r & 1;
  const dirs = odd
    ? [[1, 0], [-1, 0], [0, -1], [1, -1], [0, 1], [1, 1]]
    : [[1, 0], [-1, 0], [-1, -1], [0, -1], [-1, 1], [0, 1]];
  return dirs.map(([dc, dr]) => [c + dc, r + dr]);
}

/** Every cell connected to (c, r) with the same fill, stopping at walls on square grids. */
export function floodFill(m: MapData, c: number, r: number): string[] {
  const start = m.cells[cellKey(c, r)] ?? "";
  const seen = new Set([cellKey(c, r)]);
  const queue: [number, number][] = [[c, r]];
  const blocked = (a: [number, number], b: [number, number]) => {
    if (m.grid.type !== "square") return false;
    const [ac, ar] = a, [bc, br] = b;
    const key = bc > ac ? `${bc},${br},w` : bc < ac ? `${ac},${ar},w` : br > ar ? `${bc},${br},n` : `${ac},${ar},n`;
    return !!m.walls[key];
  };
  for (let i = 0; i < queue.length && seen.size < 20000; i++) {
    const here = queue[i];
    for (const next of neighbours(m, here[0], here[1])) {
      const [nc, nr] = next;
      const key = cellKey(nc, nr);
      if (nc < 0 || nr < 0 || nc >= m.grid.cols || nr >= m.grid.rows || seen.has(key)) continue;
      if ((m.cells[key] ?? "") !== start || blocked(here, next)) continue;
      seen.add(key);
      queue.push(next);
    }
  }
  return [...seen];
}

/** Drop what's outside the grid after it shrinks. */
export function cropTo(m: MapData): MapData {
  const inside = (c: number, r: number) => c < m.grid.cols && r < m.grid.rows;
  const keep = <T,>(o: Record<string, T>) =>
    Object.fromEntries(Object.entries(o).filter(([k]) => {
      const [c, r] = k.split(",").map(Number);
      return inside(c, r);
    }));
  return {
    ...m,
    cells: keep(m.cells),
    walls: keep(m.walls),
    labels: m.labels.filter((x) => inside(x.c, x.r)),
    markers: m.markers.filter((x) => inside(x.c, x.r)),
    tokens: m.tokens.filter((x) => inside(x.c, x.r)),
    fog: { ...m.fog, revealed: m.fog.revealed.filter((k) => inside(...(k.split(",").map(Number) as [number, number]))) },
  };
}

/**
 * Walls and doors for a Universal VTT file, in grid squares: edges in a straight line join
 * into one wall; doors become portals; secret doors stay walls (the players mustn't find them
 * in a VTT either). Square grids only.
 */
export function vttWalls(m: MapData) {
  type P = { x: number; y: number };
  const walls: P[][] = [];
  const portals: { position: P; bounds: [P, P]; rotation: number; closed: boolean; freestanding: boolean }[] = [];
  if (m.grid.type !== "square") return { walls, portals };
  const solid = (key: string) => !!m.walls[key] && m.walls[key] !== "door";
  for (let r = 0; r <= m.grid.rows; r++) {
    for (let c = 0; c < m.grid.cols; c++) {
      if (!solid(`${c},${r},n`) || solid(`${c - 1},${r},n`)) continue;
      let end = c;
      while (solid(`${end + 1},${r},n`)) end++;
      walls.push([{ x: c, y: r }, { x: end + 1, y: r }]);
    }
  }
  for (let c = 0; c <= m.grid.cols; c++) {
    for (let r = 0; r < m.grid.rows; r++) {
      if (!solid(`${c},${r},w`) || solid(`${c},${r - 1},w`)) continue;
      let end = r;
      while (solid(`${c},${end + 1},w`)) end++;
      walls.push([{ x: c, y: r }, { x: c, y: end + 1 }]);
    }
  }
  for (const [key, kind] of Object.entries(m.walls)) {
    if (kind !== "door") continue;
    const [c, r, side] = key.split(",");
    const a = { x: Number(c), y: Number(r) };
    const b = side === "w" ? { x: a.x, y: a.y + 1 } : { x: a.x + 1, y: a.y };
    portals.push({ position: { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 }, bounds: [a, b], rotation: side === "w" ? Math.PI / 2 : 0, closed: true, freestanding: false });
  }
  return { walls, portals };
}
