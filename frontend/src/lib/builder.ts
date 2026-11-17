// The character builder (2024 rules): the choices a player makes (`Build`) and everything worked
// out from them, with a plain-language explanation of each number. Ported from the dnd
// repo's character sheet spreadsheet (tools/character-sheet/build.py and its button macros).
// The rules data comes from GET /api/rules: the bundled data with the user's own edits.

import type { Ability, Attack, Character, Feature, Resource, SkillProficiency } from "../types";
import { ABILITIES, ABILITY_NAMES, modOf, profBonus, signed } from "./util";

// ------------------------------------------------------------------ rules data

export type Source = "bundled" | "edited" | "added";
export type ShortRest = "all" | "one" | "none" | { from_level: number; then: ShortRest; else: ShortRest };
export type ResourceMax =
  | { column: number }
  | { ability_mod: Ability; min: number }
  | { by_level: Record<string, number> }
  | { per_level: number }
  | { fixed: number }
  | { proficiency_bonus: true };

export interface GearLine { item: string; qty: number }
export interface LevelRow {
  level: number;
  features: string[];
  columns: (string | number)[];
  cantrips: number;
  prepared: number;
  slots: number[];
  pact_slots: number;
  pact_level: number;
  masteries: number;
}
export interface ClassRules {
  name: string;
  hit_die: number;
  saves: Ability[];
  skill_picks: number;
  skill_options: string[] | "any";
  weapon_rule: "All" | "Simple" | "Monk" | "Rogue";
  /** Martial weapons it's trained with beyond its rule (the Rogue's Longsword). */
  extra_weapons?: string[];
  weapons_text: string;
  armor_text: string;
  armor_training: { light: boolean; medium: boolean; heavy: boolean; shield: boolean };
  spell_ability: Ability | null;
  unarmored_defense: "Barbarian" | "Monk" | null;
  tools_text: string;
  column_labels: string[];
  standard_array: number[];
  resources: { name: string; max: ResourceMax; short_rest: ShortRest }[];
  fighting_style_level: number | null;
  starting_gear: Record<string, GearLine[]>;
  levels: LevelRow[];
  srd?: boolean;
  source?: Source;
}
export interface Lineage { name: string; text: string; darkvision: number; extra_speed: number }
export interface SpeciesRules {
  name: string;
  size: string;
  speed: number;
  darkvision: number;
  hp_per_level: number;
  extra_skill_picks: number;
  /** Where the extra skill comes from (the Elf's Keen Senses); any skill when missing. */
  extra_skill_options?: string[] | null;
  carry_multiplier: number;
  traits: string;
  choice_label: string | null;
  choices?: Lineage[];
  resource: { name: string; max: ResourceMax; short_rest: ShortRest } | null;
  heroic_inspiration_on_long_rest: boolean;
  extra_origin_feat: boolean;
  srd?: boolean;
  source?: Source;
}
export interface BackgroundRules {
  name: string;
  abilities: Ability[];
  feat: string;
  skills: string[];
  tool: string;
  starting_gear: Record<string, GearLine[]>;
  srd?: boolean;
  source?: Source;
}
export interface FeatRules { name: string; type: string; text: string; srd?: boolean; source?: Source }
export interface ArmorRules { name: string; category: "None" | "Light" | "Medium" | "Heavy" | "Shield"; base_ac: number; max_dex: number; str_required: number; stealth: string; source?: Source }
export interface WeaponRules { name: string; damage: string; properties: string; mastery: string; category: string; kind: "Melee" | "Ranged"; weight: string; cost: string; source?: Source }
export interface ItemRules { name: string; weight: number; cost: string; kind: string; note: string; source?: Source }
export interface MasteryRules { name: string; text: string; source?: Source }

export interface Rules {
  meta: { title: string; source: string };
  abilities: Ability[];
  skills: Record<string, Ability>;
  ability_uses: Record<Ability, string>;
  point_buy: { budget: number; min: number; max: number; costs: Record<string, number> };
  standard_array: number[];
  rolled: { min: number; max: number };
  score_cap: number;
  alignments: string[];
  fighting_styles: string[];
  classes: ClassRules[];
  species: SpeciesRules[];
  backgrounds: BackgroundRules[];
  feats: FeatRules[];
  armor: ArmorRules[];
  weapons: WeaponRules[];
  masteries: MasteryRules[];
  items: ItemRules[];
  hidden: Partial<Record<string, string[]>>;
  edited_settings: string[];
}

// ------------------------------------------------------------------ the player's choices

export type Method = "point_buy" | "standard_array" | "rolled";
export const METHODS: [Method, string][] = [["point_buy", "Point Buy"], ["standard_array", "Standard Array"], ["rolled", "Rolled"]];

export interface LevelLogEntry { date: string; level: number; hp: number; gained: string[]; todo: string[] }

export interface Build {
  method: Method;
  base: Record<Ability, number>;
  /** Background increases: 2 or 1 on the background's abilities. */
  background_bonus: Partial<Record<Ability, number>>;
  /** Ability Score Improvements and other feats. */
  other_bonus: Partial<Record<Ability, number>>;
  class: string;
  level: number;
  subclass: string;
  species: string;
  lineage: string;
  background: string;
  /** The Human's second Origin feat. */
  human_feat: string;
  fighting_style: string;
  /** Skills the player picked (class, species and Skilled picks). Background skills aren't here. */
  skill_picks: string[];
  expertise: string[];
  armor: string;
  shield: boolean;
  weapons: string[];
  /** HP from rolling instead of taking the average at level up (can be negative). */
  extra_hp: number;
  level_log: LevelLogEntry[];
  /** What the builder last wrote, so it replaces only its own rows and leaves the player's alone. */
  made?: Made;
}

export interface Made { features: string[]; attacks: string[]; resources: string[] }

export const isBuilt = (ch: Character) => !!(ch.build as Partial<Build> | undefined)?.class;

/** A character's build with anything missing filled in. */
export function buildOf(ch: Character): Build {
  const b = (ch.build ?? {}) as Partial<Build>;
  const zero = () => Object.fromEntries(ABILITIES.map((a) => [a, 0])) as Record<Ability, number>;
  return {
    method: b.method ?? "point_buy",
    base: { ...Object.fromEntries(ABILITIES.map((a) => [a, 8])), ...b.base } as Record<Ability, number>,
    background_bonus: { ...b.background_bonus },
    other_bonus: { ...zero(), ...b.other_bonus },
    class: b.class ?? "",
    level: Math.min(20, Math.max(1, Number(b.level) || 1)),
    subclass: b.subclass ?? "",
    species: b.species ?? "",
    lineage: b.lineage ?? "",
    background: b.background ?? "",
    human_feat: b.human_feat ?? "",
    fighting_style: b.fighting_style ?? "",
    skill_picks: [...(b.skill_picks ?? [])],
    expertise: [...(b.expertise ?? [])],
    armor: b.armor ?? "None (unarmored)",
    shield: !!b.shield,
    weapons: [...(b.weapons ?? [])],
    extra_hp: Number(b.extra_hp) || 0,
    level_log: [...(b.level_log ?? [])],
    made: b.made,
  };
}

/** A fresh build: point buy at 8s, or what the character already has where it matches. */
export function startBuild(ch: Character, rules: Rules): Build {
  const b = buildOf({ ...ch, build: {} });
  const named = <T extends { name: string }>(list: T[], name: string) => list.find((x) => x.name.toLowerCase() === name.trim().toLowerCase())?.name ?? "";
  b.class = named(rules.classes, ch.classes[0]?.name ?? "") || rules.classes[0]?.name || "";
  // The builder has one class: it takes the character's whole level.
  b.level = Math.min(20, Math.max(1, ch.classes.reduce((t, c) => t + (Number(c.level) || 0), 0) || 1));
  b.subclass = ch.classes[0]?.subclass ?? "";
  // "Elf (Wood Elf)" is the species and its lineage, as the builder writes it.
  const [, speciesName = ch.race, lineageName = ""] = ch.race.match(/^(.*?)\s*\((.*)\)$/) ?? [];
  b.species = named(rules.species, speciesName);
  b.lineage = rules.species.find((x) => x.name === b.species)?.choices?.find((c) => c.name.toLowerCase() === lineageName.toLowerCase())?.name ?? "";
  b.background = named(rules.backgrounds, ch.background);
  const scores = ABILITIES.map((a) => Number(ch.abilities[a]) || 10);
  if (!scores.every((x) => x === 10)) {
    // Typed-in scores become rolled bases; anything above the rolled maximum goes in "Other +".
    b.method = "rolled";
    ABILITIES.forEach((a, i) => {
      b.base[a] = Math.min(rules.rolled.max, Math.max(rules.rolled.min, scores[i]));
      b.other_bonus[a] = Math.max(0, scores[i] - b.base[a]);
    });
  }
  return b;
}

/** What switching a hand-made sheet to the builder changes besides the worked-out numbers. */
export function startNotes(ch: Character, b: Build): string[] {
  const notes: string[] = [];
  if (ch.classes.length > 1) notes.push(`The builder has one class: ${ch.classes.map((c) => `${c.name} ${c.level}`).join(" / ")} becomes ${b.class} ${b.level}.`);
  else if (ch.classes[0]?.name && ch.classes[0].name.toLowerCase() !== b.class.toLowerCase()) notes.push(`${ch.classes[0].name} isn't in the rules data, so the class becomes ${b.class}.`);
  if (ch.race && !b.species) notes.push(`${ch.race} isn't in the rules data: pick a species.`);
  if (ch.background && !b.background) notes.push(`${ch.background} isn't in the rules data: pick a background.`);
  return notes;
}

// ------------------------------------------------------------------ lookups

export const skillKey = (name: string) => name.toLowerCase().replace(/\s+/g, "-");
const find = <T extends { name: string }>(list: T[], name: string) => list.find((x) => x.name === name);
const hasProperty = (w: WeaponRules, prop: string) => new RegExp(`\\b${prop}\\b`, "i").test(w.properties);

export interface Parts {
  cls?: ClassRules;
  row?: LevelRow;
  species?: SpeciesRules;
  lineage?: Lineage;
  background?: BackgroundRules;
  feats: string[];
  armor?: ArmorRules;
}

export function parts(b: Build, rules: Rules): Parts {
  const cls = find(rules.classes, b.class);
  const species = find(rules.species, b.species);
  const background = find(rules.backgrounds, b.background);
  const lineage = species?.choices?.find((c) => c.name === b.lineage);
  const feats = [background?.feat, species?.extra_origin_feat ? b.human_feat : ""].filter((f): f is string => !!f);
  return {
    cls,
    row: cls?.levels.find((r) => r.level === b.level),
    species,
    lineage,
    background,
    feats,
    armor: find(rules.armor, b.armor) ?? rules.armor.find((a) => a.category === "None"),
  };
}

const hasFeat = (p: Parts, name: string) => p.feats.includes(name);
export const fightingStyleAllowed = (b: Build, p: Parts) => p.cls?.fighting_style_level != null && b.level >= p.cls.fighting_style_level;
export const needsLineage = (p: Parts) => (p.species?.choices?.length ?? 0) > 1;

// ------------------------------------------------------------------ ability scores

export function pointCost(rules: Rules, score: number): number | null {
  const cost = rules.point_buy.costs[String(score)];
  return cost === undefined ? null : cost;
}

/** Points spent, or null if a base score is outside point buy's range. */
export function pointsSpent(b: Build, rules: Rules): number | null {
  let total = 0;
  for (const a of ABILITIES) {
    const cost = pointCost(rules, b.base[a]);
    if (cost === null) return null;
    total += cost;
  }
  return total;
}

export function standardArrayOK(b: Build, rules: Rules) {
  const sorted = (xs: number[]) => [...xs].sort((x, y) => x - y).join(",");
  return sorted(ABILITIES.map((a) => b.base[a])) === sorted(rules.standard_array);
}

/** Level 20 capstones that raise two scores by 4, to a maximum of 25 (Primal Champion, Body and Mind). */
const CAPSTONES: Record<string, Ability[]> = { Barbarian: ["str", "con"], Monk: ["dex", "wis"] };
export const capstone = (b: Build, a: Ability) => b.level >= 20 && !!CAPSTONES[b.class]?.includes(a);

export function scores(b: Build, rules: Rules): Record<Ability, number> {
  return Object.fromEntries(
    ABILITIES.map((a) => {
      const score = Math.min(rules.score_cap, b.base[a] + (b.background_bonus[a] ?? 0) + (b.other_bonus[a] ?? 0));
      return [a, capstone(b, a) ? Math.min(25, score + 4) : score];
    }),
  ) as Record<Ability, number>;
}

/** Whether − (dir -1) or + (dir 1) can be used on an ability (greyed out otherwise). */
export function canStep(b: Build, rules: Rules, a: Ability, dir: 1 | -1): boolean {
  const cur = b.base[a];
  if (b.method === "standard_array") {
    const arr = [...rules.standard_array].sort((x, y) => x - y);
    return standardArrayOK(b, rules) && (dir > 0 ? cur < arr[arr.length - 1] : cur > arr[0]);
  }
  if (b.method === "rolled") return dir > 0 ? cur < rules.rolled.max : cur > rules.rolled.min;
  const { min, max, budget } = rules.point_buy;
  if (cur < min || cur > max) return true; // out of range (e.g. after switching method): a click brings it back in
  if (dir < 0) return cur > min;
  const spent = pointsSpent(b, rules) ?? 0;
  return cur < max && (pointCost(rules, cur + 1) ?? 0) - (pointCost(rules, cur) ?? 0) <= budget - spent;
}

/** − / + on a base score. Standard array swaps with the next score in the array, so it stays valid. */
export function step(b: Build, rules: Rules, a: Ability, dir: 1 | -1): Build {
  if (!canStep(b, rules, a, dir)) return b;
  const base = { ...b.base };
  const cur = base[a];
  if (b.method === "standard_array") {
    const arr = [...rules.standard_array].sort((x, y) => x - y);
    const target = arr[arr.indexOf(cur) + dir];
    const other = ABILITIES.find((x) => x !== a && base[x] === target);
    if (other) base[other] = cur;
    base[a] = target;
  } else if (b.method === "rolled") {
    base[a] = cur + dir;
  } else {
    const { min, max } = rules.point_buy;
    base[a] = cur < min ? min : cur > max ? max : cur + dir;
  }
  return { ...b, base };
}

/** Deal out fresh scores for the method: 8s for point buy, the class's suggested array, or 10s. */
export function resetScores(b: Build, rules: Rules): Build {
  const cls = find(rules.classes, b.class);
  // The class's suggested order, as long as it's the same six numbers as the (possibly edited) array.
  const same = (x: number[]) => [...x].sort((m, n) => m - n).join() === [...rules.standard_array].sort((m, n) => m - n).join();
  const array = cls?.standard_array?.length === 6 && same(cls.standard_array) ? cls.standard_array : rules.standard_array;
  const value = (i: number) => (b.method === "point_buy" ? rules.point_buy.min : b.method === "standard_array" ? array[i] : 10);
  return { ...b, base: Object.fromEntries(ABILITIES.map((a, i) => [a, value(i)])) as Record<Ability, number> };
}

/**
 * The background's +2 / +1 picks, like radio buttons: +2 on one of its three abilities and +1 on
 * another, or +1 on all three. Picking a +2 moves any other +2 here; with a +2, a new +1 replaces
 * the old one. Clicking a pick again clears it.
 */
export function pickBackground(b: Build, rules: Rules, a: Ability, amount: 1 | 2): Build {
  const allowed = find(rules.backgrounds, b.background)?.abilities ?? [];
  if (!allowed.includes(a)) return b;
  const bonus: Partial<Record<Ability, number>> = {};
  for (const x of allowed) if (b.background_bonus[x]) bonus[x] = b.background_bonus[x]; // drop picks from another background
  if (bonus[a] === amount) {
    delete bonus[a];
    return { ...b, background_bonus: bonus };
  }
  if (amount === 2) {
    for (const x of allowed) if (x !== a && bonus[x] === 2) delete bonus[x];
    bonus[a] = 2;
    let keptOne = false;
    for (const x of allowed) {
      if (x !== a && bonus[x] === 1) {
        if (keptOne) delete bonus[x];
        else keptOne = true;
      }
    }
  } else {
    bonus[a] = 1;
    if (Object.values(bonus).includes(2)) for (const x of allowed) if (x !== a && bonus[x] === 1) delete bonus[x];
  }
  return { ...b, background_bonus: bonus };
}

// ------------------------------------------------------------------ everything worked out

export interface Explained<T> { value: T; how: string }
export interface BuiltWeapon { name: string; attack: number; damage: string; mastery: string; notes: string; proficient: boolean; how: string; damageHow: string }
export interface BuiltResource { name: string; max: number; reset: Resource["reset"]; how: string }
export interface Sheet {
  pb: number;
  scores: Record<Ability, number>;
  mods: Record<Ability, number>;
  scoreHow: Record<Ability, string>;
  saves: Ability[];
  /** Proficiency for every skill (by key), and where it comes from. */
  skills: Record<string, { prof: SkillProficiency; from: string; bonus: number }>;
  skillPicks: { used: number; allowed: number; text: string; ok: boolean };
  jack: boolean;
  ac: Explained<number>;
  initiative: Explained<number>;
  hp: Explained<number>;
  speed: Explained<number>;
  darkvision: Explained<number>;
  passivePerception: Explained<number>;
  spell: { ability: Ability; dc: number; attack: number; how: string } | null;
  attacksPerAction: Explained<number>;
  masteries: Explained<number>;
  weapons: BuiltWeapon[];
  unarmed: BuiltWeapon;
  resources: BuiltResource[];
  cantrips: number;
  prepared: number;
  slots: number[];
  pact: { slots: number; level: number };
  carry: Explained<number>;
  features: Feature[];
  proficiencies: Character["proficiencies"];
  checks: { abilities: string; background: string; points: string };
  warnings: string[];
  hitDie: number;
}

function resourceMax(max: ResourceMax, b: Build, row: LevelRow | undefined, mods: Record<Ability, number>, pb: number): number {
  if ("column" in max) return Number(row?.columns[max.column]) || 0;
  if ("ability_mod" in max) return Math.max(max.min, mods[max.ability_mod]);
  if ("by_level" in max) {
    let n = 0;
    for (const [lvl, v] of Object.entries(max.by_level)) if (b.level >= Number(lvl)) n = v;
    return n;
  }
  if ("per_level" in max) return max.per_level * b.level;
  if ("fixed" in max) return max.fixed;
  return pb;
}

function resetOf(rule: ShortRest, level: number): Resource["reset"] {
  if (typeof rule === "object") return resetOf(level >= rule.from_level ? rule.then : rule.else, level);
  return rule === "all" ? "short" : rule === "one" ? "short_one" : "long";
}

const RESET_TEXT: Record<Resource["reset"], string> = {
  short: "Short rest: all · Long rest: all",
  short_one: "Short rest: 1 back · Long rest: all",
  long: "Long rest: all",
  none: "Reset by hand",
};
export const resetText = (r: Resource["reset"]) => RESET_TEXT[r];

function weapon(w: WeaponRules, b: Build, p: Parts, mods: Record<Ability, number>, sc: Record<Ability, number>, pb: number, rules: Rules): BuiltWeapon {
  const rule = p.cls?.weapon_rule ?? "Simple";
  const light = hasProperty(w, "Light");
  const finesse = hasProperty(w, "Finesse");
  const proficient = rule === "All" || w.category === "Simple" || (rule === "Monk" && light) || (rule === "Rogue" && (light || finesse))
    || !!p.cls?.extra_weapons?.includes(w.name);
  // Martial Arts works only unarmored and without a shield.
  const martialArts = b.class === "Monk" && p.armor?.category === "None" && !b.shield;
  const monkWeapon = martialArts && w.kind === "Melee" && (w.category === "Simple" || light);
  const ranged = w.kind === "Ranged";
  const ability: Ability = finesse || monkWeapon ? (mods.dex > mods.str ? "dex" : "str") : ranged ? "dex" : "str";
  const mod = mods[ability];
  const archery = b.fighting_style === "Archery" && fightingStyleAllowed(b, p) && ranged;
  const attack = mod + (proficient ? pb : 0) + (archery ? 2 : 0);
  const [weaponDie, ...type] = w.damage.split(" ");
  // Martial Arts: a Monk weapon can use the Martial Arts die instead, when it's bigger.
  const artsDie = monkWeapon ? String(p.row?.columns[0] ?? "") : "";
  const sides = (d: string) => Number(d.match(/d(\d+)/)?.[1] ?? 0);
  const dice = artsDie && sides(artsDie) > sides(weaponDie) ? artsDie : weaponDie;
  const damage = `${dice}${mod === 0 ? "" : signed(mod)} ${type.join(" ")}`.trim();
  const why = finesse
    ? " (finesse: Str or Dex, whichever is higher)"
    : monkWeapon ? " (Martial Arts: Monks can use Dex)"
    : ranged ? " (ranged weapons use Dex)"
    : hasProperty(w, "Thrown") ? " (melee weapon, thrown or not: Str)" : " (melee weapons use Str)";
  const heavyLow = hasProperty(w, "Heavy") && sc[ranged ? "dex" : "str"] < 13;
  const how = `${w.name}: ${signed(attack)} = ${ABILITY_NAMES[ability].slice(0, 3)} ${signed(mod)}${why}`
    + (proficient ? ` + Prof ${signed(pb)}` : " + no Prof (not proficient)")
    + (archery ? " + Archery 2" : "")
    + (heavyLow ? `. Heavy weapon and your ${ranged ? "Dex" : "Str"} is under 13: Disadvantage.` : "");
  const masteries = p.row?.masteries ?? 0;
  const masteryText = rules.masteries.find((m) => m.name === w.mastery)?.text ?? "";
  const twf = b.fighting_style === "Two-Weapon Fighting" && fightingStyleAllowed(b, p);
  const damageHow = `Damage ${damage} = ${dice === weaponDie ? "weapon die" : "Martial Arts die (bigger than the weapon's)"} + the same ${ABILITY_NAMES[ability].slice(0, 3)} modifier.`
    + (hasProperty(w, "Versatile") ? " Versatile: bigger die two-handed (see properties)." : "")
    + (light ? ` Light: extra off-hand attack as a Bonus Action, without the modifier unless it's negative${twf ? " (Two-Weapon Fighting adds it)" : ""}.` : "")
    + (masteries > 0 ? ` Mastery ${w.mastery}: ${masteryText}` : "");
  const notes = [`Uses ${ABILITY_NAMES[ability].slice(0, 3)}`, w.properties, proficient ? "" : "NOT PROFICIENT"].filter(Boolean).join(" · ");
  return { name: w.name, attack, damage, mastery: masteries > 0 ? w.mastery : "", notes, proficient, how, damageHow };
}

export function work(b: Build, rules: Rules): Sheet {
  const p = parts(b, rules);
  const { cls, row, species, lineage, background } = p;
  const pb = profBonus(b.level);
  const sc = scores(b, rules);
  const mods = Object.fromEntries(ABILITIES.map((a) => [a, modOf(sc[a])])) as Record<Ability, number>;
  const style = fightingStyleAllowed(b, p) ? b.fighting_style : "";
  const S = signed;

  const scoreHow = Object.fromEntries(ABILITIES.map((a) => {
    const raw = b.base[a] + (b.background_bonus[a] ?? 0) + (b.other_bonus[a] ?? 0);
    return [a, `Score ${sc[a]} = base ${b.base[a]} + background ${b.background_bonus[a] ?? 0} + other ${b.other_bonus[a] ?? 0}`
      + (raw > rules.score_cap ? ` (capped at ${rules.score_cap})` : "") + (capstone(b, a) ? ` + 4 from your level 20 feature (up to 25)` : "") + `. Modifier ${S(mods[a])} = (score − 10) ÷ 2, rounded down.`];
  })) as Record<Ability, string>;

  // Skills: background skills, then the player's picks; Expertise on top. Bards from level 2 add
  // half their Proficiency Bonus to the rest (Jack of All Trades: skill checks only).
  const jack = b.class === "Bard" && b.level >= 2;
  const bgSkills = (background?.skills ?? []).map(skillKey);
  const skills: Sheet["skills"] = {};
  for (const [name, ab] of Object.entries(rules.skills)) {
    const key = skillKey(name);
    const fromBg = bgSkills.includes(key);
    const picked = b.skill_picks.includes(key);
    const expert = (fromBg || picked) && b.expertise.includes(key);
    const prof: SkillProficiency = expert ? "expertise" : fromBg || picked ? "proficient" : jack ? "half" : "none";
    const bonus = mods[ab] + (prof === "expertise" ? 2 * pb : prof === "proficient" ? pb : prof === "half" ? Math.floor(pb / 2) : 0);
    skills[key] = { prof, from: fromBg ? "Background" : picked ? "Your pick" : "", bonus };
  }
  const skilled = hasFeat(p, "Skilled");
  const allowed = (cls?.skill_picks ?? 0) + (species?.extra_skill_picks ?? 0) + (skilled ? 3 : 0);
  const picks = b.skill_picks.filter((k) => !bgSkills.includes(k));
  const options = cls?.skill_options === "any" ? null : (cls?.skill_options ?? []).map(skillKey);
  const outsideClass = options ? picks.filter((k) => !options.includes(k)).length : 0;
  const freePicks = allowed - (cls?.skill_picks ?? 0);
  // The Elf's extra skill (Keen Senses) is one of a few; count picks that fit neither list.
  const speciesOptions = species?.extra_skill_options?.map(skillKey);
  const neither = options && speciesOptions && !skilled ? picks.filter((k) => !options.includes(k) && !speciesOptions.includes(k)).length : 0;
  const pickOK = picks.length === allowed && outsideClass <= freePicks && neither === 0;
  const pickText = `Used ${picks.length} of ${allowed} (class ${cls?.skill_picks ?? 0}`
    + (species?.extra_skill_picks ? `, ${species.name} ${species.extra_skill_picks}` : "") + (skilled ? ", Skilled feat 3" : "") + ")."
    + (options && outsideClass > freePicks ? ` ${outsideClass - freePicks} of your picks aren't on the ${cls?.name} list.` : options ? ` Class picks come from the ${cls?.name} list.` : "")
    + (neither ? ` The ${species!.name} pick must be ${species!.extra_skill_options!.join(", ")}.` : "");

  // Armor Class.
  const armor = p.armor;
  const cat = armor?.category ?? "None";
  const unarmored = cat === "None";
  let acValue: number;
  let acHow: string;
  if (unarmored) {
    // Unarmored Defense is another way to work out AC: use whichever is higher.
    acValue = 10 + mods.dex;
    acHow = `No armor: 10 + Dex ${S(mods.dex)}`;
    if (cls?.unarmored_defense === "Barbarian" && mods.con > 0) {
      acValue += mods.con;
      acHow = `Unarmored Defense (Barbarian): 10 + Dex ${S(mods.dex)} + Con ${S(mods.con)}`;
    } else if (cls?.unarmored_defense === "Monk" && !b.shield && mods.wis > 0) {
      acValue += mods.wis;
      acHow = `Unarmored Defense (Monk): 10 + Dex ${S(mods.dex)} + Wis ${S(mods.wis)}`;
    }
  } else {
    const dex = Math.min(mods.dex, armor!.max_dex);
    acValue = armor!.base_ac + dex;
    acHow = `${armor!.name} ${armor!.base_ac}` + (cat === "Heavy" ? " (heavy armor: no Dex)" : ` + Dex ${S(dex)}${cat === "Medium" ? " (medium armor: Dex counts up to +2)" : ""}`);
  }
  // Without shield training there's no AC bonus from it (rules glossary: Armor Training).
  const shieldTrained = cls?.armor_training.shield ?? false;
  if (b.shield && shieldTrained) {
    acValue += 2;
    acHow += " + Shield 2";
  } else if (b.shield) {
    acHow += " (no Shield bonus: not trained with shields)";
  }
  if (style === "Defense" && !unarmored) {
    acValue += 1;
    acHow += " + Defense style 1";
  }

  // Initiative: Dex, plus Proficiency with Alert. Jack of All Trades doesn't apply (not a skill check).
  const alert = hasFeat(p, "Alert");
  const initiative = mods.dex + (alert ? pb : 0);
  const initHow = `Initiative ${S(initiative)} = Dex ${S(mods.dex)}${alert ? ` + Alert feat (Prof ${S(pb)})` : ""}. Roll d20 + this when a fight starts.`
    + (jack ? " (Jack of All Trades doesn't apply: Initiative isn't a skill check.)" : "");

  // Hit Points: the full die at level 1, the average after that; Dwarven Toughness and Tough add per level.
  const die = cls?.hit_die ?? 8;
  const tough = hasFeat(p, "Tough");
  const spHp = species?.hp_per_level ?? 0;
  const hpValue = Math.max(1, die + mods.con + (b.level - 1) * (die / 2 + 1 + mods.con) + spHp * b.level + (tough ? 2 * b.level : 0) + b.extra_hp);
  const hpHow = cls
    ? `HP max ${hpValue} = level 1: the full d${die} (${die}) + Con ${S(mods.con)}`
      + (b.level > 1 ? `; levels 2–${b.level}: ${die / 2 + 1} + Con ${S(mods.con)} each (average)` : "")
      + (spHp > 0 ? `; ${species!.name} +${spHp * b.level}` : "") + (tough ? `; Tough feat +${2 * b.level}` : "")
      + (b.extra_hp ? `; extra from rolling ${S(b.extra_hp)}` : "")
    : "Pick a class.";

  // Speed.
  const strShort = !!armor && armor.str_required > 0 && sc.str < armor.str_required;
  const monkMove = b.class === "Monk" && unarmored && !b.shield ? parseInt(String(row?.columns[2] ?? "").replace(/[^\d]/g, ""), 10) || 0 : 0;
  const barbMove = b.class === "Barbarian" && b.level >= 5 && cat !== "Heavy" ? 10 : 0;
  const linSpeed = lineage?.extra_speed ?? 0;
  const speedValue = (species?.speed ?? 30) + linSpeed + monkMove + barbMove - (strShort ? 10 : 0);
  const speedHow = `Speed ${speedValue} ft = ${species?.name ?? "base"} ${species?.speed ?? 30} ft`
    + (linSpeed ? ` + ${lineage!.name} ${linSpeed} ft` : "") + (monkMove ? ` + Unarmored Movement ${monkMove} ft` : "")
    + (barbMove ? " + Fast Movement 10 ft" : "") + (strShort ? ` − 10 ft (Str ${sc.str} is below this armor's ${armor!.str_required})` : "");

  const dark = Math.max(species?.darkvision ?? 0, lineage?.darkvision ?? 0);
  const darkHow = dark ? `${species?.name} Darkvision: within ${dark} ft you see dim light as bright and darkness as dim (shades of grey).` : "No Darkvision: you need light to see.";

  const perception = skills.perception?.bonus ?? mods.wis;
  const ppHow = `Passive Perception ${10 + perception} = 10 + your Perception bonus ${S(perception)} (Wis ${S(mods.wis)}${perception !== mods.wis ? " + proficiency" : ""}). The DM uses it to see if you notice things without rolling.`;

  const sa = cls?.spell_ability ?? null;
  const spell = sa
    ? { ability: sa, dc: 8 + pb + mods[sa], attack: pb + mods[sa],
        how: `Spell save DC ${8 + pb + mods[sa]} = 8 + Prof ${S(pb)} + ${ABILITY_NAMES[sa]} ${S(mods[sa])}. Spell attack ${S(pb + mods[sa])} = Prof ${S(pb)} + ${ABILITY_NAMES[sa]} ${S(mods[sa])}.` }
    : null;

  const extra = ["Barbarian", "Monk", "Paladin", "Ranger"].includes(b.class);
  const apa = b.class === "Fighter" ? (b.level >= 20 ? 4 : b.level >= 11 ? 3 : b.level >= 5 ? 2 : 1) : extra && b.level >= 5 ? 2 : 1;
  const apaHow = `Attacks per Attack action: ${apa}` + (b.class === "Fighter" ? ". Fighters get Extra Attack at 5, two extra at 11, three at 20." : extra ? `. ${b.class}s get Extra Attack (2 attacks) at level 5.` : `. ${b.class ? `${b.class}s don't` : "This class doesn't"} get Extra Attack.`);
  const masteries = row?.masteries ?? 0;
  const masteryHow = masteries > 0 ? `Weapon Mastery: you can use the mastery property of ${masteries} kinds of weapon you're proficient with. You can swap one after a Long Rest.` : `${b.class ? `${b.class}s don't` : "This class doesn't"} get Weapon Mastery.`;

  const weapons = b.weapons.map((n) => find(rules.weapons, n)).filter((w): w is WeaponRules => !!w).map((w) => weapon(w, b, p, mods, sc, pb, rules));
  const monkOK = b.class === "Monk" && unarmored && !b.shield;
  const uaAbility: Ability = monkOK && mods.dex > mods.str ? "dex" : "str";
  const uaMod = mods[uaAbility];
  const uaDamage = monkOK ? `${row?.columns[0] ?? "1d6"}${uaMod === 0 ? "" : S(uaMod)} Bludgeoning` : `${1 + mods.str} Bludgeoning`;
  // Martial Arts lets a Monk use Dex for the Grapple and Shove DC.
  const grappleAbility: Ability = monkOK && mods.dex > mods.str ? "dex" : "str";
  const grappleDC = 8 + mods[grappleAbility] + pb;
  const unarmed: BuiltWeapon = {
    name: "Unarmed Strike", attack: uaMod + pb, damage: uaDamage, mastery: "", proficient: true,
    notes: `Uses ${ABILITY_NAMES[uaAbility].slice(0, 3)} · 5 ft reach · or Grapple / Shove (DC ${grappleDC})`,
    how: `Unarmed Strike: ${S(uaMod + pb)} = ${ABILITY_NAMES[uaAbility].slice(0, 3)} ${S(uaMod)}${monkOK ? " (Martial Arts: higher of Str or Dex)" : ""} + Prof ${S(pb)} (everyone is proficient).`,
    damageHow: `Damage ${uaDamage}${monkOK ? " (Martial Arts die + modifier)" : " (1 + Str)"}. Grapple or Shove instead: the target makes a Str or Dex save (its choice) against DC ${grappleDC} = 8 + ${ABILITY_NAMES[grappleAbility].slice(0, 3)} ${S(mods[grappleAbility])} + Prof ${S(pb)}.`,
  };

  const resources: BuiltResource[] = [];
  for (const r of cls?.resources ?? []) {
    const max = resourceMax(r.max, b, row, mods, pb);
    if (max > 0) resources.push({ name: r.name, max, reset: resetOf(r.short_rest, b.level), how: resourceHow(r.max, r.name, max, b, cls, pb, mods) });
  }
  if (species?.resource) {
    const r = species.resource;
    const max = resourceMax(r.max, b, row, mods, pb);
    if (max > 0) resources.push({ name: r.name, max, reset: resetOf(r.short_rest, b.level), how: resourceHow(r.max, r.name, max, b, cls, pb, mods) });
  }

  const carryMult = species?.carry_multiplier ?? 1;
  const carry = sc.str * 15 * carryMult;
  const carryHow = `Carrying capacity ${carry} lb = Str ${sc.str} × 15${carryMult !== 1 ? ` × ${carryMult} (${species!.name})` : ""} (drag/lift ${carry * 2} lb)`;

  const features: Feature[] = [];
  for (const r of cls?.levels ?? []) {
    if (r.level > b.level) break;
    for (const f of r.features) features.push({ name: f, source: `${cls!.name} ${r.level}`, description: "" });
  }
  if (species) features.push({ name: `${species.name} traits`, source: "Species", description: species.traits + (lineage ? `\n${lineage.name}: ${lineage.text}` : "") });
  for (const name of p.feats) {
    const feat = find(rules.feats, name);
    features.push({ name, source: name === background?.feat ? `Background (${background.name})` : `${species?.name} feat`, description: feat?.text ?? "" });
  }
  if (style) features.push({ name: `Fighting Style: ${style}`, source: cls!.name, description: "" });

  const proficiencies: Character["proficiencies"] = {
    armor: cls?.armor_text ?? "",
    weapons: cls?.weapons_text ?? "",
    tools: [cls?.tools_text && cls.tools_text !== "—" ? cls.tools_text : "", background?.tool ?? ""].filter(Boolean).join(", "),
    languages: "Common + 2 of your choice",
  };

  // The checks the spreadsheet shows beside the ability table.
  const spent = pointsSpent(b, rules);
  const pbLimits = rules.point_buy;
  const abilitiesCheck = b.method === "point_buy"
    ? spent === null ? `Point buy: base scores must be ${pbLimits.min}–${pbLimits.max}` : spent > pbLimits.budget ? `Point buy: ${spent - pbLimits.budget} points over (${pbLimits.budget} max)` : "OK"
    : b.method === "standard_array" ? standardArrayOK(b, rules) ? "OK" : `Use ${rules.standard_array.join(", ")} once each` : "OK";
  const bgAbilities = background?.abilities ?? [];
  const bgValues = ABILITIES.map((a) => b.background_bonus[a] ?? 0);
  const onBg = ABILITIES.every((a) => !(b.background_bonus[a] ?? 0) || bgAbilities.includes(a));
  const bgNames = bgAbilities.map((a) => ABILITY_NAMES[a]);
  const backgroundCheck = !background ? "Pick a background"
    : bgValues.reduce((x, y) => x + y, 0) === 3 && bgValues.filter((v) => v === 2).length <= 1 && onBg ? "OK"
    : `Put +2/+1 or +1/+1/+1 on ${bgNames.slice(0, -1).join(", ")} or ${bgNames.at(-1)}`;
  const pointsText = spent === null ? "—" : `${spent} of ${pbLimits.budget}`;

  // Warnings and notes.
  const warnings: string[] = [];
  if (b.species && !species) warnings.push(`${b.species} isn't in the rules data any more: pick a species, or add it back on the Rules data page.`);
  if (b.background && !background) warnings.push(`${b.background} isn't in the rules data any more: pick a background, or add it back on the Rules data page.`);
  const trained = cls?.armor_training;
  if (trained && ((cat === "Light" && !trained.light) || (cat === "Medium" && !trained.medium) || (cat === "Heavy" && !trained.heavy)))
    warnings.push("Not trained in this armor: Disadvantage on Str and Dex rolls, and no spellcasting.");
  if (b.shield && trained && !trained.shield) warnings.push("Not trained with shields: it gives no AC bonus (not counted).");
  if (strShort) warnings.push("Strength too low for this armor: Speed −10 ft (included).");
  if (armor?.stealth === "Disadvantage") warnings.push("This armor gives Disadvantage on Stealth.");
  if (b.class === "Monk" && !monkOK) warnings.push("Monks lose Unarmored Defense, Unarmored Movement and Martial Arts in armor or with a shield.");
  if (fightingStyleAllowed(b, p) && !b.fighting_style) warnings.push("Pick your Fighting Style.");
  if (species?.extra_origin_feat && !b.human_feat) warnings.push(`Pick the ${species.name}'s extra Origin feat.`);
  if (needsLineage(p) && !lineage) warnings.push(`Pick your ${(species!.choice_label ?? "lineage").toLowerCase()}.`);
  if (species?.extra_origin_feat && b.human_feat && b.human_feat === background?.feat && b.human_feat !== "Skilled" && !b.human_feat.startsWith("Magic Initiate"))
    warnings.push(`Your background already gives ${b.human_feat}: pick a different extra feat (only Skilled and Magic Initiate can be taken twice).`);
  if (abilitiesCheck !== "OK") warnings.push(`Ability scores: ${abilitiesCheck}.`);
  if (backgroundCheck !== "OK") warnings.push(`Background bonuses: ${backgroundCheck}.`);
  if (!pickOK) warnings.push(`Skill picks: ${pickText}`);
  for (const w of weapons) if (!w.proficient) warnings.push(`Not proficient with the ${w.name}: no Proficiency Bonus on its attack.`);

  return {
    pb,
    scores: sc,
    mods,
    scoreHow,
    saves: cls?.saves ?? [],
    skills,
    skillPicks: { used: picks.length, allowed, text: pickText, ok: pickOK },
    jack,
    ac: { value: acValue, how: `AC ${acValue} = ${acHow}` },
    initiative: { value: initiative, how: initHow },
    hp: { value: hpValue, how: hpHow },
    speed: { value: speedValue, how: speedHow },
    darkvision: { value: dark, how: darkHow },
    passivePerception: { value: 10 + perception, how: ppHow },
    spell,
    attacksPerAction: { value: apa, how: apaHow },
    masteries: { value: masteries, how: masteryHow },
    weapons,
    unarmed,
    resources,
    cantrips: row?.cantrips ?? 0,
    prepared: row?.prepared ?? 0,
    slots: row?.slots ?? [0, 0, 0, 0, 0, 0, 0, 0, 0],
    pact: { slots: row?.pact_slots ?? 0, level: row?.pact_level ?? 0 },
    carry: { value: carry, how: carryHow },
    features,
    proficiencies,
    checks: { abilities: abilitiesCheck, background: backgroundCheck, points: pointsText },
    warnings,
    hitDie: die,
  };
}

function resourceHow(max: ResourceMax, name: string, value: number, b: Build, cls: ClassRules | undefined, pb: number, mods: Record<Ability, number>) {
  if ("ability_mod" in max) return `${name}: uses = ${ABILITY_NAMES[max.ability_mod]} modifier ${signed(mods[max.ability_mod])} (at least ${max.min})`;
  if ("per_level" in max) return `${name}: pool = ${max.per_level} × level ${b.level}`;
  if ("proficiency_bonus" in max) return `${name}: uses = Proficiency Bonus ${signed(pb)}`;
  if ("fixed" in max) return name === "Arcane Recovery"
    ? `${name}: once a day after a Short Rest, regain spell slots totalling up to half your Wizard level (rounded up), none 6th level or higher`
    : `${name}: ${value}`;
  return `${name}: ${value}, from the ${cls?.name ?? "class"} level table`;
}

// ------------------------------------------------------------------ writing it to the character

/**
 * Copy what the builder works out into the character's own fields, so everything else (the DM
 * screen, encounters, rests, exports) keeps reading the usual places. Current HP, used slots and
 * spent resources are kept, within the new maximums.
 */
export function apply(ch: Character, b: Build, rules: Rules): Character {
  const p = parts(b, rules);
  // A class missing from the rules data (hidden, or from another install): change nothing.
  if (!p.cls) return ch;
  const s = work(b, rules);
  const out: Character = structuredClone(ch);
  const before: Made = (ch.build as { made?: Made }).made ?? { features: [], attacks: [], resources: [] };
  out.classes = [{ name: b.class, subclass: b.subclass, level: b.level }];
  out.race = p.lineage ? `${b.species} (${p.lineage.name})` : b.species;
  out.background = b.background;
  out.abilities = s.scores;
  out.save_proficiencies = [...s.saves];
  out.skills = Object.fromEntries(Object.entries(s.skills).filter(([, v]) => v.prof === "proficient" || v.prof === "expertise").map(([k, v]) => [k, v.prof]));
  out.jack_of_all_trades = s.jack;
  out.ac = s.ac.value;
  out.initiative_misc = s.initiative.value - s.mods.dex;
  out.speed = s.speed.value;
  // Damage taken stays the same when the maximum changes (and 0 HP stays 0), so HP never
  // depends on the order of edits.
  const damage = Math.max(0, ch.hp.max - ch.hp.current);
  const current = ch.hp.max <= 0 ? s.hp.value : ch.hp.current <= 0 ? 0 : Math.max(0, s.hp.value - damage);
  out.hp = { ...ch.hp, max: s.hp.value, current: Math.min(s.hp.value, current) };
  out.hit_dice = { total: `${b.level}d${s.hitDie}`, remaining: Math.min(b.level, ch.hit_dice.remaining + Math.max(0, b.level - totalDice(ch.hit_dice.total))) };
  out.proficiencies = { ...s.proficiencies, languages: ch.proficiencies.languages && ch.proficiencies.languages !== "Common" ? ch.proficiencies.languages : s.proficiencies.languages };

  // Rows the builder makes replace the ones it made last time (and same-named ones); the
  // player's own rows stay.
  const attack = (w: BuiltWeapon): Attack => ({ name: w.name, bonus: signed(w.attack), damage: w.damage, notes: [w.mastery && `Mastery: ${w.mastery}`, w.notes].filter(Boolean).join(" · ") });
  const attacks = [...s.weapons.map(attack), attack(s.unarmed)];
  const attackNames = new Set([...before.attacks, ...attacks.map((a) => a.name)]);
  out.attacks = [...attacks, ...ch.attacks.filter((a) => !attackNames.has(a.name))];
  out.spellcasting.ability = s.spell?.ability ?? "";
  const slots = s.pact.slots ? Array.from({ length: 9 }, (_, i) => (i + 1 === s.pact.level ? s.pact.slots : 0)) : s.slots;
  for (let lvl = 1; lvl <= 9; lvl++) {
    const max = slots[lvl - 1] || 0;
    const used = Math.min(ch.spellcasting.slots[lvl]?.used || 0, max);
    if (max || ch.spellcasting.slots[lvl]) out.spellcasting.slots[lvl] = { max, used };
  }
  const resourceNames = new Set([...before.resources, ...s.resources.map((r) => r.name)]);
  out.resources = [
    ...s.resources.map((r) => {
      const old = ch.resources.find((x) => x.name === r.name);
      return { name: r.name, max: r.max, reset: r.reset, current: old ? Math.min(old.current, r.max) : r.max };
    }),
    ...ch.resources.filter((r) => !resourceNames.has(r.name)),
  ];
  const key = (f: Feature) => `${f.name}|${f.source}`;
  const featureKeys = new Set([...before.features, ...s.features.map(key)]);
  out.features = [
    ...s.features.map((f) => ({ ...f, description: ch.features.find((x) => key(x) === key(f))?.description || f.description })),
    ...ch.features.filter((f) => !featureKeys.has(key(f))),
  ];
  const made: Made = { features: s.features.map(key), attacks: attacks.map((a) => a.name), resources: s.resources.map((r) => r.name) };
  out.build = { ...b, made } as unknown as Record<string, unknown>;
  return out;
}

/** The attacks and resources the builder made, as opposed to ones the player added. */
export const madeBy = (ch: Character): Made => (ch.build as { made?: Made }).made ?? { features: [], attacks: [], resources: [] };

const totalDice = (text: string) => (String(text).match(/(\d+)d\d+/g) ?? []).reduce((t, part) => t + parseInt(part, 10), 0);

// ------------------------------------------------------------------ level up

export interface LevelUp { build: Build; entry: LevelLogEntry }

/** Level up by one: average HP, or a rolled Hit Die (the difference goes into extra HP). */
export function levelUp(b: Build, rules: Rules, rolled: number | null): LevelUp | null {
  if (b.level >= 20) return null;
  const before = work(b, rules);
  const next: Build = { ...b, level: b.level + 1 };
  const die = before.hitDie;
  const con = before.mods.con;
  if (rolled !== null) next.extra_hp = b.extra_hp + (Math.max(1, rolled + con) - (die / 2 + 1 + con));
  const after = work(next, rules);
  const row = parts(next, rules).row;
  const feats = row?.features ?? [];
  const hp = after.hp.value - before.hp.value;
  const gained: string[] = [`New features: ${feats.join(", ") || "none"}`, `Hit Point maximum: ${before.hp.value} → ${after.hp.value} (+${hp}${rolled !== null ? `, you rolled ${rolled}` : ""})`];
  if (after.pb !== before.pb) gained.push(`Proficiency Bonus is now ${signed(after.pb)} (every proficient roll goes up)`);
  if (after.attacksPerAction.value !== before.attacksPerAction.value) gained.push(`Attacks per Attack action: ${after.attacksPerAction.value}`);
  const slotText = (s: Sheet) => (s.pact.slots ? `${s.pact.slots} pact slots, level ${s.pact.level}` : s.slots.map((n, i) => (n ? `${ordinal(i + 1)}: ${n}` : "")).filter(Boolean).join(", ") || "none");
  if (slotText(after) !== slotText(before)) gained.push(`Spell slots: ${slotText(after)}`);
  for (const r of after.resources) {
    const old = before.resources.find((x) => x.name === r.name);
    if (!old || old.max !== r.max) gained.push(`${r.name}: ${r.max}`);
  }
  const todo: string[] = [];
  const has = (text: string) => feats.some((f) => f.toLowerCase().includes(text.toLowerCase()));
  if (has("Ability Score Improvement")) todo.push("Ability Score Improvement: +2 to one score or +1 to two (the Other + column), or take another feat you qualify for.");
  if (has("Epic Boon")) todo.push("Choose an Epic Boon feat.");
  if (has("Subclass") && !has("Subclass feature")) todo.push("Choose your subclass.");
  if (has("Subclass feature")) todo.push("Read your new subclass feature in the reference.");
  if (has("Expertise")) todo.push("Expertise: set two of your proficient skills to Expertise.");
  if (has("Fighting Style")) todo.push("Choose a Fighting Style.");
  if (after.cantrips > before.cantrips) todo.push(`Learn ${after.cantrips - before.cantrips} new cantrip(s).`);
  if (after.prepared > before.prepared) todo.push(`Prepare ${after.prepared - before.prepared} more spell(s) (${after.prepared} total).`);
  if (after.masteries.value > before.masteries.value) todo.push(`Pick ${after.masteries.value - before.masteries.value} more weapon mastery.`);
  todo.push(`Read the details of each new feature in the reference (${b.class}).`);
  const entry: LevelLogEntry = { date: new Date().toISOString().slice(0, 10), level: next.level, hp, gained, todo };
  next.level_log = [...b.level_log, entry];
  return { build: next, entry };
}

export const ordinal = (n: number) => `${n}${n === 1 ? "st" : n === 2 ? "nd" : n === 3 ? "rd" : "th"}`;

// ------------------------------------------------------------------ starting gear

export interface GearFill { armor: string; shield: boolean; weapons: string[]; items: { name: string; qty: number; weight: number }[]; gp: number; sp: number; notes: string[] }

/** What a class option and a background option give, sorted into armor, weapons, items and coins. */
export function startingGear(b: Build, rules: Rules, classOption: string, backgroundOption: string): GearFill {
  const p = parts(b, rules);
  const lines = [...(p.cls?.starting_gear[classOption] ?? []), ...(p.background?.starting_gear[backgroundOption] ?? [])];
  const fill: GearFill = { armor: "None (unarmored)", shield: false, weapons: [], items: [], gp: 0, sp: 0, notes: [] };
  for (const { item, qty } of lines) {
    const base = item.includes(" (") ? item.slice(0, item.indexOf(" (")) : item;
    if (item === "GP") fill.gp += qty;
    else if (item === "SP") fill.sp += qty;
    else if (item === "Shield") fill.shield = true;
    else if (rules.armor.some((a) => a.name === item)) fill.armor = item;
    else if (rules.weapons.some((w) => w.name === base)) {
      const w = rules.weapons.find((x) => x.name === base)!;
      for (let i = 0; i < qty; i++) fill.weapons.push(base);
      if (base !== item) fill.notes.push(`${base}: ${item.slice(base.length + 2, -1)}.`);
      const existing = fill.items.find((x) => x.name === base);
      if (existing) existing.qty += qty;
      else fill.items.push({ name: base, qty, weight: parseFloat(w.weight) || 0 });
    } else {
      const known = rules.items.find((x) => x.name === item) ?? rules.items.find((x) => x.name === base);
      const existing = fill.items.find((x) => x.name === item);
      if (existing) existing.qty += qty;
      else fill.items.push({ name: item, qty, weight: known?.weight ?? 0 });
    }
  }
  // A weapon carried several times (8 javelins) is one attack row.
  fill.weapons = [...new Set(fill.weapons)];
  return fill;
}

/** The weight of an item: its exact name, or the part before " (" ("Quarterstaff (your Arcane Focus)"). */
export function itemWeight(rules: Rules, name: string): number | null {
  const base = name.includes(" (") ? name.slice(0, name.indexOf(" (")) : name;
  const item = rules.items.find((x) => x.name === name) ?? rules.items.find((x) => x.name === base);
  if (item) return item.weight;
  const w = rules.weapons.find((x) => x.name === name || x.name === base);
  return w ? parseFloat(w.weight) || 0 : null;
}

// ------------------------------------------------------------------ spells

/** How to choose spells and how spells change, for the class (from the spreadsheet's Spells tab). */
export function spellGuide(b: Build, s: Sheet): string[] {
  if (!s.spell) return [
    `${b.class ? `${b.class}s don't` : "This class doesn't"} cast spells. If a feat such as Magic Initiate gives you spells, add them below and note where they came from.`,
    "Magic Initiate: 2 cantrips + 1 level 1 spell from that list; the level 1 spell is always prepared and can be cast once per Long Rest without a slot.",
  ];
  const highest = s.pact.slots ? s.pact.level : s.slots.reduce((h, n, i) => (n > 0 ? i + 1 : h), 0);
  const change = ["Bard", "Sorcerer", "Warlock"].includes(b.class) ? "When you gain a level you can swap one prepared spell (and one cantrip)."
    : ["Paladin", "Ranger"].includes(b.class) ? "After a Long Rest you can swap one prepared spell; on a level up you can swap a cantrip if you have any."
    : b.class === "Wizard" ? "After a Long Rest you can change any prepared spells, choosing from your spellbook."
    : "After a Long Rest you can change any of your prepared spells.";
  const always = [
    b.class === "Druid" ? "Speak with Animals" : "",
    b.class === "Paladin" && b.level >= 2 ? `Divine Smite${b.level >= 5 ? ", Find Steed" : ""}` : "",
    b.class === "Ranger" ? "Hunter's Mark" : "",
    b.class === "Warlock" && b.level >= 9 ? "Contact Other Plane" : "",
  ].filter(Boolean);
  return [
    s.cantrips ? `Cantrips: pick ${s.cantrips} level 0 spells from the ${b.class} list. Casting them costs nothing.` : "",
    `Prepared spells: pick ${s.prepared} spells of level 1${highest > 1 ? `–${highest}` : ""} from the ${b.class} list and mark them prepared. ${change}`,
    b.class === "Wizard" ? "Wizard: list every spell in your spellbook (6 level 1 spells at level 1, +2 each level); only the ones marked prepared count." : "",
    `Casting a level 1+ spell uses a slot of that level or higher; slots come back on a Long Rest${b.class === "Warlock" ? " (Pact slots also on a Short Rest)" : ""}.`,
    always.length ? `Always prepared (they don't count toward your number): ${always.join(", ")}.` : "",
  ].filter(Boolean);
}

export const highestSpellLevel = (s: Sheet) => (s.pact.slots ? s.pact.level : s.slots.reduce((h, n, i) => (n > 0 ? i + 1 : h), 0));
