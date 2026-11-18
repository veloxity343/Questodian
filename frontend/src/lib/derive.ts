// Mirrors questodian.rules.derive so the sheet can update as you type.

import type { Ability, Character, Edition, Meta } from "../types";
import { ABILITIES, modOf, profBonus } from "./util";

export interface Derived {
  edition: Edition;
  level: number;
  pb: number;
  mods: Record<Ability, number>;
  /** Plain ability check bonuses (2014 Jack of All Trades and 2024 exhaustion included). */
  checks: Record<Ability, number>;
  saves: Record<Ability, number>;
  skills: Record<string, number>;
  initiative: number;
  passive: { perception: number; investigation: number; insight: number };
  spell: { dc: number; atk: number } | null;
  /** Speed after exhaustion. */
  speed: number;
  /** What the current exhaustion level does, e.g. ["D20 Tests -2", "Speed -5 ft."]. */
  exhaustion: string[];
}

// conditions/exhaustion in SRD 5.1; the effects are cumulative.
const EXHAUSTION_2014 = [
  "Disadvantage on ability checks",
  "Speed halved",
  "Disadvantage on attack rolls and saving throws",
  "Hit point maximum halved",
  "Speed reduced to 0",
  "Death",
];

function totalLevel(ch: Character) {
  return ch.classes.reduce((a, c) => a + (Number(c.level) || 0), 0) || 1;
}

/** 2024: D20 Tests -2 and Speed -5 ft. per level. 2014: the six-step table. */
function exhaustion(level: number, speed: number, edition: Edition) {
  const lvl = Math.max(0, Math.min(Number(level) || 0, 6));
  if (edition === "2024") {
    const effects = lvl ? [`D20 Tests -${2 * lvl}`, `Speed -${5 * lvl} ft.`] : [];
    if (lvl === 6) effects.push("Death");
    return { penalty: 2 * lvl, speed: Math.max(0, speed - 5 * lvl), effects };
  }
  const slowed = lvl >= 5 ? 0 : lvl >= 2 ? Math.floor(speed / 2) : speed;
  return { penalty: 0, speed: slowed, effects: EXHAUSTION_2014.slice(0, lvl) };
}

export function derive(ch: Character, meta: Meta | null, edition: Edition = "2014"): Derived {
  const level = totalLevel(ch);
  const pb = profBonus(level);
  const tired = exhaustion(ch.exhaustion, Number(ch.speed) || 0, edition);
  // The 2024 exhaustion penalty applies to D20 Tests. The save DC isn't a roll; passives aren't
  // either, so 2024 leaves them alone (our reading). 2014 exhaustion 1+ gives Disadvantage on
  // ability checks, which is -5 to passive scores (rules/ability-checks).
  const minus = tired.penalty;
  const passiveMinus = edition !== "2024" && Number(ch.exhaustion) >= 1 ? 5 : 0;
  const mods = Object.fromEntries(ABILITIES.map((a) => [a, modOf(ch.abilities[a])])) as Record<Ability, number>;
  const saves = Object.fromEntries(
    ABILITIES.map((a) => [a, mods[a] + (ch.save_proficiencies.includes(a) ? pb : 0) - minus]),
  ) as Record<Ability, number>;
  const skills: Record<string, number> = {};
  for (const [skill, ab] of Object.entries(meta?.skills || {})) {
    const p = ch.skills[skill] || "none";
    const bonus = p === "expertise" ? pb * 2 : p === "proficient" ? pb : p === "half" || ch.jack_of_all_trades ? Math.floor(pb / 2) : 0;
    skills[skill] = mods[ab] + bonus;
  }
  // Plain checks and initiative (a Dexterity check) have no proficiency: 2014 Jack of All Trades
  // covers them, the 2024 one only covers skills.
  const jack = ch.jack_of_all_trades && edition !== "2024" ? Math.floor(pb / 2) : 0;
  const checks = Object.fromEntries(ABILITIES.map((a) => [a, mods[a] + jack - minus])) as Record<Ability, number>;
  const sa = ch.spellcasting.ability as Ability;
  return {
    edition,
    level,
    pb,
    mods,
    checks,
    saves,
    skills: Object.fromEntries(Object.entries(skills).map(([k, v]) => [k, v - minus])),
    initiative: mods.dex + (Number(ch.initiative_misc) || 0) + jack - minus,
    passive: {
      perception: 10 + (skills.perception ?? 0) - passiveMinus,
      investigation: 10 + (skills.investigation ?? 0) - passiveMinus,
      insight: 10 + (skills.insight ?? 0) - passiveMinus,
    },
    spell: sa && mods[sa] !== undefined ? { dc: 8 + pb + mods[sa], atk: pb + mods[sa] - minus } : null,
    speed: tired.speed,
    exhaustion: tired.effects,
  };
}
