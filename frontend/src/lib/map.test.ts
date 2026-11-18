import { describe, expect, it } from "vitest";
import { along, blankMap, cellAt, cellCenter, edgeAt, edgeOn, floodFill, vttWalls } from "./map";

describe("grids", () => {
  it.each(["square", "hex"] as const)("finds every %s cell from its centre and near its edge", (type) => {
    const m = blankMap(type);
    for (let r = 0; r < m.grid.rows; r++) {
      for (let c = 0; c < m.grid.cols; c++) {
        const p = cellCenter(m, c, r);
        expect(cellAt(m, p.x, p.y)).toEqual({ c, r });
        expect(cellAt(m, p.x + m.grid.size * 0.3, p.y + m.grid.size * 0.2)).toEqual({ c, r });
      }
    }
    expect(cellAt(m, -5, -5)).toBeNull();
  });

  it("picks the nearest edge on a square grid", () => {
    const m = blankMap();
    expect(edgeAt(m, 120, 60)).toBe("3,1,w");
    expect(edgeAt(m, 60, 79)).toBe("1,2,n");
  });
});

describe("walls", () => {
  const room = () => {
    const m = blankMap();
    m.grid.cols = 6;
    m.grid.rows = 4;
    return m;
  };

  it("stop a flood fill", () => {
    const m = room();
    for (let r = 0; r < 4; r++) m.walls[`3,${r},w`] = "wall";
    expect(floodFill(m, 0, 0)).toHaveLength(12);
  });

  it("bound a fill up to the walls, whatever the colours inside", () => {
    const m = room();
    m.cells = { "0,0": "water", "1,1": "grass", "4,0": "stone" };
    expect(floodFill(m, 0, 0)).toEqual(["0,0"]);
    expect(floodFill(m, 0, 0, false)).toHaveLength(24);
    for (let r = 0; r < 4; r++) m.walls[`3,${r},w`] = "wall";
    expect(floodFill(m, 1, 1, false).sort()).toEqual(["0,0", "0,1", "0,2", "0,3", "1,0", "1,1", "1,2", "1,3", "2,0", "2,1", "2,2", "2,3"]);
  });

  it("go to a VTT as straight lines and doors, with secret doors as walls", () => {
    const m = room();
    m.walls = { "1,1,n": "wall", "2,1,n": "door", "3,1,n": "wall", "1,1,w": "secret", "1,2,w": "wall" };
    const { walls, portals } = vttWalls(m);
    expect(walls).toEqual([
      [{ x: 1, y: 1 }, { x: 2, y: 1 }],
      [{ x: 3, y: 1 }, { x: 4, y: 1 }],
      [{ x: 1, y: 1 }, { x: 1, y: 3 }],
    ]);
    expect(portals).toEqual([{ position: { x: 2.5, y: 1 }, bounds: [{ x: 2, y: 1 }, { x: 3, y: 1 }], rotation: 0, closed: true, freestanding: false }]);
    expect(vttWalls(blankMap("hex"))).toEqual({ walls: [], portals: [] });
  });
});

describe("strokes", () => {
  it("draw walls along their own direction, with no stubs at the corners they cross", () => {
    const m = blankMap();
    // Drawn across, a little below the line between rows 2 and 3, over five cells.
    const across = new Set(along({ x: 5, y: 84 }, { x: 195, y: 84 }, m.grid.size / 4).map((p) => edgeOn(m, p.x, p.y, "n")));
    expect([...across]).toEqual(["0,2,n", "1,2,n", "2,2,n", "3,2,n", "4,2,n"]);
    const down = new Set(along({ x: 41, y: 5 }, { x: 41, y: 115 }, m.grid.size / 4).map((p) => edgeOn(m, p.x, p.y, "w")));
    expect([...down]).toEqual(["1,0,w", "1,1,w", "1,2,w"]);
  });

  it("touch every cell between two pointer positions", () => {
    const m = blankMap();
    const cells = new Set(along({ x: 10, y: 10 }, { x: 410, y: 10 }, m.grid.size / 3).map((p) => cellAt(m, p.x, p.y)?.c));
    expect([...cells]).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
  });
});
