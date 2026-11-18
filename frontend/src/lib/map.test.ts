import { describe, expect, it } from "vitest";
import { blankMap, cellAt, cellCenter, edgeAt, floodFill, vttWalls } from "./map";

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
