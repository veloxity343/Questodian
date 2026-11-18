// Characters: the list, and a character's sheet, laid out from the panels in sheet-panels.tsx
// (worked out by the character builder, or filled in by hand).

import { useEffect, useRef, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { api } from "../api";
import { Board } from "../components/board";
import { Checkbox, EmptyState, Field, HpBar, ListSection, LoadError, Loading, NumInput, SaveStatus, Select, TextInput } from "../components/ui";
import { useApp } from "../context/app";
import { useDice } from "../context/dice";
import { useDialogs, useToast } from "../context/ui";
import {
  apply, buildOf, isBuilt, itemWeight, madeBy, parts, startBuild, startNotes, work, type Build, type GearFill, type Rules, type Sheet as Built,
} from "../lib/builder";
import { derive, type Derived } from "../lib/derive";
import { targetEncounter } from "../lib/encounters";
import { useDraft, useLoad, usePref } from "../lib/hooks";
import { ABILITY_NAMES, download, downloadJson, pickFiles, slug, titleCase } from "../lib/util";
import type { Campaign, Character, Resource } from "../types";
import {
  BuilderAbilities, BuilderBanner, BuilderGuard, BuilderHeader, BuilderHow, BuilderResources, BuilderSkills, BuilderTiles, BuilderWarnings,
  BuilderWeapons, LevelLog, LevelUpDialog, RulesDataLink, StartingGearDialog, loadRules, useRules, type EditBuild,
} from "./builder";
import {
  AbilitiesPanel, AttacksPanel, CoinsPanel, CombatPanel, FeaturesPanel, InventoryPanel, PersonalityPanel, RESETS, SheetHeader, SkillsPanel,
  SpellsPanel, type Edit, type Roll,
} from "./sheet-panels";

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
