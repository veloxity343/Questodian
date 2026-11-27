import { describe, expect, it } from "vitest";
import { isAppPath } from "../lib/desktop";
import { backupDue, daysSince } from "./backup";

describe("backups", () => {
  it("counts whole days since a backup", () => {
    expect(daysSince("")).toBeNull();
    expect(daysSince(new Date().toISOString())).toBe(0);
    expect(daysSince(new Date(Date.now() - 8.5 * 86_400_000).toISOString())).toBe(8);
  });

  it("only reminds in the web version", () => {
    expect(backupDue("", true)).toBe(false); // tests don't run the web build
  });
});

describe("the players' view's way back", () => {
  it("only leads to a page of the app", () => {
    expect(isAppPath("/")).toBe(true);
    expect(isAppPath("/characters?x=1")).toBe(true);
    expect(isAppPath("//evil.example")).toBe(false);
    expect(isAppPath("/\\evil.example")).toBe(false);
    expect(isAppPath("https://evil.example")).toBe(false);
    expect(isAppPath("")).toBe(false);
  });
});
