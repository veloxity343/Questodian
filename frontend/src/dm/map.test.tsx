import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { blankMap, type MapData } from "../lib/map";
import { MapSvg } from "./map";

/** A 6 × 4 map with a room in the revealed half and things hidden in the fogged half. */
function fogged(): MapData {
  const m = blankMap();
  m.grid.cols = 6;
  m.grid.rows = 4;
  m.fog = { on: true, revealed: ["0,0", "1,0", "0,1", "1,1"] };
  m.walls = { "1,0,w": "secret", "4,2,w": "wall" };
  m.tokens = [
    { id: "pip", c: 0, r: 0, name: "Pip", color: "#e5574e", character_id: "pip" },
    { id: "ogre", c: 5, r: 3, name: "Ogre", color: "#3aa7d9" },
  ];
  m.markers = [
    { id: "chest", c: 1, r: 1, icon: "💰", label: "Chest", note: "" },
    { id: "trap", c: 0, r: 1, icon: "⚠️", label: "Trap", note: "", secret: true },
  ];
  m.labels = [{ id: "far", c: 5.5, r: 3.5, text: "Lair", size: 18, color: "#000" }];
  return m;
}

const draw = (map: MapData, players: boolean) => renderToStaticMarkup(<MapSvg map={map} players={players} />);

describe("the players' screen", () => {
  const players = draw(fogged(), true);
  const dm = draw(fogged(), false);

  it("hides what's under fog: tokens, markers, text and walls", () => {
    expect(players).toContain("token:pip");
    expect(players).not.toContain("token:ogre");
    expect(players).not.toContain("Lair");
    expect(dm).toContain("token:ogre");
    expect(dm).toContain("Lair");
    expect((players.match(/fog solid/g) ?? []).length).toBe(20);
  });

  it("hides DM-only markers and shows secret doors as walls", () => {
    expect(players).toContain("Chest");
    expect(players).not.toContain("Trap");
    expect(players).toContain("map-edge wall");
    expect(players).not.toContain("map-edge secret");
    expect(dm).toContain("map-edge secret");
  });
});
