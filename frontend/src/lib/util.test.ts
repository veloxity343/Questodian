import { describe, expect, it } from "vitest";
import { placed, zoomAround } from "./util";

describe("arranging panels", () => {
  it("moves a panel past its visible neighbour, wherever hidden ones sit", () => {
    // b is hidden: moving a down puts it after c, its next visible neighbour.
    expect(placed(["a", "b", "c"], "a", "c", true)).toEqual(["b", "c", "a"]);
    expect(placed(["a", "b", "c"], "c", "a", false)).toEqual(["c", "a", "b"]);
  });

  it("drops a dragged panel where the target was", () => {
    expect(placed(["a", "b", "c", "d"], "a", "c", true)).toEqual(["b", "c", "a", "d"]);
    expect(placed(["a", "b", "c", "d"], "d", "b", false)).toEqual(["a", "d", "b", "c"]);
    expect(placed(["a", "b"], "a", "gone", true)).toEqual(["a", "b"]);
  });
});

describe("zooming with the wheel", () => {
  const box = { left: 0, top: 0, width: 400, height: 300 } as DOMRect;
  const view = { zoom: 1, x: 0, y: 0 };
  const at = (deltaY: number, more: { deltaMode?: number; ctrlKey?: boolean } = {}) =>
    zoomAround(view, { clientX: 200, clientY: 150, deltaY, ...more }, box, [0.1, 10]).zoom;

  it("zooms 15% for one mouse-wheel notch, either way", () => {
    expect(at(-100)).toBeCloseTo(1.15);
    expect(at(100)).toBeCloseTo(1 / 1.15);
  });

  it("zooms a little for a touchpad's small scroll, and never more than a notch", () => {
    expect(at(-4)).toBeGreaterThan(1);
    expect(at(-4)).toBeLessThan(1.01);
    expect(at(-2000)).toBeCloseTo(1.15);
  });

  it("counts Firefox's line scrolls and a pinch (Ctrl) for more", () => {
    expect(at(-3, { deltaMode: 1 })).toBeCloseTo(at(-99));
    expect(at(-4, { ctrlKey: true })).toBeCloseTo(at(-32));
  });

  it("keeps the point under the pointer still and stays within the limits", () => {
    const moved = zoomAround({ zoom: 1, x: 0, y: 0 }, { clientX: 300, clientY: 150, deltaY: -100 }, box, [0.1, 10]);
    // The pointer is 100px right of centre: that point stays put on screen.
    expect(100 - moved.x).toBeCloseTo(100 * moved.zoom);
    expect(zoomAround({ zoom: 9.9, x: 0, y: 0 }, { clientX: 200, clientY: 150, deltaY: -100 }, box, [0.1, 10]).zoom).toBe(10);
  });
});
