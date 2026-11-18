import { useEffect, useRef, useState, type ReactNode } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { api } from "../api";
import { SrdPicker, useStatblock } from "../components/srd";
import {
  Checkbox, Conditions, EmptyState, Field, HpBar, ListSection, LoadError, Loading, NumInput, SaveStatus, Select, TextArea, TextInput,
  type Option,
} from "../components/ui";
import { useApp } from "../context/app";
import { useDice } from "../context/dice";
import { Modal, useDialogs, useToast } from "../context/ui";
import { derive, type Derived } from "../lib/derive";
import { useDraft, useLoad, usePref } from "../lib/hooks";
import { ABILITIES, ABILITY_NAMES, download, downloadJson, pickFiles, signed, slug, titleCase } from "../lib/util";
import { targetEncounter } from "../lib/encounters";
import { Board } from "../components/board";
import { apply, buildOf, extraSpell, highestSpellLevel, isBuilt, itemWeight, madeBy, parts, resetText, spellCounts, startBuild, startNotes, work, type Build, type GearFill, type Rules, type Sheet as Built } from "../lib/builder";
import {
  BuilderAbilities, BuilderBanner, BuilderGuard, BuilderHeader, BuilderHow, BuilderResources, BuilderSkills, BuilderSpellNotes, BuilderTiles, BuilderWarnings,
  BuilderWeapons, LevelLog, LevelUpDialog, RulesDataLink, StartingGearDialog, loadRules, useRules, type EditBuild,
} from "./builder";
import type { Ability, Campaign, Character, Edition, Resource, RollResult, SkillProficiency, SrdEntry } from "../types";

const ALIGNMENTS = ["", "Lawful Good", "Neutral Good", "Chaotic Good", "Lawful Neutral", "True Neutral", "Chaotic Neutral", "Lawful Evil", "Neutral Evil", "Chaotic Evil", "Unaligned"];
const PROF_CYCLE: SkillProficiency[] = ["none", "proficient", "expertise", "half"];
const PROF_ICON: Record<SkillProficiency, string> = { none: "○", half: "◐", proficient: "●", expertise: "◉" };
const PROF_LABEL: Record<SkillProficiency, string> = { none: "Not proficient", half: "Half proficiency", proficient: "Proficient", expertise: "Expertise" };

const d20 = (bonus: number) => `d20${signed(bonus)}`;
const normBonus = (b: string) => {
  const n = parseInt(String(b).replace(/\s/g, ""), 10);
  return isNaN(n) ? "+0" : signed(n);
};

interface SrdLists {
  classes: SrdEntry[];
  races: SrdEntry[];
  backgrounds: SrdEntry[];
}
const srdCache: Partial<Record<Edition, Promise<SrdLists>>> = {};
function srdLists(edition: Edition) {
  // 2024 rules call races "species".
  const races = edition === "2024" ? "species" : "races";
  srdCache[edition] ??= Promise.all(["classes", races, "backgrounds"].map((category) => api.srdSearch({ category, limit: 50, edition }))).then(
    ([c, r, b]) => ({ classes: c.results, races: r.results, backgrounds: b.results }),
    (e: Error) => {
      delete srdCache[edition]; // try again next time instead of keeping the failure
      throw e;
    },
  );
  return srdCache[edition]!;
}

// ------------------------------------------------------------------ list

export function CharacterList() {
  const { campaignId, campaigns, meta } = useApp();
  const toast = useToast();
  const navigate = useNavigate();
  const [showAllPref, setShowAll] = usePref("chars.all", false);
  const showAll = !campaignId || showAllPref;
  const { data: all, error, reload } = useLoad(() => api.list("characters"), []);
  if (error) return <LoadError error={error} />;
  if (!all) return <Loading />;
  const list = showAll ? all : all.filter((c) => c.campaign_id === campaignId);

  // New characters start in the builder (point buy at 8s); "Edit by hand" on the sheet turns it off.
  const create = async () => {
    const ch = await api.create("characters", { campaign_id: campaignId });
    try {
      const rules = await loadRules();
      await api.update("characters", ch.id, apply(ch, startBuild(ch, rules), rules));
    } catch (e) {
      toast(`Opened a blank sheet: ${(e as Error).message}`, "error");
    }
    navigate(`/characters/${ch.id}`);
  };
  const importChar = async () => {
    for (const f of await pickFiles({ accept: ".json,application/json", multiple: true })) {
      try {
        const r = await api.importData(JSON.parse(f.text), "copy");
        toast(`${f.name}: imported ${Object.entries(r.imported).map(([k, v]) => `${v} ${k}`).join(", ") || "nothing"}`);
      } catch (e) {
        toast(`${f.name}: ${(e as Error).message}`, "error");
      }
    }
    reload();
  };

  return (
    <>
      <header className="page-head">
        <h1>Characters</h1>
        <div className="row gap wrap">
          {campaignId && <label className="check small"><Checkbox checked={showAllPref} onChange={setShowAll} /> Show all campaigns</label>}
          <RulesDataLink />
          <button type="button" className="btn" onClick={importChar}>Import</button>
          <button type="button" className="btn primary" onClick={create}>+ New character</button>
        </div>
      </header>
      {list.length ? (
        <div className="cards">
          {list.map((ch) => {
            const camp = campaigns.find((c) => c.id === ch.campaign_id);
            const d = derive(ch, meta, camp?.edition);
            return (
              <Link key={ch.id} className="card char-card" to={`/characters/${ch.id}`}>
                <h3>{ch.name}</h3>
                <p className="muted">{[ch.race, ch.classes.map((c) => `${c.name} ${c.level}`).join(" / ")].filter(Boolean).join(" · ")}</p>
                <HpBar current={ch.hp.current} max={ch.hp.max} />
                <div className="card-meta">
                  <span>HP {ch.hp.current}/{ch.hp.max}</span>
                  <span>AC {ch.ac}</span>
                  <span>PP {d.passive.perception}</span>
                  {ch.player && <span>🎮 {ch.player}</span>}
                  {camp && <span className="tag">{camp.name}</span>}
                </div>
              </Link>
            );
          })}
        </div>
      ) : (
        <EmptyState title="No characters here yet" text="Create a sheet for each player character — or NPCs you want to track.">
          <button type="button" className="btn primary" onClick={create}>+ New character</button>
        </EmptyState>
      )}
    </>
  );
}

// ------------------------------------------------------------------ sheet

export function CharacterSheet() {
  const { id = "" } = useParams();
  const { data, error } = useLoad(() => Promise.all([api.get("characters", id), api.list("campaigns")]), [id]);
  if (error) return <LoadError error={error} />;
  if (!data) return <Loading />;
  const [ch, campaigns] = data;
  return <Sheet key={id} initial={ch} campaigns={campaigns} />;
}

type Edit = (fn: (d: Character) => void) => void;
type Roll = (expr: string, label: string) => void;

function Sheet({ initial, campaigns }: { initial: Character; campaigns: Campaign[] }) {
  const { meta } = useApp();
  const dice = useDice();
  const toast = useToast();
  const { confirm } = useDialogs();
  const navigate = useNavigate();
  const id = initial.id;
  const { doc: ch, setDoc, edit, flush, discard, status } = useDraft<Character>(initial, (doc) => api.update("characters", id, doc), {
    onError: (e) => toast(e.message, "error"),
  });
  const { rules } = useRules();
  const [dialog, setDialog] = useState<"" | "level" | "gear">("");
  // The builder (2024 rules) works out most of the sheet; "by hand" switches it off but keeps the choices.
  const wantsBuilder = !!rules && !!ch && isBuilt(ch) && !(ch.build as { manual?: boolean }).manual;
  // A class missing from the rules data (hidden, or from another install) can't be worked out.
  const missingClass = wantsBuilder && !parts(buildOf(ch!), rules!).cls ? buildOf(ch!).class : "";
  let building = wantsBuilder && !missingClass;
  const b = building ? buildOf(ch!) : null;
  // Rules data edited into a shape the builder can't read shows a message, not a broken page.
  let built: Built | null = null;
  let buildError = "";
  try {
    built = b && rules ? work(b, rules) : null;
  } catch (e) {
    buildError = (e as Error).message;
    building = false;
  }
  const editBuild: EditBuild = (fn) => {
    if (!rules) return;
    try {
      apply(ch!, fn(buildOf(ch!)), rules); // throws, before anything is saved, if the rules data can't be read
      edit((x) => void Object.assign(x, apply(x, fn(buildOf(x)), rules)));
    } catch (e) {
      toast(`The builder couldn't work that out: ${(e as Error).message}`, "error");
    }
  };
  // When the rules data changes (an edit on the Rules data page), bring the worked-out fields up to date.
  const syncedFor = useRef<Rules | null>(null);
  useEffect(() => {
    if (!building || !rules || !ch || syncedFor.current === rules) return;
    syncedFor.current = rules;
    try {
      if (JSON.stringify(apply(ch, buildOf(ch), rules)) !== JSON.stringify(ch)) edit((x) => void Object.assign(x, apply(x, buildOf(x), rules)));
    } catch {
      /* shown by the guard below */
    }
  }, [building, rules, ch, edit]);
  if (!ch) return null;
  // Rules follow the character's own campaign, which may not be the one open in the app.
  const d = derive(ch, meta, built ? "2024" : campaigns.find((c) => c.id === ch.campaign_id)?.edition);
  // 2024 exhaustion takes 2 per level off every D20 Test; the builder's numbers are before it.
  const tired = built ? 2 * Math.max(0, Math.min(6, Number(ch.exhaustion) || 0)) : 0;
  const roll: Roll = (expr, label) => void dice.roll(tired && expr.startsWith("d20") ? `${expr}-${tired}` : expr, `${ch.name}: ${label}`);
  const rollPlain: Roll = (expr, label) => void dice.roll(expr, `${ch.name}: ${label}`);

  const startBuilder = async () => {
    if (!rules || !ch) return;
    const next = isBuilt(ch) ? buildOf(ch) : startBuild(ch, rules);
    const notes = isBuilt(ch) ? [] : startNotes(ch, next);
    const ok = await confirm(
      "Switch this sheet to the builder? It works out ability scores, saves, skills, AC, HP, speed, attacks, resources, spell slots and features from your choices, replacing what's typed in those places now. Spells, inventory, coins, notes and anything you added yourself stay."
        + (notes.length ? ` Also: ${notes.join(" ")}` : ""),
      { label: "Use the builder", danger: false },
    );
    if (ok) edit((x) => void Object.assign(x, apply(x, next, rules)));
  };
  const byHand = async () => {
    const ok = await confirm("Edit this sheet by hand? Every field becomes editable and nothing is worked out any more. Your builder choices are kept, so you can switch back (which works everything out again).", { label: "Edit by hand", danger: false });
    if (ok) edit((x) => void (x.build = { ...x.build, manual: true }));
  };
  const fillGear = (f: GearFill) =>
    rules && edit((x) => {
      const next: Build = { ...buildOf(x), armor: f.armor, shield: f.shield, weapons: f.weapons };
      const worn = [f.armor !== "None (unarmored)" ? f.armor : "", f.shield ? "Shield" : ""].filter(Boolean);
      x.inventory = [
        ...worn.map((name) => ({ name, qty: 1, weight: itemWeight(rules, name) ?? 0, equipped: true, notes: "" })),
        ...f.items.map((i) => ({ name: i.name, qty: i.qty, weight: i.weight, equipped: f.weapons.includes(i.name), notes: "" })),
      ];
      x.currency = { cp: 0, sp: f.sp, ep: 0, gp: f.gp, pp: 0 };
      Object.assign(x, apply(x, next, rules));
      toast(`Starting gear filled: ${f.gp} GP${f.sp ? `, ${f.sp} SP` : ""}.${f.notes.length ? ` ${f.notes.join(" ")}` : ""}`);
    });

  const addToEncounter = async () => {
    await flush();
    const target = await targetEncounter(ch.campaign_id);
    if (!target) return toast("Link this character to a campaign that has an encounter first", "error");
    try {
      await api.encounter(target.id, { action: "add_character", character_ids: [id] });
      toast(`Added to ${target.name}`);
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };
  const duplicate = async () => {
    await flush();
    const { id: _id, created: _c, updated: _u, ...copy } = ch;
    const c = await api.create("characters", { ...copy, name: `${ch.name} (copy)` });
    navigate(`/characters/${c.id}`);
  };
  const remove = async () => {
    if (!(await confirm(`Delete ${ch.name}? This can't be undone (export first if unsure).`))) return;
    discard();
    await api.remove("characters", id);
    navigate("/characters");
  };
  const rest = async (type: "short" | "long") => {
    try {
      await flush();
      setDoc(await api.rest(id, type));
      const pact = ch.classes.length > 0 && ch.classes.every((c) => c.name.toLowerCase() === "warlock") ? " and Pact Magic slots" : "";
      toast(type === "long" ? "Long rest: HP, slots and resources restored" : `Short rest: short-rest resources${pact} restored`);
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };

  return (
    <>
      <header className="page-head">
        <Link to="/characters" className="small">← Characters</Link>
        <div className="row gap wrap">
          <SaveStatus status={status} />
          {building && <button type="button" className="btn" title="Turn off the builder for this sheet" onClick={byHand}>Edit by hand</button>}
          <button type="button" className="btn" title="Add to the campaign's running (or last edited) encounter" onClick={addToEncounter}>⚔️ To encounter</button>
          <button type="button" className="btn" onClick={async () => { await flush(); download(`${slug(ch.name, "character")}.md`, await api.exportMarkdown("characters", id), "text/markdown"); }}>Export .md</button>
          <button type="button" className="btn" onClick={async () => { await flush(); downloadJson(`${slug(ch.name, "character")}.character.json`, await api.exportOne("characters", id)); }}>Export .json</button>
          <button type="button" className="btn" onClick={duplicate}>Duplicate</button>
          <button type="button" className="btn danger" onClick={remove}>Delete</button>
        </div>
      </header>
      {built && b && rules ? (
        <BuilderGuard>
          <BuiltSheet ch={ch} b={b} s={built} rules={rules} d={d} edit={edit} editBuild={editBuild} campaigns={campaigns} roll={roll} rollPlain={rollPlain}
            rest={rest} openDialog={setDialog} />
        </BuilderGuard>
      ) : (
      <>
      {buildError ? (
        <p className="panel warnings">The builder can't work this sheet out: something in the rules data doesn't have the shape it expects ({buildError}). It's most likely an edit: open <Link to="/rules-data">Rules data</Link> and reset the entries or settings marked "edited". Until then the sheet is shown as it was last saved.</p>
      ) : missingClass ? (
        <p className="panel warnings">The builder is off for this sheet: its class, {missingClass}, isn't in the rules data. Add it back on the <Link to="/rules-data">Rules data</Link> page (or bring it back if it was hidden).</p>
      ) : rules && <BuilderBanner onStart={startBuilder} again={isBuilt(ch)} />}
      <Board id="sheet" items={[
        { id: "who", title: "Who you are", width: "full", node: <SheetHeader ch={ch} d={d} edit={edit} campaigns={campaigns} /> },
        { id: "abilities", title: "Abilities", node: <AbilitiesPanel ch={ch} d={d} edit={edit} roll={roll} /> },
        { id: "combat", title: "Combat", node: <CombatPanel ch={ch} d={d} edit={edit} roll={roll} rest={rest} /> },
        { id: "skills", title: "Skills", node: <SkillsPanel ch={ch} d={d} edit={edit} roll={roll} /> },
        { id: "attacks", title: "Attacks", node: <AttacksPanel ch={ch} edit={edit} roll={roll} /> },
        { id: "proficiencies", title: "Proficiencies", node: (
          <section className="panel">
            <h2>Proficiencies &amp; Languages</h2>
            <label className="check">
              <Checkbox checked={ch.jack_of_all_trades} onChange={(v) => edit((x) => void (x.jack_of_all_trades = v))} /> Jack of All Trades (half proficiency on other checks)
            </label>
            {(["armor", "weapons", "tools", "languages"] as const).map((k) => (
              <Field key={k} label={titleCase(k)}><TextInput value={ch.proficiencies[k]} onChange={(v) => edit((x) => void (x.proficiencies[k] = v))} /></Field>
            ))}
          </section>
        ) },
        { id: "resources", title: "Class resources", node: (
          <ListSection<Resource>
            title="Class resources"
            items={ch.resources}
            onChange={(v) => edit((x) => void (x.resources = v))}
            blank={() => ({ name: "", current: 1, max: 1, reset: "long" })}
            head={["Name", "Current", "Max", "Resets on", ""]}
            row={(r, set) => [
              <TextInput value={r.name} onChange={(v) => set({ name: v })} placeholder="Rage, Ki, Channel Divinity…" aria-label="Resource" />,
              <NumInput value={r.current} onChange={(v) => set({ current: v })} aria-label="Current" />,
              <NumInput value={r.max} onChange={(v) => set({ max: v })} aria-label="Max" />,
              <Select value={r.reset} options={RESETS} onChange={(v) => set({ reset: v as typeof r.reset })} aria-label="Resets on" />,
            ]}
          />
        ) },
        { id: "spells", title: "Spellcasting", node: <SpellsPanel ch={ch} d={d} edit={edit} roll={roll} /> },
        { id: "inventory", title: "Inventory", node: <InventoryPanel ch={ch} edit={edit} /> },
        { id: "coins", title: "Coins", node: <CoinsPanel ch={ch} edit={edit} /> },
        { id: "features", title: "Features & Traits", node: <FeaturesPanel ch={ch} edit={edit} /> },
        { id: "story", title: "Personality & Story", width: "full", node: <PersonalityPanel ch={ch} edit={edit} /> },
      ]} />
      </>
      )}
      {dialog === "level" && b && built && rules && (
        <LevelUpDialog b={b} s={built} rules={rules} onClose={() => setDialog("")} onDone={(next) => editBuild(() => next)} />
      )}
      {dialog === "gear" && b && rules && <StartingGearDialog b={b} rules={rules} onClose={() => setDialog("")} onFill={fillGear} />}
    </>
  );
}

/** The sheet when the builder works it out: choices at the top, numbers worked out, play state editable. */
function BuiltSheet({ ch, b, s, rules, d, edit, editBuild, campaigns, roll, rollPlain, rest, openDialog }: {
  ch: Character;
  b: Build;
  s: Built;
  rules: Rules;
  d: Derived;
  edit: Edit;
  editBuild: EditBuild;
  campaigns: Campaign[];
  roll: Roll;
  rollPlain: Roll;
  rest: (t: "short" | "long") => void;
  openDialog: (which: "level" | "gear") => void;
}) {
  const made = madeBy(ch);
  const builtAttacks = new Set(made.attacks);
  const builtResources = new Set(made.resources);
  const otherAttacks = ch.attacks.filter((a) => !builtAttacks.has(a.name));
  const otherResources = ch.resources.filter((r) => !builtResources.has(r.name));
  return (
    <Board id="built-sheet" items={[
      { id: "who", title: "Who you are", width: "full", node: <BuilderHeader ch={ch} b={b} s={s} rules={rules} edit={edit} editBuild={editBuild} campaigns={campaigns} onLevelUp={() => openDialog("level")} /> },
      { id: "warnings", title: "Warnings", width: "full", node: <BuilderWarnings s={s} /> },
      { id: "abilities", title: "Ability scores", node: <BuilderAbilities b={b} s={s} rules={rules} editBuild={editBuild} roll={roll} /> },
      { id: "combat", title: "Combat", node: <CombatPanel ch={ch} d={d} edit={edit} roll={roll} rest={rest} built={s} tiles={<BuilderTiles s={s} initiative={d.initiative} roll={rollPlain} />} /> },
      { id: "skills", title: "Skills", node: <BuilderSkills b={b} s={s} rules={rules} editBuild={editBuild} roll={roll} /> },
      { id: "weapons", title: "Weapons and armor", node: <BuilderWeapons b={b} s={s} rules={rules} editBuild={editBuild} roll={roll} /> },
      { id: "proficiencies", title: "Proficiencies", node: (
        <section className="panel">
          <h2>Proficiencies &amp; Languages</h2>
          <p className="small"><strong>Saving throws:</strong> {s.saves.map((a) => ABILITY_NAMES[a]).join(", ") || "—"}</p>
          {(["armor", "weapons", "tools"] as const).map((k) => <p key={k} className="small"><strong>{titleCase(k)}:</strong> {ch.proficiencies[k] || "—"}</p>)}
          <Field label="Languages"><TextInput value={ch.proficiencies.languages} onChange={(v) => edit((x) => void (x.proficiencies.languages = v))} /></Field>
        </section>
      ) },
      { id: "resources", title: "Class resources", node: <BuilderResources ch={ch} s={s} edit={edit} /> },
      { id: "spells", title: "Spellcasting", node: <SpellsPanel ch={ch} d={d} edit={edit} roll={rollPlain} built={s} b={b} /> },
      { id: "other-attacks", title: "Other attacks", node: (
        <AttacksPanel ch={{ ...ch, attacks: otherAttacks }} title="Other attacks & actions"
          edit={(fn) => edit((x) => {
            const draft = { ...x, attacks: x.attacks.filter((a) => !builtAttacks.has(a.name)) };
            fn(draft);
            x.attacks = [...x.attacks.filter((a) => builtAttacks.has(a.name)), ...draft.attacks];
          })} roll={roll} />
      ) },
      { id: "other-resources", title: "Other resources", node: (
        <ListSection<Resource>
          title="Other resources (magic items, feats…)"
          items={otherResources}
          onChange={(v) => edit((x) => void (x.resources = [...x.resources.filter((r) => builtResources.has(r.name)), ...v]))}
          blank={() => ({ name: "", current: 1, max: 1, reset: "long" })}
          head={["Name", "Current", "Max", "Resets on", ""]}
          row={(r, set) => [
            <TextInput value={r.name} onChange={(v) => set({ name: v })} aria-label="Resource" />,
            <NumInput value={r.current} onChange={(v) => set({ current: v })} aria-label="Current" />,
            <NumInput value={r.max} onChange={(v) => set({ max: v })} aria-label="Max" />,
            <Select value={r.reset} options={RESETS} onChange={(v) => set({ reset: v as typeof r.reset })} aria-label="Resets on" />,
          ]}
        />
      ) },
      { id: "inventory", title: "Inventory", node: <InventoryPanel ch={ch} edit={edit} rules={rules} capacity={s.carry.value} onStartingGear={() => openDialog("gear")} /> },
      { id: "coins", title: "Coins", node: <CoinsPanel ch={ch} edit={edit} /> },
      { id: "features", title: "Features & Traits", node: <FeaturesPanel ch={ch} edit={edit} /> },
      { id: "story", title: "Personality & Story", width: "full", node: <PersonalityPanel ch={ch} edit={edit} /> },
      { id: "level-log", title: "Level log", width: "full", node: <LevelLog b={b} /> },
      { id: "how", title: "How it's worked out", width: "full", node: <BuilderHow b={b} s={s} rules={rules} /> },
    ]} />
  );
}

const RESETS: Option[] = [["long", resetText("long")], ["short", resetText("short")], ["short_one", resetText("short_one")], ["none", resetText("none")]];

function CoinsPanel({ ch, edit }: { ch: Character; edit: Edit }) {
  return (
    <section className="panel">
      <h2>Coins</h2>
      <div className="row gap wrap currency">
        {(["cp", "sp", "ep", "gp", "pp"] as const).map((k) => (
          <label key={k} className="field coin"><span>{k.toUpperCase()}</span><NumInput min={0} value={ch.currency[k]} onChange={(v) => edit((x) => void (x.currency[k] = v))} /></label>
        ))}
      </div>
    </section>
  );
}

function FeaturesPanel({ ch, edit }: { ch: Character; edit: Edit }) {
  return (
    <ListSection
      title="Features & Traits"
      items={ch.features}
      onChange={(v) => edit((x) => void (x.features = v))}
      blank={() => ({ name: "", source: "", description: "" })}
      head={["Name", "Source", "Description", ""]}
      row={(f, set) => [
        <TextInput value={f.name} onChange={(v) => set({ name: v })} aria-label="Feature" />,
        <TextInput value={f.source} onChange={(v) => set({ source: v })} placeholder="Class, species, feat…" aria-label="Source" />,
        <TextArea rows={2} value={f.description} onChange={(v) => set({ description: v })} aria-label="Description" />,
      ]}
    />
  );
}

function PersonalityPanel({ ch, edit }: { ch: Character; edit: Edit }) {
  return (
    <section className="panel">
      <h2>Personality &amp; Story</h2>
      <div className="form-grid">
        {(["traits", "ideals", "bonds", "flaws"] as const).map((k) => (
          <Field key={k} label={titleCase(k)}><TextArea rows={2} value={ch.personality[k]} onChange={(v) => edit((x) => void (x.personality[k] = v))} /></Field>
        ))}
      </div>
      {(["appearance", "backstory", "notes"] as const).map((k) => (
        <Field key={k} label={titleCase(k)}><TextArea rows={k === "appearance" ? 2 : 5} value={ch[k]} onChange={(v) => edit((x) => void (x[k] = v))} /></Field>
      ))}
    </section>
  );
}

const NO_LISTS: SrdLists = { classes: [], races: [], backgrounds: [] };

function SheetHeader({ ch, d, edit, campaigns }: { ch: Character; d: Derived; edit: Edit; campaigns: Campaign[] }) {
  // Suggestions follow the character's own campaign edition, and switch when it's relinked.
  const lists = useLoad(() => srdLists(d.edition), [d.edition]).data ?? NO_LISTS;
  const { meta } = useApp();
  const next = meta?.xp_by_level?.[d.level];

  const setClass = (i: number, patch: Partial<Character["classes"][number]>) =>
    edit((x) => {
      Object.assign(x.classes[i], patch);
      if (patch.name !== undefined) {
        // Fill in what the SRD class implies, without overwriting anything already set.
        const entry = lists.classes.find((c) => c.name.toLowerCase() === String(patch.name).toLowerCase());
        if (entry) {
          if (i === 0 && !x.save_proficiencies.length) x.save_proficiencies = [...entry.data.saves];
          if (entry.data.spellcasting_ability && !x.spellcasting.ability) x.spellcasting.ability = entry.data.spellcasting_ability;
        }
      }
      if (patch.name !== undefined || patch.level !== undefined) {
        const parts: Record<number, number> = {};
        for (const k of x.classes) {
          const die = meta?.hit_dice?.[String(k.name).toLowerCase()];
          if (die) parts[die] = (parts[die] || 0) + (Number(k.level) || 0);
        }
        const txt = Object.entries(parts).sort((a, b) => Number(b[0]) - Number(a[0])).map(([die, n]) => `${n}d${die}`).join(" + ");
        if (txt) x.hit_dice.total = txt;
      }
    });

  return (
    <section className="panel sheet-header">
      <datalist id="dl-classes">{lists.classes.map((c) => <option key={c.id} value={c.name} />)}</datalist>
      <datalist id="dl-races">{lists.races.flatMap((r) => [r.name, ...(r.data.subraces || [])]).map((n) => <option key={n} value={n} />)}</datalist>
      <datalist id="dl-backgrounds">{lists.backgrounds.map((b) => <option key={b.id} value={b.name} />)}</datalist>
      <div className="row gap wrap">
        <TextInput className="title-input grow" value={ch.name} onChange={(v) => edit((x) => void (x.name = v))} aria-label="Character name" />
        <label className="check"><Checkbox checked={ch.inspiration} onChange={(v) => edit((x) => void (x.inspiration = v))} /> Inspiration</label>
      </div>
      <div className="form-grid">
        <Field label="Player"><TextInput value={ch.player} onChange={(v) => edit((x) => void (x.player = v))} /></Field>
        <Field label={d.edition === "2024" ? "Species" : "Race"}><TextInput list="dl-races" value={ch.race} onChange={(v) => edit((x) => void (x.race = v))} /></Field>
        <Field label="Background"><TextInput list="dl-backgrounds" value={ch.background} onChange={(v) => edit((x) => void (x.background = v))} /></Field>
        <Field label="Alignment"><Select value={ch.alignment} options={ALIGNMENTS.map((a): Option => [a, a || "—"])} onChange={(v) => edit((x) => void (x.alignment = v))} /></Field>
        <Field label="Campaign">
          <Select value={ch.campaign_id} options={[["", "— none —"], ...campaigns.map((c): Option => [c.id, c.name])]} onChange={(v) => edit((x) => void (x.campaign_id = v))} />
        </Field>
        <Field label="Experience"><NumInput min={0} value={ch.xp} onChange={(v) => edit((x) => void (x.xp = v))} /></Field>
      </div>
      <div className="row gap wrap level-line">
        <strong>Level {d.level}</strong>
        <span className="tag">Proficiency {signed(d.pb)}</span>
        <span className="muted small">{next ? `Next level at ${next.toLocaleString()} XP${ch.xp >= next ? " — level up!" : ""}` : "Max level"}</span>
      </div>
      <Field label="Classes">
        <div className="stack">
          {ch.classes.map((k, i) => (
            <div className="row gap class-row" key={i}>
              <TextInput list="dl-classes" placeholder="Class" aria-label="Class" value={k.name} onChange={(v) => setClass(i, { name: v })} />
              <TextInput placeholder="Subclass" aria-label="Subclass" value={k.subclass} onChange={(v) => setClass(i, { subclass: v })} />
              <NumInput min={1} max={20} aria-label="Level" value={k.level} onChange={(v) => setClass(i, { level: v })} />
              {ch.classes.length > 1 && (
                <button type="button" className="icon-btn" aria-label="Remove class" onClick={() => edit((x) => void x.classes.splice(i, 1))}>✕</button>
              )}
            </div>
          ))}
          <button type="button" className="link small" onClick={() => edit((x) => void x.classes.push({ name: "", subclass: "", level: 1 }))}>+ Multiclass</button>
        </div>
      </Field>
    </section>
  );
}

function AbilitiesPanel({ ch, d, edit, roll }: { ch: Character; d: Derived; edit: Edit; roll: Roll }) {
  const { meta } = useApp();
  const [rolled, setRolled] = useState<RollResult[] | null>(null);
  const cost = meta?.point_buy?.cost || {};
  const scores = ABILITIES.map((a) => Number(ch.abilities[a]));
  const pointBuy = scores.every((s) => s in cost)
    ? `Point buy: ${scores.reduce((t, s) => t + cost[s], 0)}/27`
    : `Point buy: n/a (scores must be 8–15 before ${d.edition === "2024" ? "background" : "racial"} increases)`;
  const toggleSave = (a: Ability, on: boolean) =>
    edit((x) => void (x.save_proficiencies = on ? [...new Set([...x.save_proficiencies, a])] : x.save_proficiencies.filter((s) => s !== a)));

  return (
    <section className="panel">
      <div className="panel-head"><h2>Abilities</h2></div>
      <div className="abilities">
        {ABILITIES.map((a) => (
          <div className="ability" key={a}>
            <div className="ability-name">{ABILITY_NAMES[a]}</div>
            <button type="button" className="ability-mod" title={`Roll ${ABILITY_NAMES[a]} check (${signed(d.checks[a])})`} onClick={() => roll(d20(d.checks[a]), `${ABILITY_NAMES[a]} check`)}>{signed(d.mods[a])}</button>
            <NumInput className="ability-score" min={1} max={30} aria-label={`${ABILITY_NAMES[a]} score`} value={ch.abilities[a]} onChange={(v) => edit((x) => void (x.abilities[a] = v))} />
            <div className="ability-save">
              <label className="check small" title="Proficient in this saving throw">
                <Checkbox checked={ch.save_proficiencies.includes(a)} onChange={(v) => toggleSave(a, v)} /> Save{" "}
              </label>
              <button type="button" className="link" onClick={() => roll(d20(d.saves[a]), `${ABILITY_NAMES[a]} save`)}>{signed(d.saves[a])}</button>
            </div>
          </div>
        ))}
      </div>
      <div className="row gap wrap">
        <button type="button" className="btn small" onClick={async () => setRolled((await api.stats()).results)}>🎲 Roll stats</button>
        <button type="button" className="btn small" onClick={() => edit((x) => ABILITIES.forEach((a, i) => (x.abilities[a] = [15, 14, 13, 12, 10, 8][i])))}>Standard array</button>
        <span className="muted small">{pointBuy}</span>
      </div>
      {rolled && (
        <Modal title="Rolled ability scores (4d6 drop lowest)" onClose={() => setRolled(null)} actions={[
          { label: "Cancel" },
          { label: "Apply", primary: true, onClick: () => edit((x) => ABILITIES.forEach((a, i) => (x.abilities[a] = rolled[i].total))) },
        ]}>
          <div className="stack">
            <div className="row gap wrap">{rolled.map((x, i) => <span key={i} className="tag big">{x.total}</span>)}</div>
            <p className="muted small">{rolled.map((x) => x.breakdown.replace(/~~/g, "")).join(" · ")}</p>
            <p>Total {rolled.reduce((a, b) => a + b.total, 0)}. Apply in order (Str → Cha)? You can rearrange afterwards.</p>
          </div>
        </Modal>
      )}
    </section>
  );
}

function SkillsPanel({ ch, d, edit, roll }: { ch: Character; d: Derived; edit: Edit; roll: Roll }) {
  const { meta } = useApp();
  return (
    <section className="panel">
      <div className="panel-head"><h2>Skills</h2><span className="muted small">○ none ● proficient ◉ expertise ◐ half</span></div>
      <ul className="skills">
        {Object.entries(meta?.skills || {}).map(([skill, ab]) => {
          const p = ch.skills[skill] || "none";
          return (
            <li key={skill} className={`skill prof-${p}`}>
              <button
                type="button"
                className="prof-toggle"
                title={`${PROF_LABEL[p]} — click to change`}
                aria-label={`${titleCase(skill)}: ${PROF_LABEL[p]}`}
                onClick={() => edit((x) => {
                  const next = PROF_CYCLE[(PROF_CYCLE.indexOf(p) + 1) % PROF_CYCLE.length];
                  if (next === "none") delete x.skills[skill];
                  else x.skills[skill] = next;
                })}
              >{PROF_ICON[p]}</button>
              <button type="button" className="link skill-name" onClick={() => roll(d20(d.skills[skill]), titleCase(skill))}>
                {titleCase(skill)}<span className="muted small"> ({ab.toUpperCase()})</span>
              </button>
              <strong className="skill-bonus">{signed(d.skills[skill])}</strong>
            </li>
          );
        })}
      </ul>
      <p className="muted small">Passive Perception {d.passive.perception} · Investigation {d.passive.investigation} · Insight {d.passive.insight}</p>
    </section>
  );
}

function StatBox({ label, children }: { label: string; children: ReactNode }) {
  return <div className="stat-box"><span>{label}</span>{children}</div>;
}

function CombatPanel({ ch, d, edit, roll, rest, built, tiles }: {
  ch: Character;
  d: Derived;
  edit: Edit;
  roll: Roll;
  rest: (t: "short" | "long") => void;
  /** With the builder: AC, speed, max HP and Hit Dice are worked out, shown in `tiles`. */
  built?: Built;
  tiles?: ReactNode;
}) {
  const toast = useToast();
  const [amount, setAmount] = useState("");
  const changeHp = (sign: number) => {
    const n = Math.abs(parseInt(amount, 10));
    if (!n) return;
    const dead = ch.death_saves.failures >= 3;
    const before = ch.hp.current;
    // Damage left after temp HP and reaching 0 (rules/damage-and-healing in both SRDs).
    const overflow = n - ch.hp.temp - before;
    edit((x) => {
      if (sign < 0) {
        const absorbed = Math.min(x.hp.temp, n);
        x.hp.temp -= absorbed;
        x.hp.current = Math.max(0, x.hp.current - (n - absorbed));
        if (before === 0 && overflow > 0) x.death_saves.failures = Math.min(3, x.death_saves.failures + 1);
      } else {
        x.hp.current = Math.min(x.hp.max || Infinity, x.hp.current + n);
        // A dead character (3 failures) keeps them: coming back takes revival magic.
        if (before === 0 && !dead) x.death_saves = { successes: 0, failures: 0 };
      }
    });
    if (sign < 0 && ch.hp.max && overflow >= ch.hp.max) {
      edit((x) => void (x.death_saves.failures = 3));
      toast("Massive damage: the remaining damage equals or exceeds max HP, so the character dies", "error");
    }
    else if (sign < 0 && before === 0 && overflow > 0) toast("Damage at 0 HP: one death save failure (two if it was a critical hit)");
    else if (sign > 0 && dead) toast("This character has 3 failed death saves; clear them if revival magic brought them back");
    setAmount("");
  };
  const spendHitDie = () => {
    const m = String(ch.hit_dice.total).match(/d(\d+)/);
    if (!m) return toast("Set your hit dice first (e.g. 3d10)", "error");
    if (ch.hit_dice.remaining < 1) return toast("No hit dice left", "error");
    // Hit dice are spent during a short rest, which needs 1 HP in 2024 (rules-glossary-short-rest).
    if (d.edition === "2024" && ch.hp.current < 1) return toast("A short rest needs at least 1 HP", "error");
    edit((x) => void (x.hit_dice.remaining -= 1));
    // 2024 rules: regain at least 1 HP (rules-glossary-short-rest). 2014 has no minimum.
    roll(d.edition === "2024" ? `max(1, d${m[1]}${signed(d.mods.con)})` : `d${m[1]}${signed(d.mods.con)}`, "Hit die");
  };
  const deathBoxes = (key: "successes" | "failures") => (
    <span className="row gap-sm">
      {[0, 1, 2].map((i) => (
        <Checkbox key={i} aria-label={`${key} ${i + 1}`} checked={ch.death_saves[key] > i} onChange={(v) => edit((x) => void (x.death_saves[key] = v ? i + 1 : i))} />
      ))}
    </span>
  );

  return (
    <section className="panel">
      <div className="panel-head">
        <h2>Combat</h2>
        <div className="row gap">
          <button type="button" className="btn small" onClick={() => rest("short")}>Short rest</button>
          <button type="button" className="btn small" onClick={() => rest("long")}>Long rest</button>
        </div>
      </div>
      {tiles ?? <div className="stat-boxes">
        <StatBox label="Armor Class"><NumInput className="num-input big-num" aria-label="Armor class" value={ch.ac} onChange={(v) => edit((x) => void (x.ac = v))} /></StatBox>
        <StatBox label="Initiative">
          <button type="button" className="big-num link" onClick={() => roll(d20(d.initiative), "Initiative")}>{signed(d.initiative)}</button>
          <label className="small muted">misc <NumInput className="num-input tiny" aria-label="Initiative misc bonus" value={ch.initiative_misc} onChange={(v) => edit((x) => void (x.initiative_misc = v))} /></label>
        </StatBox>
        <StatBox label="Speed">
          <NumInput className="num-input big-num" aria-label="Speed" value={ch.speed} onChange={(v) => edit((x) => void (x.speed = v))} />
          {d.speed !== Number(ch.speed) && <span className="small muted">{d.speed} ft. while exhausted</span>}
        </StatBox>
        <StatBox label="Passive Perception"><strong className="big-num">{d.passive.perception}</strong></StatBox>
      </div>}
      <div className="hp-block">
        <div className="row gap wrap">
          <label className="field"><span>Current HP</span><NumInput className="num-input big-num" aria-label="Current HP" value={ch.hp.current} onChange={(v) => edit((x) => void (x.hp.current = v))} /></label>
          {built
            ? <div className="field" title={built.hp.how}><span>Max HP</span><strong className="readout">{ch.hp.max}</strong></div>
            : <label className="field"><span>Max HP</span><NumInput value={ch.hp.max} onChange={(v) => edit((x) => void (x.hp.max = v))} /></label>}
          <label className="field"><span>Temp HP</span><NumInput aria-label="Temporary HP" value={ch.hp.temp} onChange={(v) => edit((x) => void (x.hp.temp = v))} /></label>
          <div className="field">
            <span>Damage / heal</span>
            <div className="row gap-sm">
              <input type="number" min={0} className="num-input" placeholder="0" aria-label="HP change" value={amount} onChange={(e) => setAmount(e.target.value)} />
              <button type="button" className="btn small danger" onClick={() => changeHp(-1)}>−</button>
              <button type="button" className="btn small ok" onClick={() => changeHp(1)}>+</button>
            </div>
          </div>
        </div>
        <div className="row gap wrap">
          {built
            ? <div className="field"><span>Hit dice</span><strong className="readout">{ch.hit_dice.total}</strong></div>
            : <label className="field"><span>Hit dice</span><TextInput placeholder="e.g. 5d10" aria-label="Hit dice total" value={ch.hit_dice.total} onChange={(v) => edit((x) => void (x.hit_dice.total = v))} /></label>}
          <label className="field"><span>Remaining</span><NumInput min={0} aria-label="Hit dice remaining" value={ch.hit_dice.remaining} onChange={(v) => edit((x) => void (x.hit_dice.remaining = v))} /></label>
          <div className="field"><span>Spend</span><button type="button" className="btn small" onClick={spendHitDie}>🎲 Roll hit die</button></div>
          <label className="field">
            <span>Exhaustion</span>
            <Select value={String(ch.exhaustion)} options={["0", "1", "2", "3", "4", "5", "6"]} onChange={(v) => edit((x) => void (x.exhaustion = Number(v)))} />
            {d.exhaustion.length > 0 && <span className="small muted">{d.exhaustion.join("; ")}</span>}
          </label>
        </div>
      </div>
      <Field label="Conditions"><Conditions value={ch.conditions} onChange={(v) => edit((x) => void (x.conditions = v))} /></Field>
      <div className="field">
        <span>Death saves</span>
        <div className="death-saves">
          <div><span className="small">Successes </span>{deathBoxes("successes")}</div>
          <div><span className="small">Failures </span>{deathBoxes("failures")}</div>
          <button type="button" className="btn small" onClick={() => roll("d20", "Death save")}>🎲 Death save</button>
        </div>
      </div>
    </section>
  );
}

function AttacksPanel({ ch, edit, roll, title = "Attacks & Actions" }: { ch: Character; edit: Edit; roll: Roll; title?: string }) {
  const toast = useToast();
  const rollDamage = (a: Character["attacks"][number], crit: boolean) => {
    const m = String(a.damage).match(/^[\d\sd+\-*()]+/i);
    if (!m) return toast("Damage should start with dice, e.g. 1d8+3", "error");
    let expr = m[0].trim().replace(/[+-]$/, "");
    if (crit) expr = expr.replace(/(\d*)d(\d+)/gi, (_, n, s) => `${(Number(n) || 1) * 2}d${s}`);
    roll(expr, `${a.name || "Attack"} damage${crit ? " (crit)" : ""}`);
  };
  return (
    <ListSection
      title={title}
      items={ch.attacks}
      onChange={(v) => edit((x) => void (x.attacks = v))}
      blank={() => ({ name: "", bonus: "+0", damage: "1d6", notes: "" })}
      head={["Name", "Attack", "Damage", "Notes", ""]}
      row={(a, set) => [
        <TextInput value={a.name} onChange={(v) => set({ name: v })} placeholder="Longsword" aria-label="Attack name" />,
        <div className="row gap-sm nowrap">
          <TextInput className="num-input" value={a.bonus} onChange={(v) => set({ bonus: v })} placeholder="+5" aria-label="Attack bonus" />
          <button type="button" className="btn small" title="Roll attack" onClick={() => roll(`d20${normBonus(a.bonus)}`, `${a.name || "Attack"} to hit`)}>🎲</button>
        </div>,
        <div className="row gap-sm nowrap">
          <TextInput value={a.damage} onChange={(v) => set({ damage: v })} placeholder="1d8+3 slashing" aria-label="Damage" />
          <button type="button" className="btn small" title="Roll damage" onClick={() => rollDamage(a, false)}>🎲</button>
          <button type="button" className="btn small" title="Roll critical damage" onClick={() => rollDamage(a, true)}>Crit</button>
        </div>,
        <TextInput value={a.notes} onChange={(v) => set({ notes: v })} placeholder="finesse, 5 ft." aria-label="Notes" />,
      ]}
    />
  );
}

function SpellsPanel({ ch, d, edit, roll, built, b }: { ch: Character; d: Derived; edit: Edit; roll: Roll; built?: Built; b?: Build }) {
  const toast = useToast();
  const showStatblock = useStatblock();
  const [picking, setPicking] = useState(false);
  const sc = ch.spellcasting;
  const slot = (lvl: number) => sc.slots[lvl] || { max: 0, used: 0 };
  // With the builder: only spells you can cast, and only as many as the class lets you know or prepare.
  // (A class without spells can still take Magic Initiate: cantrips and level 1.)
  const highest = built ? (built.spell ? highestSpellLevel(built) : 1) : 9;
  const counts = spellCounts(sc.spells);
  const refuseSpell = (e: SrdEntry) => {
    if (!built || !built.spell) return null;
    if (sc.spells.some((sp) => sp.srd_id === e.id)) return `${e.name} is already on your list`;
    if (e.data.level === 0 && counts.cantrips >= built.cantrips) return `You know ${built.cantrips} cantrips already: remove one first (or note a feat's as "Magic Initiate")`;
    return null;
  };
  const prepareLimit = (sp: Character["spellcasting"]["spells"][number]) => {
    if (!built || sp.prepared || extraSpell(sp)) return {};
    if (sp.level === 0) return { disabled: true, title: "Cantrips are always ready: no need to prepare them" };
    if (sp.level > highest) return { disabled: true, title: `You can cast up to level ${highest} spells` };
    if (built.spell && counts.prepared >= built.prepared) return { disabled: true, title: `You've prepared ${built.prepared}, your limit: unprepare one first` };
    return {};
  };
  const setSlot = (lvl: number, next: { max: number; used: number }) => edit((x) => void (x.spellcasting.slots[lvl] = next));
  const autoFill = async () => {
    try {
      const slots = await api.slots(ch.classes, d.edition);
      edit((x) => {
        for (let lvl = 1; lvl <= 9; lvl++) {
          const max = slots[lvl - 1] || 0;
          x.spellcasting.slots[lvl] = { max, used: Math.min(x.spellcasting.slots[lvl]?.used || 0, max) };
        }
      });
      toast(slots.length ? "Spell slots set from your class levels" : "No spell slots for these classes (for a multiclass warlock, add Pact Magic slots by hand)");
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };

  return (
    <section className="panel">
      <h2>Spellcasting</h2>
      <div className="row gap wrap">
        {built ? (
          <Field label="Spellcasting ability"><span className="readout">{built.spell ? ABILITY_NAMES[built.spell.ability] : "—"}</span></Field>
        ) : (
          <Field label="Spellcasting ability">
            <Select value={sc.ability} options={[["", "— none —"], ...ABILITIES.map((a): Option => [a, ABILITY_NAMES[a]])]} onChange={(v) => edit((x) => void (x.spellcasting.ability = v))} />
          </Field>
        )}
        <StatBox label="Save DC"><strong className="big-num">{d.spell ? d.spell.dc : "—"}</strong></StatBox>
        <StatBox label="Spell attack">
          <button type="button" className="link big-num" onClick={() => d.spell && roll(d20(d.spell.atk), "Spell attack")}>{d.spell ? signed(d.spell.atk) : "—"}</button>
        </StatBox>
        {!built && <button type="button" className="btn small" onClick={autoFill}>Auto-fill slots</button>}
      </div>
      {built && b && <BuilderSpellNotes b={b} s={built} ch={ch} />}
      <h3>Spell slots</h3>
      <div className="slots">
        {[1, 2, 3, 4, 5, 6, 7, 8, 9].map((lvl) => {
          const s = slot(lvl);
          return (
            <div key={lvl} className={`slot ${s.max ? "" : "empty-slot"}`}>
              <span className="slot-level">L{lvl}</span>
              {built ? <strong className="slot-max">{s.max}</strong> : (
                <NumInput className="num-input tiny" min={0} max={9} aria-label={`Level ${lvl} slots`} value={s.max}
                  onChange={(v) => setSlot(lvl, { max: Math.max(0, v), used: Math.min(s.used, Math.max(0, v)) })} />
              )}
              <div className="pips">
                {Array.from({ length: s.max }, (_, i) => (
                  <button type="button" key={i} className={`pip ${i < s.used ? "used" : ""}`} aria-label={`Level ${lvl} slot ${i + 1}${i < s.used ? " (used)" : ""}`}
                    onClick={() => setSlot(lvl, { ...s, used: i < s.used ? i : i + 1 })} />
                ))}
              </div>
            </div>
          );
        })}
      </div>
      <ListSection
        title="Spells"
        items={sc.spells}
        onChange={(v) => edit((x) => void (x.spellcasting.spells = [...v].sort((a, b) => a.level - b.level || a.name.localeCompare(b.name))))}
        blank={() => ({ name: "", level: 0, prepared: false, notes: "", srd_id: "" })}
        head={["Lvl", "Name", "Prep", "Notes", ""]}
        extra={<button type="button" className="btn small" onClick={() => setPicking(true)}>+ From SRD</button>}
        row={(sp, set) => [
          <NumInput className="num-input tiny" min={0} max={9} aria-label="Spell level" value={sp.level} onChange={(v) => set({ level: v })} />,
          <div className="row gap-sm">
            <TextInput value={sp.name} onChange={(v) => set({ name: v })} placeholder="Spell name" aria-label="Spell name" />
            {sp.srd_id && <button type="button" className="btn small" title="Read spell" onClick={() => showStatblock(sp.srd_id, sp.name)}>📖</button>}
          </div>,
          <Checkbox aria-label="Prepared" checked={sp.prepared} onChange={(v) => set({ prepared: v })} {...prepareLimit(sp)} />,
          <TextInput value={sp.notes} onChange={(v) => set({ notes: v })} aria-label="Notes" />,
        ]}
      />
      {picking && (
        <SrdPicker category="spells" className={b?.class ?? ch.classes[0]?.name} onClose={() => setPicking(false)}
          maxLevel={built ? highest : undefined} edition={built ? "2024" : undefined} refuse={built ? refuseSpell : undefined}
          note={built ? (built.spell
            ? `Showing cantrips${highest ? ` and spells up to level ${highest}` : ""}: what you can cast at level ${b?.level}. Cantrips ${counts.cantrips} of ${built.cantrips}; prepare up to ${built.prepared}.`
            : "Your class doesn't cast spells; this is for spells from a feat such as Magic Initiate (cantrips and level 1).") : undefined}
          onPick={(e) => edit((x) => void x.spellcasting.spells.push({ name: e.name, level: e.data.level, prepared: false, notes: e.summary, srd_id: e.id }))} />
      )}
    </section>
  );
}

function InventoryPanel({ ch, edit, rules, capacity, onStartingGear }: {
  ch: Character;
  edit: Edit;
  /** With the builder: item weights fill in from the rules data, and the starting gear button shows. */
  rules?: Rules;
  capacity?: number;
  onStartingGear?: () => void;
}) {
  const [picking, setPicking] = useState<"" | "equipment" | "magic-items">("");
  const weight = ch.inventory.reduce((t, i) => t + (Number(i.weight) || 0) * (Number(i.qty) || 0), 0);
  const coins = Object.values(ch.currency).reduce((a, b) => a + (Number(b) || 0), 0) / 50;
  const max = capacity ?? (Number(ch.abilities.str) || 10) * 15;
  const total = Math.round((weight + coins) * 10) / 10;
  const carrying = `Carrying ${total} / ${max} lb.${total > max ? " Over capacity: Speed drops to 5 ft." : ""}`;
  const named = (name: string) => {
    const w = rules ? itemWeight(rules, name) : null;
    return w === null ? { name } : { name, weight: w };
  };
  return (
    <>
      <ListSection
        title="Inventory"
        items={ch.inventory}
        onChange={(v) => edit((x) => void (x.inventory = v))}
        blank={() => ({ name: "", qty: 1, weight: 0, equipped: false, notes: "" })}
        head={["Item", "Qty", "Wt (lb)", "Equipped", "Notes", ""]}
        extra={
          <>
            <span className={`small ${total > max ? "bad-text" : "muted"}`}>{carrying}</span>
            {onStartingGear && <button type="button" className="btn small" onClick={onStartingGear}>Fill starting gear</button>}
            <button type="button" className="btn small" onClick={() => setPicking("equipment")}>+ Equipment</button>
            <button type="button" className="btn small" onClick={() => setPicking("magic-items")}>+ Magic item</button>
          </>
        }
        row={(it, set) => [
          <TextInput list={rules ? "dl-items" : undefined} value={it.name} onChange={(v) => set(named(v))} aria-label="Item" />,
          <NumInput className="num-input tiny" min={0} aria-label="Quantity" value={it.qty} onChange={(v) => set({ qty: v })} />,
          <NumInput className="num-input tiny" min={0} step="0.1" aria-label="Weight" value={it.weight} onChange={(v) => set({ weight: v })} />,
          <Checkbox aria-label="Equipped" checked={it.equipped} onChange={(v) => set({ equipped: v })} />,
          <TextInput value={it.notes} onChange={(v) => set({ notes: v })} aria-label="Notes" />,
        ]}
      />
      {rules && <datalist id="dl-items">{[...rules.items.map((i) => i.name), ...rules.weapons.map((w) => w.name)].map((n) => <option key={n} value={n} />)}</datalist>}
      {picking && (
        <SrdPicker category={picking} onClose={() => setPicking("")} onPick={(e) =>
          edit((x) => void x.inventory.push({ name: e.name, qty: 1, weight: picking === "equipment" ? e.data.weight || 0 : 0, equipped: false, notes: e.summary || "" }))} />
      )}
    </>
  );
}

