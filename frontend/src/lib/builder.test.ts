import { describe, expect, it } from "vitest";
import { rules } from "../test/rules";
import type { Character } from "../types";
import {
  apply, buildOf, canStep, levelUp, pickBackground, resetScores, spellCounts, startBuild, startingGear, step, work, type Build,
} from "./builder";

const blank = {
  name: "Test", classes: [{ name: "Fighter", subclass: "", level: 1 }], race: "", background: "",
  abilities: { str: 10, dex: 10, con: 10, int: 10, wis: 10, cha: 10 }, hp: { max: 10, current: 10, temp: 0 },
  hit_dice: { total: "1d10", remaining: 1 }, proficiencies: { armor: "", weapons: "", tools: "", languages: "Common" },
  attacks: [], spellcasting: { ability: "", slots: {}, spells: [] }, resources: [], features: [], skills: {}, build: {},
} as unknown as Character;

/** The spreadsheet's example character: a Human Fighter 1, Soldier, with the Tough feat and the Defense style. */
const fighter: Build = Object.assign(buildOf(blank), {
  class: "Fighter", species: "Human", background: "Soldier", human_feat: "Tough", fighting_style: "Defense",
  base: { str: 15, dex: 13, con: 14, int: 8, wis: 12, cha: 10 }, background_bonus: { str: 2, con: 1 },
  skill_picks: ["perception", "survival", "insight"], armor: "Chain Mail", shield: true, weapons: ["Longsword"],
});

describe("ability scores", () => {
  it("spends exactly the point-buy budget, and greys out + when a step costs too much", () => {
    expect(work(fighter, rules).checks.points).toBe("27 of 27");
    expect(canStep(fighter, rules, "str", 1)).toBe(false);
    expect(canStep(fighter, rules, "int", -1)).toBe(false);
    expect(canStep({ ...fighter, base: { ...fighter.base, str: 14 } }, rules, "int", 1)).toBe(true);
  });

  it("deals the class's standard array and swaps with the next score", () => {
    const dealt = resetScores({ ...fighter, method: "standard_array" }, rules);
    expect(Object.values(dealt.base)).toEqual([15, 14, 13, 8, 10, 12]);
    expect(Object.values(step(dealt, rules, "str", -1).base).slice(0, 2)).toEqual([14, 15]);
  });

  it("picks background bonuses like radio buttons", () => {
    expect(pickBackground(fighter, rules, "dex", 1).background_bonus).toEqual({ str: 2, dex: 1 });
    expect(pickBackground(fighter, rules, "dex", 2).background_bonus).toEqual({ dex: 2, con: 1 });
    expect(pickBackground(fighter, rules, "str", 2).background_bonus).toEqual({ con: 1 });
    expect(pickBackground(fighter, rules, "wis", 1).background_bonus).toEqual(fighter.background_bonus);
  });
});

describe("working it out", () => {
  const s = work(fighter, rules);

  it("matches the spreadsheet", () => {
    expect(s.ac.value).toBe(19); // chain mail 16 + shield 2 + Defense 1
    expect(s.hp.value).toBe(14); // d10 10 + Con 2 + Tough 2
    expect(s.weapons[0]).toMatchObject({ attack: 5, damage: "1d8+3 Slashing" });
    expect(s.warnings).toEqual(["This armor gives Disadvantage on Stealth."]);
  });

  it("follows the Monk's Martial Arts and Unarmored Defense", () => {
    const monk = work({ ...fighter, class: "Monk", level: 5, armor: "None (unarmored)", shield: false, weapons: ["Dagger"], fighting_style: "",
      base: { str: 10, dex: 16, con: 12, int: 8, wis: 15, cha: 8 }, background_bonus: {} }, rules);
    expect(monk.ac.value).toBe(15);
    expect(monk.weapons[0].damage).toBe("1d8+3 Piercing");
    const armoured = work({ ...fighter, class: "Monk", level: 5, armor: "Leather Armor", weapons: ["Spear"], fighting_style: "" }, rules);
    expect(armoured.weapons[0].notes).toMatch(/^Uses Str/);
  });

  it("limits Expertise by class and level", () => {
    const rogue = { ...fighter, class: "Rogue", skill_picks: ["stealth", "acrobatics", "perception", "deception"] };
    expect(work({ ...rogue, level: 1, expertise: ["stealth", "acrobatics"] }, rules).expertise.ok).toBe(true);
    expect(work({ ...rogue, level: 5, expertise: ["stealth", "acrobatics", "perception"] }, rules).expertise.ok).toBe(false);
    expect(work({ ...fighter, expertise: ["athletics"] }, rules).expertise.ok).toBe(false);
  });
});

describe("the character", () => {
  it("keeps damage taken and your own rows through every change", () => {
    let ch = apply(blank, fighter, rules);
    ch.hp.current = 5;
    ch.features.push({ name: "Improved Critical", source: "Fighter (Champion) 3", description: "" });
    ch = apply(ch, levelUp(buildOf(ch), rules, null)!.build, rules);
    expect([ch.hp.current, ch.hp.max]).toEqual([15, 24]);
    expect(ch.features.some((f) => f.name === "Improved Critical")).toBe(true);
    expect(JSON.stringify(apply(ch, buildOf(ch), rules))).toBe(JSON.stringify(ch));
  });

  it("leaves a sheet alone when its class isn't in the rules data", () => {
    const ch = apply(blank, fighter, rules);
    expect(apply(ch, buildOf(ch), { ...rules, classes: [] })).toBe(ch);
  });

  it("starts from a hand-made sheet's whole level", () => {
    expect(startBuild({ ...blank, classes: [{ name: "Fighter", subclass: "", level: 3 }, { name: "Wizard", subclass: "", level: 2 }] }, rules).level).toBe(5);
  });

  it("fills starting gear", () => {
    expect(startingGear(fighter, rules, "A", "A")).toMatchObject({ armor: "Chain Mail", gp: 18 });
  });

  it("counts spells that aren't from a feat", () => {
    const spells = [
      { level: 0, prepared: false, notes: "" }, { level: 0, prepared: false, notes: "Magic Initiate" },
      { level: 1, prepared: true, notes: "" }, { level: 1, prepared: true, notes: "Always" },
    ];
    expect(spellCounts(spells)).toEqual({ cantrips: 1, prepared: 1 });
  });
});
