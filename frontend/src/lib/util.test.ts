import { describe, expect, it } from "vitest";
import { placed } from "./util";

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
