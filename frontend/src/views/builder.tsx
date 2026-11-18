// The character builder's parts of the sheet: every choice is a pick from the rules data, and
// everything else is worked out (lib/builder.ts) with a note on how.

import { Component, useEffect, useState, type ReactNode } from "react";
import { Link } from "react-router-dom";
import { api } from "../api";
import { Checkbox, Field, NumInput, Select, TextInput, type Option } from "../components/ui";
import { Modal, useDialogs, useToast } from "../context/ui";
import {
  METHODS, canStep, extraSpell, spellCounts, standardArrayOK, fightingStyleAllowed, highestSpellLevel, levelUp, needsLineage, ordinal, parts, pickBackground, resetScores,
  resetText, skillKey, spellGuide, startingGear, step, type Build, type BuiltWeapon, type Method, type Rules, type Sheet,
} from "../lib/builder";
import { ABILITIES, ABILITY_NAMES, signed, d20 } from "../lib/util";
import type { Ability, Campaign, Character, Resource } from "../types";
import type { Edit, Roll } from "./sheet-panels";

export type EditBuild = (fn: (b: Build) => Build) => void;

// ------------------------------------------------------------------ the rules data, shared

let rulesCache: Promise<Rules> | null = null;
const listeners = new Set<(r: Rules) => void>();

export function loadRules() {
  rulesCache ??= api.rules().catch((e: Error) => {
    rulesCache = null; // try again next time
    throw e;
  });
  return rulesCache;
}

/** After an edit to the rules data: everything showing it updates. */
export function setRules(rules: Rules) {
  rulesCache = Promise.resolve(rules);
  for (const listener of listeners) listener(rules);
}

export function useRules() {
  const [rules, setState] = useState<Rules | null>(null);
  const [error, setError] = useState<Error | null>(null);
  useEffect(() => {
    let live = true;
    loadRules().then((r) => live && setState(r), (e: Error) => live && setError(e));
    listeners.add(setState);
    return () => {
      live = false;
      listeners.delete(setState);
    };
  }, []);
  return { rules, error };
}

/** "Farmer", or "Farmer (edited)" / "Gunslinger (yours)" for entries changed on the Rules data page. */
const label = (e: { name: string; source?: string }) =>
  `${e.name}${e.source === "added" ? " (yours)" : e.source === "edited" ? " (edited)" : ""}`;

/** A number with its explanation on hover and tap. */
function How({ how, children }: { how: string; children: ReactNode }) {
  return <span className="how" title={how} tabIndex={0}>{children}</span>;
}

// ------------------------------------------------------------------ who you are

export function BuilderHeader({ ch, b, s, rules, edit, editBuild, campaigns, onLevelUp }: {
  ch: Character;
  b: Build;
  s: Sheet;
  rules: Rules;
  edit: Edit;
  editBuild: EditBuild;
  campaigns: Campaign[];
  onLevelUp: () => void;
}) {
  const p = parts(b, rules);
  const set = <K extends keyof Build>(key: K, value: Build[K]) => editBuild((x) => ({ ...x, [key]: value }));
  const dash: Option = ["", "—"];
  return (
    <section className="panel sheet-header">
      <div className="row gap wrap">
        <TextInput className="title-input grow" value={ch.name} onChange={(v) => edit((x) => void (x.name = v))} aria-label="Character name" />
        <label className="check" title={p.species?.heroic_inspiration_on_long_rest ? "Humans regain Heroic Inspiration after every Long Rest (Resourceful)" : "Spend it to reroll one die you just rolled"}>
          <Checkbox checked={ch.inspiration} onChange={(v) => edit((x) => void (x.inspiration = v))} /> Heroic Inspiration
        </label>
      </div>
      <div className="form-grid">
        <Field label="Player"><TextInput value={ch.player} onChange={(v) => edit((x) => void (x.player = v))} /></Field>
        <Field label="Class">
          <Select value={b.class} options={rules.classes.map((c): Option => [c.name, label(c)])} onChange={(v) => set("class", v)} />
        </Field>
        <Field label="Level">
          <div className="row gap-sm">
            <strong className="big-num">{b.level}</strong>
            <button type="button" className="btn small primary" disabled={b.level >= 20} onClick={onLevelUp}>Level up</button>
          </div>
        </Field>
        <Field label="Subclass"><TextInput value={b.subclass} onChange={(v) => set("subclass", v)} placeholder={b.level >= 3 ? "Choose at level 3" : "From level 3"} /></Field>
        <Field label="Species">
          <Select value={b.species} options={[dash, ...rules.species.map((x): Option => [x.name, label(x)])]} onChange={(v) => editBuild((x) => ({ ...x, species: v, lineage: "", human_feat: "" }))} />
        </Field>
        <Field label={p.species?.choice_label ?? "Lineage"} hint={needsLineage(p) ? undefined : "No choice for this species"}>
          <Select value={b.lineage} disabled={!needsLineage(p)} options={[dash, ...(p.species?.choices ?? []).map((c): Option => [c.name, c.name])]} onChange={(v) => set("lineage", v)} />
        </Field>
        <Field label="Background">
          <Select value={b.background} options={[dash, ...rules.backgrounds.map((x): Option => [x.name, label(x)])]} onChange={(v) => set("background", v)} />
        </Field>
        <Field label="Origin feat" hint="From your background">
          <span className="readout">{p.background?.feat ?? "—"}</span>
        </Field>
        <Field label="Extra Origin feat" hint={p.species?.extra_origin_feat ? `${p.species.name}: a second Origin feat` : "Only Humans get a second Origin feat"}>
          <Select value={b.human_feat} disabled={!p.species?.extra_origin_feat}
            options={[dash, ...rules.feats.filter((f) => f.type === "Origin").map((f): Option => [f.name, label(f)])]} onChange={(v) => set("human_feat", v)} />
        </Field>
        <Field label="Fighting style" hint={fightingStyleAllowed(b, p) ? undefined : "Fighters from level 1, Paladins and Rangers from level 2"}>
          <Select value={b.fighting_style} disabled={!fightingStyleAllowed(b, p)} options={[dash, ...rules.fighting_styles]} onChange={(v) => set("fighting_style", v)} />
        </Field>
        <Field label="Alignment"><Select value={ch.alignment} options={[dash, ...rules.alignments]} onChange={(v) => edit((x) => void (x.alignment = v))} /></Field>
        <Field label="Size"><span className="readout">{p.species?.size ?? "—"}</span></Field>
        <Field label="Campaign">
          <Select value={ch.campaign_id} options={[["", "— none —"], ...campaigns.map((c): Option => [c.id, c.name])]} onChange={(v) => edit((x) => void (x.campaign_id = v))} />
        </Field>
        <Field label="Experience"><NumInput min={0} value={ch.xp} onChange={(v) => edit((x) => void (x.xp = v))} /></Field>
      </div>
      <div className="row gap wrap level-line">
        <strong>Level {b.level}</strong>
        <span className="tag">Proficiency {signed(s.pb)}</span>
      </div>
    </section>
  );
}

// ------------------------------------------------------------------ ability scores

export function BuilderAbilities({ b, s, rules, editBuild, roll }: { b: Build; s: Sheet; rules: Rules; editBuild: EditBuild; roll: Roll }) {
  const { confirm } = useDialogs();
  const bg = parts(b, rules).background;
  const methodNote = {
    point_buy: `Point Buy: ${rules.point_buy.budget} points. Costs: ${Object.entries(rules.point_buy.costs).map(([k, v]) => `${k} = ${v}`).join(", ")}.`,
    standard_array: `Standard Array: use ${rules.standard_array.join(", ")} once each. − / + swap with the next score in the array.`,
    rolled: `Rolled: 4d6, drop the lowest, six times. Scores ${rules.rolled.min} to ${rules.rolled.max}.`,
  }[b.method];
  const left = b.method === "point_buy" ? s.checks.points === "—" ? `use ${rules.point_buy.min}–${rules.point_buy.max}` : `${rules.point_buy.budget - Number(s.checks.points.split(" ")[0])} of ${rules.point_buy.budget}` : "";
  const reset = async () => {
    const what = b.method === "point_buy" ? `All scores go to ${rules.point_buy.min} and you get ${rules.point_buy.budget} points to spend.`
      : b.method === "standard_array" ? "The array is dealt out the way the SRD suggests for your class; use − / + to swap."
      : "All scores go to 10; step them to your rolls, or roll them here.";
    if (await confirm(`Reset the six base scores? ${what}`, { label: "Reset", danger: false })) editBuild((x) => resetScores(x, rules));
  };
  const rollScores = async () => {
    const { results } = await api.stats();
    const text = results.map((r) => r.total).join(", ");
    if (await confirm(`Rolled ${text} (4d6, drop the lowest). Use them in order, Strength to Charisma? You can step them afterwards.`, { label: "Use them", danger: false }))
      editBuild((x) => ({ ...x, base: Object.fromEntries(ABILITIES.map((a, i) => [a, results[i].total])) as Record<Ability, number> }));
  };
  const radio = (a: Ability, amount: 1 | 2) => {
    const on = (b.background_bonus[a] ?? 0) === amount;
    const allowed = !!bg?.abilities.includes(a);
    return (
      <button type="button" role="radio" aria-checked={on} disabled={!allowed} className={`radio-pick ${on ? "on" : ""}`}
        title={allowed ? `${on ? "Clear" : "Give"} +${amount} ${ABILITY_NAMES[a]}` : bg ? `${bg.name} raises ${bg.abilities.map((x) => ABILITY_NAMES[x]).join(", ")}` : "Pick a background first"}
        onClick={() => editBuild((x) => pickBackground(x, rules, a, amount))}>
        {on ? "●" : "○"} +{amount}
      </button>
    );
  };
  return (
    <section className="panel">
      <div className="panel-head">
        <h2>Ability scores</h2>
        <div className="row gap wrap">
          <Select aria-label="Ability method" value={b.method} options={METHODS} onChange={(v) => editBuild((x) => {
            const next = { ...x, method: v as Method };
            // The standard array's − / + only work on the array: deal it out if the scores aren't one.
            return next.method === "standard_array" && !standardArrayOK(next, rules) ? resetScores(next, rules) : next;
          })} />
          {b.method === "rolled" && <button type="button" className="btn small" onClick={rollScores}>🎲 Roll</button>}
          <button type="button" className="btn small" onClick={reset}>Reset scores</button>
        </div>
      </div>
      <div className="table-wrap">
        <table className="table compact ability-table">
          <thead>
            <tr><th>Ability</th><th>Base</th><th title="+2 and +1 on two of your background's abilities, or +1 on all three">Background</th><th title="Ability Score Improvements and other feats">Other +</th><th>Score</th><th>Mod</th><th>Save</th></tr>
          </thead>
          <tbody>
            {ABILITIES.map((a) => (
              <tr key={a} title={`${s.scoreHow[a]}\n${rules.ability_uses[a]}`}>
                <th scope="row">{ABILITY_NAMES[a]}{s.spell?.ability === a && <span className="tag">Spellcasting</span>}</th>
                <td>
                  <div className="stepper">
                    <button type="button" className="btn small" aria-label={`Lower ${ABILITY_NAMES[a]}`} disabled={!canStep(b, rules, a, -1)} onClick={() => editBuild((x) => step(x, rules, a, -1))}>−</button>
                    <strong>{b.base[a]}</strong>
                    <button type="button" className="btn small" aria-label={`Raise ${ABILITY_NAMES[a]}`} disabled={!canStep(b, rules, a, 1)} onClick={() => editBuild((x) => step(x, rules, a, 1))}>+</button>
                  </div>
                </td>
                <td><div className="row gap-sm nowrap" role="radiogroup" aria-label={`${ABILITY_NAMES[a]} background bonus`}>{radio(a, 2)}{radio(a, 1)}</div></td>
                <td><NumInput className="num-input tiny" min={0} max={10} aria-label={`${ABILITY_NAMES[a]} other bonus`} value={b.other_bonus[a] ?? 0}
                  onChange={(v) => editBuild((x) => ({ ...x, other_bonus: { ...x.other_bonus, [a]: Math.max(0, v) } }))} /></td>
                <td><strong className="score">{s.scores[a]}</strong></td>
                <td><button type="button" className="link mod" title={`Roll ${ABILITY_NAMES[a]} check`} onClick={() => roll(d20(s.mods[a]), `${ABILITY_NAMES[a]} check`)}>{signed(s.mods[a])}</button></td>
                <td>
                  <button type="button" className="link" title={s.saves.includes(a) ? "Proficient (your class)" : "Not proficient"} onClick={() => roll(d20(s.mods[a] + (s.saves.includes(a) ? s.pb : 0)), `${ABILITY_NAMES[a]} save`)}>
                    {s.saves.includes(a) ? "✓ " : ""}{signed(s.mods[a] + (s.saves.includes(a) ? s.pb : 0))}
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="row gap wrap">
        {b.method === "point_buy" && <span className={`tag big ${s.checks.abilities === "OK" ? "" : "bad"}`}>Points left {left}</span>}
        <Check label="Ability scores" result={s.checks.abilities} />
        <Check label="Background +" result={s.checks.background} />
      </div>
      <p className="muted small">{methodNote} {bg ? `${bg.name}: +2 to one and +1 to another of ${bg.abilities.map((x) => ABILITY_NAMES[x]).join(", ")}, or +1 to all three. Nothing above ${rules.score_cap}.` : "Pick a background: it decides which abilities get +2 and +1."}</p>
    </section>
  );
}

function Check({ label: name, result }: { label: string; result: string }) {
  const ok = result === "OK";
  return <span className={`check-result ${ok ? "ok" : "bad"}`}>{ok ? "✓" : "✗"} {name}{ok ? "" : `: ${result}`}</span>;
}

// ------------------------------------------------------------------ skills

export function BuilderSkills({ b, s, rules, editBuild, roll }: { b: Build; s: Sheet; rules: Rules; editBuild: EditBuild; roll: Roll }) {
  const cls = parts(b, rules).cls;
  const options = cls?.skill_options === "any" ? null : new Set((cls?.skill_options ?? []).map(skillKey));
  const toggle = (key: string, list: "skill_picks" | "expertise", on: boolean) =>
    editBuild((x) => {
      const next = on ? [...new Set([...x[list], key])] : x[list].filter((k) => k !== key);
      // Dropping a skill drops its Expertise too.
      return list === "skill_picks" && !on ? { ...x, skill_picks: next, expertise: x.expertise.filter((k) => k !== key) } : { ...x, [list]: next };
    });
  return (
    <section className="panel">
      <div className="panel-head">
        <h2>Skills</h2>
        <div className="stack">
          <span className={`check-result ${s.skillPicks.ok ? "ok" : "bad"}`}>{s.skillPicks.ok ? "✓" : "✗"} {s.skillPicks.text}</span>
          {s.expertise.text && <span className={`check-result ${s.expertise.ok ? "ok" : "bad"}`}>{s.expertise.ok ? "✓" : "✗"} {s.expertise.text}</span>}
        </div>
      </div>
      <div className="table-wrap">
        <table className="table compact">
          <thead><tr><th>Skill</th><th title="Your class can pick it">Class</th><th>Given by</th><th>Pick</th><th title="Only when a feature (e.g. Bard, Rogue) grants it">Expertise</th><th>Bonus</th></tr></thead>
          <tbody>
            {Object.entries(rules.skills).map(([name, ab]) => {
              const key = skillKey(name);
              const sk = s.skills[key];
              const fromBg = sk.from === "Background";
              const proficient = sk.prof === "proficient" || sk.prof === "expertise";
              return (
                <tr key={key} className={`prof-${sk.prof}`}>
                  <td>{name} <span className="muted small">({ab.toUpperCase()})</span></td>
                  <td>{!options || options.has(key) ? "✓" : ""}</td>
                  <td className="small">{fromBg ? "Background" : ""}</td>
                  <td><Checkbox aria-label={`Pick ${name}`} disabled={fromBg} checked={fromBg || b.skill_picks.includes(key)} onChange={(v) => toggle(key, "skill_picks", v)} /></td>
                  <td>
                    {/* Only as many as the class gives at this level, and only on skills it allows. */}
                    <Checkbox aria-label={`${name} expertise`} checked={b.expertise.includes(key)} onChange={(v) => toggle(key, "expertise", v)}
                      disabled={!b.expertise.includes(key) && (!proficient || s.expertise.used >= s.expertise.allowed || (!!s.expertise.options && !s.expertise.options.includes(key)))} />
                  </td>
                  <td><button type="button" className="link" onClick={() => roll(d20(sk.bonus), name)}>{signed(sk.bonus)}</button>{sk.prof === "half" && <span className="muted small" title="Jack of All Trades: half Proficiency Bonus"> ½</span>}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <p className="muted small">
        Bonus = ability modifier + Proficiency Bonus if proficient (twice it with Expertise).{s.jack ? " Jack of All Trades: half your Proficiency Bonus on skills you aren't proficient in." : ""}{" "}
        Passive Perception <How how={s.passivePerception.how}>{s.passivePerception.value}</How>.
      </p>
    </section>
  );
}

// ------------------------------------------------------------------ in a fight

/** The numbers worked out for a fight. `initiative` is the rolled bonus (after exhaustion). */
export function BuilderTiles({ s, initiative, roll }: { s: Sheet; initiative: number; roll: Roll }) {
  const tile = (name: string, value: ReactNode, how: string, onClick?: () => void) => (
    <div className="stat-box" title={how}>
      <span>{name}</span>
      {onClick ? <button type="button" className="big-num link" onClick={onClick}>{value}</button> : <strong className="big-num">{value}</strong>}
    </div>
  );
  return (
    <div className="stat-boxes">
      {tile("Armor Class", s.ac.value, s.ac.how)}
      {tile("Initiative", signed(s.initiative.value), s.initiative.how, () => roll(d20(initiative), "Initiative"))}
      {tile("Speed", `${s.speed.value} ft`, s.speed.how)}
      {tile("HP max", s.hp.value, s.hp.how)}
      {tile("Darkvision", s.darkvision.value ? `${s.darkvision.value} ft` : "None", s.darkvision.how)}
      {tile("Passive Perception", s.passivePerception.value, s.passivePerception.how)}
      {tile("Attacks / action", s.attacksPerAction.value, s.attacksPerAction.how)}
      {tile("Weapon masteries", s.masteries.value, s.masteries.how)}
    </div>
  );
}

// ------------------------------------------------------------------ weapons and armor

export function BuilderWeapons({ b, s, rules, editBuild, roll }: { b: Build; s: Sheet; rules: Rules; editBuild: EditBuild; roll: Roll }) {
  const toast = useToast();
  const p = parts(b, rules);
  const rollDamage = (w: BuiltWeapon, crit: boolean) => {
    const m = w.damage.match(/^[\d\sd+\-]+/i);
    if (!m) return toast("No dice in this damage", "error");
    let expr = m[0].trim().replace(/[+-]$/, "");
    if (crit) expr = expr.replace(/(\d*)d(\d+)/gi, (_, n, sides) => `${(Number(n) || 1) * 2}d${sides}`);
    roll(expr, `${w.name} damage${crit ? " (crit)" : ""}`);
  };
  const row = (w: BuiltWeapon, i: number | null) => (
    <tr key={`${w.name}-${i}`}>
      <td>{w.name}{w.mastery && <div className="muted small">Mastery: {w.mastery}</div>}</td>
      <td title={w.how}><button type="button" className="btn small" onClick={() => roll(d20(w.attack), `${w.name} to hit`)}>{signed(w.attack)}</button></td>
      <td title={w.damageHow}>
        <div className="row gap-sm nowrap">
          <span>{w.damage}</span>
          <button type="button" className="btn small" title="Roll damage" onClick={() => rollDamage(w, false)}>🎲</button>
          <button type="button" className="btn small" title="Roll critical damage" onClick={() => rollDamage(w, true)}>Crit</button>
        </div>
      </td>
      <td className={`small ${w.proficient ? "" : "bad-text"}`}>{w.notes}</td>
      <td>{i !== null && <button type="button" className="icon-btn" aria-label={`Remove ${w.name}`} onClick={() => editBuild((x) => ({ ...x, weapons: x.weapons.filter((_, j) => j !== i) }))}>✕</button>}</td>
    </tr>
  );
  const training = p.cls ? `Training: ${p.cls.armor_text || "none"}. Weapons: ${p.cls.weapons_text}.` : "";
  return (
    <section className="panel">
      <div className="panel-head">
        <h2>Weapons and armor</h2>
        <Select aria-label="Add a weapon" className="small-select" value=""
          options={[["", "+ weapon"], ...rules.weapons.map((w): Option => [w.name, `${w.name} (${w.category} ${w.kind.toLowerCase()})`])]}
          onChange={(v) => v && editBuild((x) => ({ ...x, weapons: [...x.weapons, v] }))} />
      </div>
      <div className="row gap wrap">
        <Field label="Armor">
          <Select value={b.armor} options={rules.armor.filter((a) => a.category !== "Shield").map((a): Option => [a.name, a.category === "None" ? a.name : `${a.name} (${a.category}, AC ${a.base_ac})`])}
            onChange={(v) => editBuild((x) => ({ ...x, armor: v }))} />
        </Field>
        <label className="check"><Checkbox checked={b.shield} onChange={(v) => editBuild((x) => ({ ...x, shield: v }))} /> Shield (+2 AC)</label>
      </div>
      <p className="muted small">{training}</p>
      <div className="table-wrap">
        <table className="table compact">
          <thead><tr><th>Weapon</th><th>Attack</th><th>Damage</th><th>Range · properties</th><th></th></tr></thead>
          <tbody>
            {s.weapons.map((w, i) => row(w, i))}
            {row(s.unarmed, null)}
          </tbody>
        </table>
      </div>
      <p className="muted small">Hover an attack or damage to see how it's worked out.</p>
    </section>
  );
}

// ------------------------------------------------------------------ resources

export function BuilderResources({ ch, s, edit }: { ch: Character; s: Sheet; edit: Edit }) {
  if (!s.resources.length) return null;
  const setCurrent = (name: string, current: number) =>
    edit((x) => {
      const r = x.resources.find((y) => y.name === name);
      if (r) r.current = Math.max(0, Math.min(r.max, current));
    });
  return (
    <section className="panel">
      <h2>Class resources</h2>
      <div className="table-wrap">
        <table className="table compact">
          <thead><tr><th>Resource</th><th>Left</th><th>Max</th><th>Recharges</th></tr></thead>
          <tbody>
            {s.resources.map((r) => {
              const current = ch.resources.find((x: Resource) => x.name === r.name)?.current ?? r.max;
              return (
                <tr key={r.name} title={r.how}>
                  <td>{r.name}</td>
                  <td>
                    {r.max <= 10 ? (
                      <div className="pips">
                        {Array.from({ length: r.max }, (_, i) => (
                          <button type="button" key={i} className={`pip ${i < current ? "" : "used"}`} aria-label={`${r.name} ${i + 1}${i < current ? "" : " (used)"}`}
                            onClick={() => setCurrent(r.name, i < current ? i : i + 1)} />
                        ))}
                      </div>
                    ) : <NumInput className="num-input tiny" min={0} max={r.max} aria-label={`${r.name} left`} value={current} onChange={(v) => setCurrent(r.name, v)} />}
                  </td>
                  <td>{r.max}</td>
                  <td className="small">{resetText(r.reset)}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}

// ------------------------------------------------------------------ warnings and explanations

export function BuilderWarnings({ s }: { s: Sheet }) {
  if (!s.warnings.length) return <p className="check-result ok panel">✓ Nothing to fix: every choice is made and adds up.</p>;
  return (
    <section className="panel warnings">
      <h2>Warnings and notes</h2>
      <ul>{s.warnings.map((w) => <li key={w}>{w}</li>)}</ul>
    </section>
  );
}

export function BuilderHow({ b, s, rules }: { b: Build; s: Sheet; rules: Rules }) {
  const p = parts(b, rules);
  const feat = (name: string) => rules.feats.find((f) => f.name === name)?.text ?? "";
  const lines: [string, string][] = [
    ["Armor Class", s.ac.how],
    ["Initiative", s.initiative.how],
    ["Hit Points", s.hp.how],
    ["Hit Point Dice", `One d${s.hitDie} per level. Short Rest: roll each one you spend + Con ${signed(s.mods.con)} (at least 1 HP each). Long Rest: all back.`],
    ["Speed", s.speed.how],
    ["Proficiency Bonus", `${signed(s.pb)} at level ${b.level} (+2 at levels 1–4, +3 at 5–8, +4 at 9–12, +5 at 13–16, +6 at 17–20). Added to anything you're proficient in.`],
    ["Passive Perception", s.passivePerception.how],
    ["Darkvision", s.darkvision.how],
    ["Spellcasting", s.spell ? s.spell.how : `${b.class ? `${b.class}s don't` : "This class doesn't"} cast spells (their spellcasting subclasses aren't in the SRD).`],
    ["Attacks", s.attacksPerAction.how],
    ["Weapon Mastery", s.masteries.how],
    ["Carrying", s.carry.how],
    ["Origin feat", p.background ? `${p.background.feat} (from ${p.background.name}): ${feat(p.background.feat)}` : "Pick a background: it gives you an Origin feat."],
  ];
  if (p.species?.extra_origin_feat) lines.push([`${p.species.name}'s feat`, b.human_feat ? `${b.human_feat}: ${feat(b.human_feat)}` : "Pick a second Origin feat."]);
  if (p.species) lines.push([p.species.choice_label ?? "Species traits", `${p.species.traits}${p.lineage ? ` · ${p.lineage.name}: ${p.lineage.text}` : ""}`]);
  for (const a of ABILITIES) lines.push([ABILITY_NAMES[a], `${s.scoreHow[a]} Used for: ${rules.ability_uses[a]}`]);
  for (const w of [...s.weapons, s.unarmed]) lines.push([w.name, `${w.how} ${w.damageHow}`]);
  for (const r of s.resources) lines.push([r.name, `${r.how}. ${resetText(r.reset)}.`]);
  return (
    <details className="panel how-panel">
      <summary><h2>How it's worked out</h2></summary>
      <dl>
        {lines.map(([k, v]) => <div key={k}><dt>{k}</dt><dd>{v}</dd></div>)}
      </dl>
    </details>
  );
}

// ------------------------------------------------------------------ spells

export function BuilderSpellNotes({ b, s, ch }: { b: Build; s: Sheet; ch: Character }) {
  const spells = ch.spellcasting.spells;
  const { cantrips, prepared } = spellCounts(spells);
  const highest = highestSpellLevel(s);
  const tooHigh = spells.filter((x) => x.level > highest && x.level > 0 && !extraSpell(x));
  const status = cantrips <= s.cantrips && prepared <= s.prepared ? (cantrips === s.cantrips && prepared === s.prepared ? "✓" : "room for more") : "too many";
  return (
    <div className="spell-notes">
      {s.spell && (
        <p className={`check-result ${status === "too many" ? "bad" : "ok"}`}>
          Cantrips {cantrips} of {s.cantrips} · prepared {prepared} of {s.prepared}{s.pact.slots ? ` · ${s.pact.slots} pact slots, ${ordinal(s.pact.level)} level` : ""} · {status}
        </p>
      )}
      {tooHigh.length > 0 && <p className="check-result bad">Above the highest level you can cast ({highest}): {tooHigh.map((x) => x.name).join(", ")}</p>}
      <ul className="small muted">{spellGuide(b, s).map((line) => <li key={line}>{line}</li>)}</ul>
      <p className="small muted">Spells from a feat (Magic Initiate): put "Magic Initiate" in their notes so they don't count. Always-prepared spells: put "Always" in their notes.</p>
    </div>
  );
}

// ------------------------------------------------------------------ level up

export function LevelUpDialog({ b, s, rules, onClose, onDone }: { b: Build; s: Sheet; rules: Rules; onClose: () => void; onDone: (next: Build) => void }) {
  const [choice, setChoice] = useState<"average" | "roll">("average");
  const [rolled, setRolled] = useState<number | null>(null);
  const [result, setResult] = useState<ReturnType<typeof levelUp>>(null);
  const die = s.hitDie;
  const avg = die / 2 + 1;
  const con = s.mods.con;
  if (result) {
    return (
      <Modal title={`Welcome to level ${result.entry.level}!`} onClose={onClose} actions={[{ label: "Done", primary: true }]}>
        <h3>What you gained</h3>
        <ul>{result.entry.gained.map((g) => <li key={g}>{g}</li>)}</ul>
        <h3>What to do</h3>
        <ul>{result.entry.todo.map((t) => <li key={t}>{t}</li>)}</ul>
        <p className="muted small">This is kept in the level log at the bottom of the sheet.</p>
      </Modal>
    );
  }
  const go = () => {
    if (choice === "roll" && rolled === null) return false;
    const r = levelUp(b, rules, choice === "roll" ? rolled : null);
    if (!r) return false;
    onDone(r.build);
    setResult(r);
    return false;
  };
  return (
    <Modal title={`Level up to ${b.class} ${b.level + 1}?`} onClose={onClose} actions={[{ label: "Cancel" }, { label: "Level up", primary: true, onClick: go }]}>
      <p>Your Hit Point maximum goes up by your Hit Die (d{die}) + your Constitution modifier ({signed(con)}). Take the fixed average or roll.</p>
      <div className="stack">
        <label className="check"><input type="radio" name="hp" checked={choice === "average"} onChange={() => setChoice("average")} /> Take the average: {avg} {signed(con)} = {Math.max(1, avg + con)} HP</label>
        <label className="check"><input type="radio" name="hp" checked={choice === "roll"} onChange={() => setChoice("roll")} /> Roll a d{die} (at least 1 HP)</label>
        {choice === "roll" && (
          <div className="row gap">
            <button type="button" className="btn small" onClick={async () => setRolled((await api.roll(`d${die}`)).total)}>🎲 Roll d{die}</button>
            <label className="row gap-sm">or type it <NumInput className="num-input tiny" min={1} max={die} value={rolled ?? 0} onChange={(v) => setRolled(v >= 1 && v <= die ? v : null)} /></label>
            {rolled !== null && <strong>{rolled} {signed(con)} = {Math.max(1, rolled + con)} HP</strong>}
          </div>
        )}
      </div>
    </Modal>
  );
}

export function LevelLog({ b }: { b: Build }) {
  if (!b.level_log.length) return null;
  return (
    <section className="panel">
      <h2>Level log</h2>
      <div className="table-wrap">
        <table className="table compact">
          <thead><tr><th>Date</th><th>Level</th><th>HP gained</th><th>What you gained</th><th>What to do</th></tr></thead>
          <tbody>
            {[...b.level_log].reverse().map((e, i) => (
              <tr key={i}><td className="nowrap">{e.date}</td><td>{e.level}</td><td>+{e.hp}</td><td className="small">{e.gained.join("; ")}</td><td className="small">{e.todo.join(" ")}</td></tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

// ------------------------------------------------------------------ starting gear

export function StartingGearDialog({ b, rules, onClose, onFill }: {
  b: Build;
  rules: Rules;
  onClose: () => void;
  onFill: (fill: ReturnType<typeof startingGear>) => void;
}) {
  const { confirm } = useDialogs();
  const p = parts(b, rules);
  const classOptions = Object.keys(p.cls?.starting_gear ?? {});
  const bgOptions = Object.keys(p.background?.starting_gear ?? {});
  const [co, setCo] = useState(classOptions[0] ?? "");
  const [bo, setBo] = useState(bgOptions[0] ?? "");
  const describe = (lines: { item: string; qty: number }[] = []) => lines.map((l) => (l.item === "GP" || l.item === "SP" ? `${l.qty} ${l.item}` : l.qty > 1 ? `${l.qty} × ${l.item}` : l.item)).join(", ");
  if (!p.cls || !p.background) {
    return <Modal title="Starting equipment" onClose={onClose} actions={[{ label: "OK" }]}><p>Pick a class and a background first.</p></Modal>;
  }
  const fill = async () => {
    if (!(await confirm(`This replaces your armor, shield, weapons, inventory and coins with ${p.cls!.name} option ${co} and ${p.background!.name} option ${bo}.`, { label: "Fill", danger: false }))) return false;
    onFill(startingGear(b, rules, co, bo));
  };
  return (
    <Modal title="Starting equipment" onClose={onClose} actions={[{ label: "Cancel" }, { label: "Fill", primary: true, onClick: fill }]}>
      <h3>From your class ({p.cls.name})</h3>
      {classOptions.map((o) => (
        <label key={o} className="check gear-option"><input type="radio" name="class-gear" checked={co === o} onChange={() => setCo(o)} /> <strong>{o}</strong> {describe(p.cls!.starting_gear[o])}</label>
      ))}
      <h3>From your background ({p.background.name})</h3>
      {bgOptions.map((o) => (
        <label key={o} className="check gear-option"><input type="radio" name="bg-gear" checked={bo === o} onChange={() => setBo(o)} /> <strong>{o}</strong> {describe(p.background!.starting_gear[o])}</label>
      ))}
      <p className="muted small">Items marked "your choice" or "same kind": rename them to the exact one you picked.</p>
    </Modal>
  );
}

// ------------------------------------------------------------------ switching on and off

export function BuilderBanner({ onStart, again }: { onStart: () => void; again: boolean }) {
  return (
    <section className="panel builder-banner">
      <div className="row gap wrap">
        <div className="grow">
          <strong>{again ? "This sheet is edited by hand." : "Use the character builder?"}</strong>
          <p className="muted small">{again
            ? "Switch back to the builder to have your scores, skills, AC, HP, attacks and resources worked out from your choices again."
            : "Pick a class, species and background, set your scores with point buy, the standard array or rolls, and the sheet works out the rest (2024 rules)."}</p>
        </div>
        <button type="button" className="btn primary" onClick={onStart}>{again ? "Back to the builder" : "Use the builder"}</button>
      </div>
    </section>
  );
}

export function RulesDataLink() {
  return <Link to="/rules-data" className="btn small">Rules data</Link>;
}

/** If the rules data has been edited into a shape the builder can't read, say so instead of a blank page. */
export class BuilderGuard extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };
  static getDerivedStateFromError(error: Error) {
    return { error };
  }
  render() {
    if (!this.state.error) return this.props.children;
    return (
      <section className="panel warnings">
        <h2>The builder can't work this sheet out</h2>
        <p>Something in the rules data doesn't have the shape it expects ({this.state.error.message}). It's most likely an edit: open <Link to="/rules-data">Rules data</Link> and reset the entries or settings marked "edited", then come back.</p>
      </section>
    );
  }
}
