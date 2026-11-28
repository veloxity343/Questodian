import { describe, expect, it } from "vitest";
import { backupsIn, backupsToRemove } from "./backups";

/** Backups for each day from `start`, going back `days` days. */
function daily(start: string, days: number): string[] {
  const out = [];
  const d = new Date(`${start}T00:00:00Z`);
  for (let i = 0; i < days; i++) {
    out.push(`backup-${d.toISOString().slice(0, 10)}.json.gz`);
    d.setUTCDate(d.getUTCDate() - 1);
  }
  return out;
}

describe("cloud backups", () => {
  it("finds backups among the folder's files", () => {
    const found = backupsIn(["changes-abc-000000001.json", "backup-2026-11-28-093000.json.gz", "image-ff.png", "backup-2026-11-27-101500-before-restore.json"]);
    expect(found.map((b) => b.date)).toEqual(["2026-11-28", "2026-11-27"]);
    expect(found[0].made?.toISOString()).toBe("2026-11-28T09:30:00.000Z");
  });

  it("keeps the last 14 days and one a week for 8 weeks before", () => {
    const names = daily("2026-11-28", 120);
    const remove = new Set(backupsToRemove(backupsIn(names)));
    const kept = names.filter((n) => !remove.has(n));
    expect(kept.slice(0, 14)).toEqual(names.slice(0, 14));
    expect(kept.length).toBe(14 + 8);
    // The weekly ones are a week apart.
    const weekly = kept.slice(14).map((n) => Date.parse(n.slice(7, 17)));
    for (let i = 1; i < weekly.length; i++) expect(weekly[i - 1] - weekly[i]).toBe(7 * 86400000);
  });

  it("keeps a backup made before a restore, and removes nothing when there are few", () => {
    const names = ["backup-2026-11-28-090000.json.gz", "backup-2026-11-28-120000-before-restore.json.gz", "backup-2026-11-20-090000.json.gz"];
    expect(backupsToRemove(backupsIn(names))).toEqual([]);
  });
});
