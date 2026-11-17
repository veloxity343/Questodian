// Mirrors crates/core/src/schemas.rs.

export type Kind =
  | "campaigns" | "sessions" | "characters" | "handbook"
  | "scenes" | "npcs" | "monsters" | "notes" | "encounters" | "trackers" | "handouts";

export type Edition = "2014" | "2024";

export interface RecordBase {
  id: string;
  created: string;
  updated: string;
}

export interface Player {
  name: string;
  contact: string;
  character_id: string;
  notes: string;
}

export interface Campaign extends RecordBase {
  name: string;
  system: string;
  edition: Edition;
  description: string;
  setting: string;
  players: Player[];
  notes: string;
}

export type CombatantKind = "monster" | "pc" | "npc";

export interface Combatant {
  id: string;
  name: string;
  kind: CombatantKind;
  initiative: number;
  init_bonus: number;
  ac: number;
  hp: number;
  max_hp: number;
  temp_hp: number;
  conditions: string[];
  notes: string;
  character_id: string;
  srd_id: string;
  monster_id: string;
  npc_id: string;
  xp: number;
  /** Legendary monsters are worth more fought in their lair (0: no lair value). */
  xp_in_lair: number;
  in_lair: boolean;
  /** Caught unawares when combat starts (cleared when it ends). */
  surprised: boolean;
  hidden: boolean;
}

export interface LogEntry {
  id: string;
  time: string;
  kind: string;
  text: string;
}

export interface LootItem {
  name: string;
  qty: number;
  value: string;
  assigned_to: string;
}

export type SessionStatus = "planned" | "active" | "done";

export interface Session extends RecordBase {
  campaign_id: string;
  number: number;
  title: string;
  date: string;
  status: SessionStatus;
  attendance: string[];
  prep: string;
  recap: string;
  log: LogEntry[];
  loot: LootItem[];
  xp_awarded: number;
}

export type Ability = "str" | "dex" | "con" | "int" | "wis" | "cha";
export type SkillProficiency = "none" | "half" | "proficient" | "expertise";

export interface ClassLevel {
  name: string;
  subclass: string;
  level: number;
}

export interface Attack {
  name: string;
  bonus: string;
  damage: string;
  notes: string;
}

export interface SpellSlot {
  max: number;
  used: number;
}

export interface Spell {
  name: string;
  level: number;
  prepared: boolean;
  notes: string;
  srd_id: string;
}

export interface Resource {
  name: string;
  current: number;
  max: number;
  reset: "long" | "short" | "short_one" | "none";
}

export interface Item {
  name: string;
  qty: number;
  weight: number;
  equipped: boolean;
  notes: string;
}

export interface Feature {
  name: string;
  source: string;
  description: string;
}

export type Coin = "cp" | "sp" | "ep" | "gp" | "pp";

export interface Character extends RecordBase {
  name: string;
  player: string;
  campaign_id: string;
  race: string;
  background: string;
  alignment: string;
  classes: ClassLevel[];
  xp: number;
  abilities: Record<Ability, number>;
  save_proficiencies: Ability[];
  skills: Record<string, SkillProficiency>;
  jack_of_all_trades: boolean;
  ac: number;
  initiative_misc: number;
  speed: number;
  hp: { max: number; current: number; temp: number };
  hit_dice: { total: string; remaining: number };
  death_saves: { successes: number; failures: number };
  inspiration: boolean;
  conditions: string[];
  exhaustion: number;
  proficiencies: { armor: string; weapons: string; tools: string; languages: string };
  attacks: Attack[];
  spellcasting: { ability: string; slots: Record<string, SpellSlot>; spells: Spell[] };
  resources: Resource[];
  inventory: Item[];
  currency: Record<Coin, number>;
  features: Feature[];
  personality: { traits: string; ideals: string; bonds: string; flaws: string };
  appearance: string;
  backstory: string;
  notes: string;
  /** The character builder's choices (see lib/builder.ts); empty for sheets filled in by hand. */
  build: Record<string, unknown>;
}

export interface HandbookEntry extends RecordBase {
  title: string;
  category: string;
  tags: string[];
  campaign_id: string;
  body: string;
}

export type SceneStatus = "planned" | "active" | "completed" | "skipped";

export interface Scene extends RecordBase {
  campaign_id: string;
  session_id: string;
  position: number;
  title: string;
  location: string;
  status: SceneStatus;
  summary: string;
  read_aloud: string;
  body: string;
}

export interface Npc extends RecordBase {
  campaign_id: string;
  name: string;
  role: string;
  first_appears: string;
  looks: string;
  voice: string;
  wants: string;
  secret: string;
  knows: string;
  stats: string;
  stat_ref: string;
  notes: string;
  tags: string[];
}

export interface Monster extends RecordBase {
  campaign_id: string;
  name: string;
  kind: string;
  cr: string;
  xp: number;
  ac: number;
  hp: number;
  hp_roll: string;
  init_bonus: number;
  speed: string;
  abilities: Record<Ability, number>;
  body: string;
  notes: string;
}

export type NoteKind = "general" | "session" | "quest" | "pin";

export interface Note extends RecordBase {
  campaign_id: string;
  session_id: string;
  kind: NoteKind;
  title: string;
  body: string;
  pinned: boolean;
  done: boolean;
}

export interface Encounter extends RecordBase {
  campaign_id: string;
  session_id: string;
  scene_id: string;
  name: string;
  round: number;
  turn: number;
  active: boolean;
  combatants: Combatant[];
  party_size: number;
  party_level: number;
  log: LogEntry[];
  notes: string;
}

export interface Tracker extends RecordBase {
  campaign_id: string;
  session_id: string;
  name: string;
  value: number;
  max: number;
  steps: { label: string }[];
  notes: string;
}

export interface Handout extends RecordBase {
  campaign_id: string;
  folder: string;
  title: string;
  filename: string;
  mime: string;
  body: string;
}

export interface KindMap {
  campaigns: Campaign;
  sessions: Session;
  characters: Character;
  handbook: HandbookEntry;
  scenes: Scene;
  npcs: Npc;
  monsters: Monster;
  notes: Note;
  encounters: Encounter;
  trackers: Tracker;
  handouts: Handout;
}

export interface RollResult {
  expression: string;
  total: number;
  breakdown: string;
  groups: unknown[];
  critical: boolean;
  fumble: boolean;
}

export interface RollEntry extends RollResult {
  label: string;
  time: string;
}

export interface SrdEntry {
  id: string;
  name: string;
  category: string;
  summary: string;
  tags: string[];
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  data: Record<string, any>;
  body?: string;
}

export interface SrdCategory {
  id: string;
  label: string;
  count: number;
}

export interface SrdMeta {
  attribution: string;
  source: string;
  edition: Edition;
  edition_label: string;
}

export interface Meta {
  version: string;
  data_dir: string;
  database: string;
  srd: SrdMeta;
  editions: Record<Edition, string>;
  xp_budget: [number, number, number][];
  abilities: Record<Ability, string>;
  skills: Record<string, Ability>;
  conditions: string[];
  hit_dice: Record<string, number>;
  xp_by_level: number[];
  cr_xp: Record<string, number>;
  spell_slots: number[][];
  point_buy: { cost: Record<string, number>; budget: number };
  standard_array: number[];
}

export interface ImportSummary {
  imported: Partial<Record<Kind, number>>;
  skipped: Partial<Record<Kind, number>>;
  mode: string;
}

export type EncounterAction =
  | { action: "add"; combatant: Partial<Combatant> }
  | { action: "add_monster"; srd_id?: string; monster_id?: string; count?: number; average_hp?: boolean }
  | { action: "add_character"; character_ids: string[] }
  | { action: "add_npc"; npc_id: string }
  | { action: "update"; id: string; changes: Partial<Combatant> }
  | { action: "remove"; id: string }
  | { action: "hp"; id: string; amount: number }
  | { action: "roll_initiative"; which: "all" | "monsters" | "unset"; group?: boolean }
  | { action: "roll_hp" }
  | { action: "sort" | "next" | "prev" | "end" }
  | { action: "clear"; all?: boolean };
