// The encounter monitor: initiative order, HP, conditions, difficulty.

import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { Checkbox, NumInput, TextInput } from "../components/ui";
import { useApp } from "../context/app";
import { useDialogs, useToast } from "../context/ui";
import { useDebounced, usePref } from "../lib/hooks";
import { fmtDate, hpLevel, signed, titleCase } from "../lib/util";
import type { Combatant, Encounter, EncounterAction, SrdEntry } from "../types";
import { useDeleteRecord } from "./parts";
import { useDM, useParty } from "./store";

const DIFFICULTY_LABEL: Record<string, string> = {
  trivial: "Under half of Low", low: "Low", moderate: "Moderate", high: "High", deadly: "Above High",
};

/**
 * The 2024 XP budget (rules/gameplay-toolbox-combat-encounters), the only encounter table in
 * either SRD, so 2014 campaigns use it too. The SRD has Low, Moderate and High; "under half of
 * Low" and "above High" are the app's own bands around them.
 */
function difficulty(xp: number, size: number, level: number, table: [number, number, number][] | undefined) {
  const row = table?.[Math.max(1, Math.min(level, 20)) - 1] ?? [50, 75, 100];
  const budget = { low: row[0] * size, moderate: row[1] * size, high: row[2] * size };
  let rating = "deadly";
  if (xp <= 0 || xp < budget.low * 0.5) rating = "trivial";
  else if (xp <= budget.low) rating = "low";
  else if (xp <= budget.moderate) rating = "moderate";
  else if (xp <= budget.high) rating = "high";
  return { budget, rating };
}

export function EncounterDoc({ initial }: { initial: Encounter }) {
  const dm = useDM();
  const { meta } = useApp();
  const toast = useToast();
  const { confirm } = useDialogs();
  const [enc, setEnc] = useState(initial);
  const [adding, setAdding] = useState(false);
  const [showLog, setShowLog] = useState(false);
  const party = useParty();
  const [groupRolls, setGroupRolls] = usePref("group-initiative", false);
  const onDelete = useDeleteRecord("encounters", initial.id, initial.name, () => {});
  // Pick up changes made elsewhere (e.g. "To encounter" on an NPC or a stat block).
  const listed = dm.lists.encounters.find((e) => e.id === initial.id);
  useEffect(() => {
    if (listed && listed.updated !== enc.updated) setEnc(listed);
  }, [listed, enc.updated]);

  const act = async (action: EncounterAction) => {
    try {
      const fresh = await api.encounter(enc.id, action);
      setEnc(fresh);
      dm.upsert("encounters", fresh);
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };
  const patch = async (changes: Partial<Encounter>) => {
    const fresh = await api.update("encounters", enc.id, changes);
    setEnc(fresh);
    dm.upsert("encounters", fresh);
  };
  const saveName = useDebounced((name: string) => void patch({ name }), 500);

  const monsterXp = enc.combatants
    .filter((c) => c.kind === "monster")
    .reduce((a, c) => a + ((c.in_lair && c.xp_in_lair) || c.xp || 0), 0);
  const { budget, rating } = difficulty(monsterXp, enc.party_size, enc.party_level, meta?.xp_budget);
  const current = enc.active ? enc.combatants[enc.turn] : null;

  return (
    <article className="doc encounter">
      <header className="doc-head">
        <div className="doc-title">
          <span className="kind-badge">Encounter</span>
          <input className="title-input" value={enc.name} aria-label="Encounter name"
            onChange={(e) => { setEnc({ ...enc, name: e.target.value }); saveName(e.target.value); }} />
          <div className="eyebrow">{enc.active ? `Round ${enc.round}` : "Not started"}</div>
        </div>
        <div className="doc-actions">
          <button type="button" className="btn small ghost" onClick={() => act({ action: "roll_initiative", which: enc.active ? "unset" : "all", group: groupRolls })}
            title="Roll for everyone without an initiative (all, before combat starts). Ties start in order of initiative bonus; edit an initiative to change the order.">Roll initiative</button>
          <label className="check small" title="One roll for each group of identical monsters, who then act together (the SRD's way)">
            <Checkbox checked={groupRolls} onChange={setGroupRolls} /> group
          </label>
          <button type="button" className="btn small ghost" onClick={() => act({ action: "roll_hp" })} title="Roll hit dice for every monster">Roll HP</button>
          {enc.active && <button type="button" className="btn small ghost" onClick={() => act({ action: "prev" })} aria-label="Previous turn">←</button>}
          <button type="button" className="btn small primary" onClick={() => act({ action: "next" })}>{enc.active ? "Next →" : "Start combat →"}</button>
          <button type="button" className={`btn small ${adding ? "primary" : ""}`} onClick={() => setAdding((a) => !a)}>+ Add combatant</button>
          {enc.active && <button type="button" className="btn small ghost" onClick={() => act({ action: "end" })}>Finish</button>}
          <details className="menu">
            <summary className="btn small ghost" aria-label="More">⋯</summary>
            <div className="menu-pop">
              <button type="button" onClick={async () => (await confirm("Remove every monster and NPC? Party members stay.", { label: "Clear" })) && act({ action: "clear" })}>Clear monsters</button>
              <button type="button" onClick={() => setShowLog((v) => !v)}>{showLog ? "Hide" : "Show"} combat log</button>
              <button type="button" className="danger" onClick={onDelete}>Delete encounter</button>
            </div>
          </details>
        </div>
      </header>

      <div className="party-bar">
        <label>Party <NumInput className="num-input tiny" min={1} max={12} value={enc.party_size} onChange={(v) => patch({ party_size: Math.max(1, v) })} /></label>
        <span className="muted">×</span>
        <label>Lv <NumInput className="num-input tiny" min={1} max={20} value={enc.party_level} onChange={(v) => patch({ party_level: Math.max(1, Math.min(20, v)) })} /></label>
        <span className={`difficulty d-${rating}`} title="XP budget from the 2024 rules (SRD 5.2.1). SRD 5.1 has no encounter-building table, so 2014 campaigns use this one too.">{DIFFICULTY_LABEL[rating]}</span>
        <span className="grow" />
        <span className="muted small">Monsters {monsterXp.toLocaleString()} XP · Low {budget.low.toLocaleString()} · Moderate {budget.moderate.toLocaleString()} · High {budget.high.toLocaleString()}</span>
      </div>

      {adding && <AddCombatant enc={enc} party={party} act={act} onDone={() => setAdding(false)} />}

      {current && (
        <div className="now-banner">
          <span className="eyebrow">Now</span> <strong>{current.name}</strong> <KindBadge c={current} />
          {current.conditions.length > 0 && <span className="muted small"> · {current.conditions.map(titleCase).join(", ")}</span>}
        </div>
      )}

      {enc.combatants.length ? (
        <ol className="initiative">
          {enc.combatants.map((c, i) => <CombatantRow key={c.id} c={c} isTurn={enc.active && i === enc.turn} started={enc.active} act={act} />)}
        </ol>
      ) : (
        <div className="empty">
          <h3>No combatants yet</h3>
          <p>Add the party, monsters (SRD or homebrew) and NPCs. Then roll initiative and start.</p>
          <button type="button" className="btn primary" onClick={() => setAdding(true)}>+ Add combatant</button>
        </div>
      )}

      {showLog && (
        <ul className="log">
          {[...enc.log].reverse().map((e) => (
            <li key={e.id} className="log-entry kind-combat"><time className="muted small">{fmtDate(e.time)}</time><span className="grow">{e.text}</span></li>
          ))}
        </ul>
      )}
    </article>
  );
}

function KindBadge({ c }: { c: Combatant }) {
  const label = c.kind === "pc" ? "PC" : c.kind === "npc" ? "NPC" : "M";
  return <span className={`kind-badge k-${c.kind}`}>{label}</span>;
}

const SURPRISE_HELP = {
  "2024": "Surprised: Disadvantage on its Initiative roll",
  "2014": "Surprised: can't move or take an action on its first turn, or take a reaction until that turn ends",
};

function CombatantRow({ c, isTurn, started, act }: { c: Combatant; isTurn: boolean; started: boolean; act: (a: EncounterAction) => Promise<void> }) {
  const dm = useDM();
  const [hpOpen, setHpOpen] = useState(false);
  const update = (changes: Partial<Combatant>) => act({ action: "update", id: c.id, changes });
  const pct = c.max_hp ? Math.max(0, Math.min(100, (c.hp / c.max_hp) * 100)) : 100;
  const showSheet = () => {
    if (c.srd_id) dm.show({ type: "srd", id: c.srd_id });
    else if (c.monster_id) dm.show({ type: "monster", id: c.monster_id });
    else if (c.npc_id) dm.show({ type: "npc", id: c.npc_id });
    else if (c.character_id) dm.show({ type: "character", id: c.character_id });
  };
  return (
    <li className={`combatant ${isTurn ? "turn" : ""} ${c.hp <= 0 && c.max_hp ? "down" : ""} ${c.hidden ? "is-hidden" : ""}`}>
      <NumInput commit className="init" value={c.initiative} onChange={(v) => update({ initiative: v })} aria-label={`Initiative for ${c.name}`} />
      <div className="who">
        <button type="button" className="name" onClick={showSheet}>{c.name}</button> <KindBadge c={c} />
        {c.init_bonus !== 0 && <span className="muted small"> {signed(c.init_bonus)}</span>}
        {!started ? (
          <label className="check small" title={SURPRISE_HELP[dm.edition]}>
            <Checkbox checked={c.surprised} onChange={(v) => update({ surprised: v })} /> surprised
          </label>
        ) : c.surprised && <span className="muted small" title={SURPRISE_HELP[dm.edition]}> surprised</span>}
        {c.kind === "monster" && c.xp_in_lair > 0 && (
          <label className="check small lair" title={`${c.xp.toLocaleString()} XP, or ${c.xp_in_lair.toLocaleString()} XP fought in its lair`}>
            <Checkbox checked={c.in_lair} onChange={(v) => update({ in_lair: v })} /> in lair
          </label>
        )}
        <div className="chips">
          {c.conditions.map((k) => (
            <span className="chip" key={k}>{titleCase(k)}<button type="button" className="chip-x" aria-label={`Remove ${k}`}
              onClick={() => update({ conditions: c.conditions.filter((x) => x !== k) })}>×</button></span>
          ))}
          <ConditionAdd c={c} onAdd={(k) => update({ conditions: [...c.conditions, k] })} />
        </div>
      </div>
      <div className="ac"><NumInput commit className="num-input tiny" value={c.ac} onChange={(v) => update({ ac: v })} aria-label="Armor class" /><small>AC</small></div>
      <div className="hp">
        <button type="button" className="hp-pill" data-level={hpLevel(pct)} onClick={() => setHpOpen((o) => !o)} aria-label={`HP ${c.hp} of ${c.max_hp}, change`}>
          {c.hp}/{c.max_hp}{c.temp_hp > 0 && <small> +{c.temp_hp}</small>}
        </button>
        {hpOpen && <HpBox c={c} act={act} onClose={() => setHpOpen(false)} />}
      </div>
      <button type="button" className={`icon-btn eye ${c.hidden ? "off" : ""}`} title={c.hidden ? "Hidden from players" : "Visible to players"}
        onClick={() => update({ hidden: !c.hidden })} aria-label="Toggle hidden">{c.hidden ? "◌" : "◉"}</button>
      <button type="button" className="icon-btn" onClick={() => act({ action: "remove", id: c.id })} aria-label={`Remove ${c.name}`}>✕</button>
    </li>
  );
}

function ConditionAdd({ c, onAdd }: { c: Combatant; onAdd: (k: string) => void }) {
  const { meta } = useApp();
  const options = (meta?.conditions || []).filter((k) => !c.conditions.includes(k));
  return (
    <select className="small-select cond-add" value="" aria-label="Add condition" onChange={(e) => e.target.value && onAdd(e.target.value)}>
      <option value="">+ condition</option>
      {options.map((k) => <option key={k} value={k}>{titleCase(k)}</option>)}
    </select>
  );
}

function HpBox({ c, act, onClose }: { c: Combatant; act: (a: EncounterAction) => Promise<void>; onClose: () => void }) {
  const [amount, setAmount] = useState("");
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);
  const apply = async (sign: number) => {
    const n = Math.abs(parseInt(amount, 10)); // a typed minus sign mustn't turn damage into healing
    if (!n) return;
    await act({ action: "hp", id: c.id, amount: sign * n });
    onClose();
  };
  return (
    <div className="hp-box" onKeyDown={(e) => e.key === "Escape" && onClose()}>
      <input ref={input} type="number" min={0} placeholder="0" value={amount} onChange={(e) => setAmount(e.target.value)} aria-label="Amount"
        onKeyDown={(e) => e.key === "Enter" && apply(e.shiftKey ? 1 : -1)} />
      <button type="button" className="btn small danger" onClick={() => apply(-1)} title="Damage (Enter)">Damage</button>
      <button type="button" className="btn small ok" onClick={() => apply(1)} title="Heal (Shift+Enter)">Heal</button>
      <button type="button" className="btn small" onClick={async () => { await act({ action: "update", id: c.id, changes: { temp_hp: parseInt(amount, 10) || 0 } }); onClose(); }}>Temp</button>
    </div>
  );
}

// ------------------------------------------------------------------ adding combatants

type AddTab = "party" | "monster" | "npc" | "custom";

function AddCombatant({ enc, party, act, onDone }: {
  enc: Encounter;
  party: { id: string; name: string }[];
  act: (a: EncounterAction) => Promise<void>;
  onDone: () => void;
}) {
  const dm = useDM();
  const [tab, setTab] = useState<AddTab>("monster");
  const inTracker = new Set(enc.combatants.map((c) => c.character_id).filter(Boolean));
  return (
    <div className="add-panel">
      <div className="pills">
        {(["party", "monster", "npc", "custom"] as AddTab[]).map((t) => (
          <button type="button" key={t} className={`pill ${tab === t ? "on" : ""}`} onClick={() => setTab(t)}>{t === "npc" ? "NPC" : titleCase(t)}</button>
        ))}
        <span className="grow" />
        <button type="button" className="icon-btn" onClick={onDone} aria-label="Close">✕</button>
      </div>
      {tab === "party" && (
        party.length ? (
          <div className="row gap wrap">
            {party.map((p) => (
              <button type="button" key={p.id} className="btn small" disabled={inTracker.has(p.id)}
                onClick={() => act({ action: "add_character", character_ids: [p.id] })}>{p.name}</button>
            ))}
            <button type="button" className="btn small primary" disabled={party.every((p) => inTracker.has(p.id))}
              onClick={() => act({ action: "add_character", character_ids: party.filter((p) => !inTracker.has(p.id)).map((p) => p.id) })}>Add everyone</button>
          </div>
        ) : <p className="muted small">No characters in this campaign yet.</p>
      )}
      {tab === "monster" && <MonsterSearch act={act} />}
      {tab === "npc" && (
        dm.lists.npcs.length ? (
          <div className="row gap wrap">
            {dm.lists.npcs.map((n) => <button type="button" key={n.id} className="btn small" onClick={() => act({ action: "add_npc", npc_id: n.id })}>{n.name}</button>)}
          </div>
        ) : <p className="muted small">No NPCs yet.</p>
      )}
      {tab === "custom" && <CustomCombatant act={act} />}
    </div>
  );
}

function MonsterSearch({ act }: { act: (a: EncounterAction) => Promise<void> }) {
  const dm = useDM();
  const [q, setQ] = useState("");
  const [count, setCount] = useState(1);
  const [average, setAverage] = useState(true);
  const [results, setResults] = useState<SrdEntry[]>([]);
  const search = useDebounced((text: string) => {
    const m = text.trim().match(/^cr\s*([\d/]+)$/i);
    api.srdSearch(m ? { category: "monsters", cr_label: m[1], limit: 30, edition: dm.edition } : { q: text.trim(), category: "monsters", limit: 30, edition: dm.edition })
      .then((r) => setResults(r.results), () => setResults([]));
  }, 200);
  useEffect(() => search(q), [q, search]);
  const homebrew = useMemo(() => dm.lists.monsters.filter((m) => m.name.toLowerCase().includes(q.trim().toLowerCase())), [dm.lists.monsters, q]);
  return (
    <div className="stack">
      <div className="row gap">
        <input type="search" className="grow" autoFocus placeholder="Search monsters (zombie, dragon, CR 1/4…)" value={q} onChange={(e) => setQ(e.target.value)} />
        <label className="row gap small">× <NumInput className="num-input tiny" min={1} max={50} value={count} onChange={(v) => setCount(Math.max(1, v))} /></label>
        <label className="check small"><Checkbox checked={average} onChange={setAverage} /> Average HP</label>
      </div>
      <ul className="list pick-list">
        {homebrew.map((m) => (
          <li key={m.id}><button type="button" className="list-link" onClick={() => act({ action: "add_monster", monster_id: m.id, count, average_hp: average })}>
            <span className="grow">{m.name} <span className="kind-badge">Homebrew</span></span>
            <span className="muted small">CR {m.cr} · AC {m.ac} · HP {m.hp}</span>
          </button></li>
        ))}
        {results.map((e) => (
          <li key={e.id}><button type="button" className="list-link" onClick={() => act({ action: "add_monster", srd_id: e.id, count, average_hp: average })}>
            <span className="grow">{e.name} <span className="kind-badge">SRD</span></span>
            <span className="muted small">{e.summary} · AC {e.data.ac} · HP {e.data.hp}</span>
          </button></li>
        ))}
      </ul>
    </div>
  );
}

function CustomCombatant({ act }: { act: (a: EncounterAction) => Promise<void> }) {
  const [c, setC] = useState({ name: "", kind: "npc" as Combatant["kind"], init_bonus: 0, ac: 10, hp: 10 });
  const set = <K extends keyof typeof c>(k: K) => (v: (typeof c)[K]) => setC((x) => ({ ...x, [k]: v }));
  return (
    <div className="row gap wrap">
      <TextInput value={c.name} onChange={set("name")} placeholder="Name" aria-label="Name" />
      <select value={c.kind} onChange={(e) => set("kind")(e.target.value as Combatant["kind"])} aria-label="Type">
        <option value="npc">NPC</option><option value="monster">Monster</option><option value="pc">PC</option>
      </select>
      <label className="row gap small">Init <NumInput className="num-input tiny" value={c.init_bonus} onChange={set("init_bonus")} /></label>
      <label className="row gap small">AC <NumInput className="num-input tiny" value={c.ac} onChange={set("ac")} /></label>
      <label className="row gap small">HP <NumInput className="num-input tiny" value={c.hp} onChange={set("hp")} /></label>
      <button type="button" className="btn small primary" disabled={!c.name.trim()}
        onClick={async () => { await act({ action: "add", combatant: { ...c, max_hp: c.hp } }); setC({ ...c, name: "" }); }}>Add</button>
    </div>
  );
}
