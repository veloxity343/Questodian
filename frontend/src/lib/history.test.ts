import { describe, expect, it } from "vitest";
import { emptyHistory, redo, remember, undo } from "./history";

describe("undo and redo", () => {
  it("steps back and forward through changes", () => {
    let h = remember(remember(emptyHistory<string>(), "a"), "b"); // a → b → c
    const back = undo(h, "c")!;
    expect(back.value).toBe("b");
    const back2 = undo(back.history, back.value)!;
    expect(back2.value).toBe("a");
    expect(undo(back2.history, back2.value)).toBeNull();
    const forward = redo(back2.history, back2.value)!;
    expect(forward.value).toBe("b");
    h = remember(forward.history, "b");
    expect(redo(h, "x")).toBeNull(); // a new change clears what could be redone
  });

  it("keeps the most recent changes only", () => {
    let h = emptyHistory<number>();
    for (let i = 0; i < 5; i++) h = remember(h, i, 3);
    expect(h.past).toEqual([2, 3, 4]);
  });
});
