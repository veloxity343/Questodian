// The DM screen's right-hand panel: Reference / View / Notes, with the dice dock below.

import { useEffect, useMemo, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { api } from "../api";
import { HpBar, Markdown, NumInput, Select, type Option } from "../components/ui";
import { Breakdown, HistoryItem, useDice } from "../context/dice";
import { useToast } from "../context/ui";
import { derive } from "../lib/derive";
import { useDebounced, usePref } from "../lib/hooks";
import { titleCase } from "../lib/util";
import { useApp } from "../context/app";
import type { NoteKind, SrdEntry } from "../types";
import { StatSummary } from "./docs";
import { useDM, type RightTab } from "./store";

const TABS: [RightTab, string][] = [["reference", "Reference"], ["view", "View"], ["notes", "Notes"]];

export function RightPanel() {
  const dm = useDM();
  return (
    <aside className="right-panel" aria-label="Reference panel">
      <nav className="panel-tabs" role="tablist">
        {TABS.map(([t, label]) => (
          <button type="button" key={t} role="tab" aria-selected={dm.rightTab === t} className={dm.rightTab === t ? "on" : ""} onClick={() => dm.setRightTab(t)}>{label}</button>
        ))}
      </nav>
      <div className="panel-body">
        {dm.rightTab === "reference" && <ReferenceTab />}
        {dm.rightTab === "view" && <ViewTab />}
        {dm.rightTab === "notes" && <NotesTab />}
      </div>
      <DiceDock />
    </aside>
  );
}

// ------------------------------------------------------------------ reference

const QUICK: [string, string][] = [["monsters", "Monsters"], ["spells", "Spells"], ["magic-items", "Items"], ["rules", "Rules"]];
const MORE_2014 = ["conditions", "equipment", "classes", "races", "backgrounds", "feats", "skills", "damage-types", "weapon-properties", "languages"];
const MORE_2024 = ["conditions", "equipment", "weapon-mastery", "classes", "species", "backgrounds", "feats", "skills", "poisons", "damage-types", "languages"];
const CRS = ["", "0", "1/8", "1/4", "1/2", ...Array.from({ length: 30 }, (_, i) => String(i + 1))];
const RARITIES = ["", "Common", "Uncommon", "Rare", "Very Rare", "Legendary", "Artifact"];

function ReferenceTab() {
  const dm = useDM();
  const [category, setCategory] = usePref("dm.ref.category", "monsters");
  const [q, setQ] = useState("");
  const [filters, setFilters] = useState<Record<string, string>>({});
  const [results, setResults] = useState<SrdEntry[]>([]);
  const [total, setTotal] = useState(0);
  const search = useDebounced((text: string, cat: string, f: Record<string, string>) => {
    api.srdSearch({ q: text.trim(), category: cat, limit: 80, edition: dm.edition, ...f })
      .then((r) => { setResults(r.results); setTotal(r.total); }, () => setResults([]));
  }, 180);
  useEffect(() => search(q, category, filters), [q, category, filters, search, dm.edition]);
  const homebrew = category === "monsters" ? dm.lists.monsters.filter((m) => m.name.toLowerCase().includes(q.trim().toLowerCase())) : [];
  const more = dm.edition === "2024" ? MORE_2024 : MORE_2014;
  const setFilter = (k: string) => (v: string) => setFilters((f) => ({ ...f, [k]: v }));

  return (
    <div className="ref-tab">
      <div className="sub-tabs">
        {QUICK.map(([c, label]) => (
          <button type="button" key={c} className={category === c ? "on" : ""} onClick={() => { setCategory(c); setFilters({}); }}>{label}</button>
        ))}
        <select aria-label="More categories" value={more.includes(category) ? category : ""} onChange={(e) => { if (e.target.value) { setCategory(e.target.value); setFilters({}); } }}>
          <option value="">More…</option>
          {more.map((c) => <option key={c} value={c}>{titleCase(c)}</option>)}
        </select>
      </div>
      <input type="search" className="wide" placeholder={`Filter ${titleCase(category).toLowerCase()}…`} value={q} onChange={(e) => setQ(e.target.value)} aria-label="Filter" />
      <div className="row gap wrap filter-row">
        {category === "monsters" && <Select aria-label="Challenge rating" value={filters.cr_label ?? ""} onChange={setFilter("cr_label")} options={CRS.map((c): Option => [c, c ? `CR ${c}` : "Any CR"])} />}
        {category === "spells" && (
          <>
            <Select aria-label="Level" value={filters.level ?? ""} onChange={setFilter("level")}
              options={[["", "Any level"], ["0", "Cantrip"], ...[1, 2, 3, 4, 5, 6, 7, 8, 9].map((l): Option => [String(l), `Level ${l}`])]} />
            <Select aria-label="Class" value={filters.classes ?? ""} onChange={setFilter("classes")}
              options={["", "Bard", "Cleric", "Druid", "Paladin", "Ranger", "Sorcerer", "Warlock", "Wizard"].map((c): Option => [c, c || "Any class"])} />
          </>
        )}
        {category === "magic-items" && <Select aria-label="Rarity" value={filters.rarity ?? ""} onChange={setFilter("rarity")} options={RARITIES.map((r): Option => [r, r || "Any rarity"])} />}
        <span className="muted small">{total + homebrew.length} results</span>
      </div>
      <ul className="ref-results">
        {homebrew.map((m) => (
          <li key={m.id}><button type="button" onClick={() => dm.show({ type: "monster", id: m.id })}>
            <span className="ref-name">{m.name}</span><span className="kind-badge">Homebrew</span><span className="muted small">CR {m.cr} · {m.kind}</span>
          </button></li>
        ))}
        {results.map((e) => (
          <li key={e.id}><button type="button" className={dm.view?.type === "srd" && dm.view.id === e.id ? "on" : ""} onClick={() => dm.show({ type: "srd", id: e.id })}>
            <span className="ref-name">{e.name}</span><span className="kind-badge">SRD</span><span className="muted small">{e.summary}</span>
          </button></li>
        ))}
      </ul>
    </div>
  );
}

// ------------------------------------------------------------------ view

function ViewTab() {
  const dm = useDM();
  const v = dm.view;
  if (!v) return <p className="muted pad">Click a monster, spell, rule, NPC or combatant to see it here.</p>;
  if (v.type === "srd") return <SrdView key={v.id} id={v.id} />;
  if (v.type === "monster") {
    const m = dm.lists.monsters.find((x) => x.id === v.id);
    if (!m) return <p className="muted pad">That stat block was deleted.</p>;
    return (
      <div className="view" data-roll-context={m.name}>
        <ViewHead name={m.name} badge="Homebrew" summary={m.kind} onOpen={() => dm.open({ kind: "monsters", id: m.id })} />
        <AddToEncounter monsterId={m.id} />
        <StatSummary m={m} />
        <Markdown source={m.body} rollContext={m.name} />
      </div>
    );
  }
  if (v.type === "npc") {
    const n = dm.lists.npcs.find((x) => x.id === v.id);
    if (!n) return <p className="muted pad">That NPC was deleted.</p>;
    const [kind, ref] = n.stat_ref.split(/:(.*)/s);
    return (
      <div className="view">
        <ViewHead name={n.name} badge="NPC" summary={n.role} onOpen={() => dm.open({ kind: "npcs", id: n.id })} />
        <dl className="facts compact">
          {([["Looks", n.looks], ["Voice", n.voice], ["Wants", n.wants], ["Secret", n.secret], ["Knows", n.knows], ["Stats", n.stats]] as const)
            .filter(([, x]) => x).map(([label, x]) => <div key={label}><dt>{label}</dt><dd><Markdown source={x} /></dd></div>)}
        </dl>
        {kind === "srd" && <SrdView id={ref} embedded />}
      </div>
    );
  }
  return <CharacterView id={v.id} />;
}

function ViewHead({ name, badge, summary, onOpen }: { name: string; badge: string; summary?: string; onOpen?: () => void }) {
  return (
    <div className="view-head">
      <h2>{name} <span className="kind-badge">{badge}</span></h2>
      {summary && <div className="muted"><em>{summary}</em></div>}
      {onOpen && <button type="button" className="link small" onClick={onOpen}>Open as tab →</button>}
    </div>
  );
}

function SrdView({ id, embedded = false }: { id: string; embedded?: boolean }) {
  const dm = useDM();
  const [entry, setEntry] = useState<SrdEntry | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    api.srdEntry(id, dm.edition).then(setEntry, (e: Error) => setError(e.message));
  }, [id, dm.edition]);
  if (error) return <p className="muted pad">{error}</p>;
  if (!entry) return <p className="muted pad">Loading…</p>;
  return (
    <div className="view" data-roll-context={entry.name}>
      {!embedded && <ViewHead name={entry.name} badge="SRD" />}
      {entry.category === "monsters" && <AddToEncounter srdId={entry.id} />}
      <Markdown source={entry.body || ""} rollContext={entry.name} />
    </div>
  );
}

function AddToEncounter({ srdId, monsterId }: { srdId?: string; monsterId?: string }) {
  const dm = useDM();
  const toast = useToast();
  const [count, setCount] = useState(1);
  const target = dm.lists.encounters.find((e) => e.id === dm.encounterId);
  if (!target) {
    return <p className="muted small">Open an encounter to add this to it.</p>;
  }
  return (
    <div className="row gap add-to-enc">
      <button type="button" className="btn small primary" onClick={async () => {
        try {
          dm.upsert("encounters", await api.encounter(target.id, { action: "add_monster", srd_id: srdId, monster_id: monsterId, count }));
          toast(`Added to ${target.name}`);
        } catch (e) {
          toast((e as Error).message, "error");
        }
      }}>⚔ Add to {target.name}</button>
      <label className="row gap small">× <NumInput className="num-input tiny" min={1} max={50} value={count} onChange={(v) => setCount(Math.max(1, v))} /></label>
    </div>
  );
}

function CharacterView({ id }: { id: string }) {
  const dm = useDM();
  const { meta } = useApp();
  const ch = dm.lists.characters.find((c) => c.id === id);
  if (!ch) return <p className="muted pad">Character not found.</p>;
  const d = derive(ch, meta, dm.edition);
  return (
    <div className="view">
      <ViewHead name={ch.name} badge="PC" summary={`${ch.race} ${ch.classes.map((c) => `${c.name} ${c.level}`).join(" / ")}`} />
      <Link className="link small" to={`/characters/${ch.id}`}>Open character sheet →</Link>
      <div className="stat-row">
        <span><b>AC</b> {ch.ac}</span><span><b>HP</b> {ch.hp.current}/{ch.hp.max}</span>
        <span><b>Passive Perception</b> {d.passive.perception}</span><span><b>Speed</b> {ch.speed} ft.</span>
      </div>
      <HpBar current={ch.hp.current} max={ch.hp.max} />
      {ch.conditions.length > 0 && <p>Conditions: {ch.conditions.map(titleCase).join(", ")}</p>}
    </div>
  );
}

// ------------------------------------------------------------------ notes

/** A one-line preview of Markdown: no headings, list markers, emphasis or links syntax. */
const plain = (md: string) =>
  md.replace(/^\s*(#{1,6}|[-*+]|\d+\.|>)\s+/gm, "").replace(/[*_`|]/g, "").replace(/\[\[([^\]|]+)(\|[^\]]+)?\]\]/g, "$1")
    .replace(/\s+/g, " ").trim().slice(0, 160);

const NOTE_FILTERS: ["" | NoteKind, string][] = [["", "All"], ["general", "General"], ["session", "Session"], ["quest", "Quest"], ["pin", "Pin board"]];

function NotesTab() {
  const dm = useDM();
  const { sessionId } = useApp();
  const [filter, setFilter] = usePref<"" | NoteKind>("dm.notes.filter", "");
  const [title, setTitle] = useState("");
  const notes = dm.lists.notes.filter((n) => !filter || n.kind === filter);
  const add = async (kind: NoteKind) => {
    await dm.create("notes", { kind, title: title.trim() || `New ${kind === "pin" ? "idea" : `${kind} note`}`,
      session_id: kind === "session" && dm.lists.sessions.some((s) => s.id === sessionId) ? sessionId : "" });
    setTitle("");
  };
  return (
    <div className="notes-tab">
      <div className="sub-tabs">
        {NOTE_FILTERS.map(([k, label]) => <button type="button" key={k} className={filter === k ? "on" : ""} onClick={() => setFilter(k)}>{label}</button>)}
      </div>
      <div className="row gap">
        <input type="text" className="grow" placeholder="Title for a new note…" value={title} onChange={(e) => setTitle(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add(filter || "general")} aria-label="New note title" />
      </div>
      <div className="row gap wrap quick-add">
        {(["general", "session", "quest", "pin"] as NoteKind[]).map((k) => (
          <button type="button" key={k} className="link small" onClick={() => add(k)}>+ {k === "pin" ? "Pin" : titleCase(k)}</button>
        ))}
      </div>
      {notes.length ? (
        <ul className="note-list">
          {notes.map((n) => (
            <li key={n.id} className={n.done ? "done" : ""}>
              <button type="button" onClick={() => dm.open({ kind: "notes", id: n.id })}>
                <span className="row gap"><strong className="grow">{n.pinned ? "📌 " : ""}{n.title}</strong><span className="kind-badge">{n.kind === "pin" ? "pin" : n.kind}</span></span>
                {n.body && <span className="muted small clamp">{plain(n.body)}</span>}
              </button>
            </li>
          ))}
        </ul>
      ) : <p className="muted pad">No notes yet.</p>}
    </div>
  );
}

// ------------------------------------------------------------------ dice

function DiceDock() {
  const { roll, history, clearHistory } = useDice();
  const [expr, setExpr] = useState("");
  const [open, setOpen] = usePref("dm.dice.history", true);
  const last = history[0];
  const recent = useMemo(() => history.slice(1, 30), [history]);
  const input = useRef<HTMLInputElement>(null);
  return (
    <section className="dice-dock" aria-label="Dice">
      <div className="eyebrow">Dice</div>
      <div className="row gap">
        <input ref={input} type="text" className="grow mono" placeholder="2d6+4" value={expr} aria-label="Dice expression"
          onChange={(e) => setExpr(e.target.value)} onKeyDown={(e) => e.key === "Enter" && expr.trim() && roll(expr.trim())} />
        <button type="button" className="btn primary" onClick={() => expr.trim() && roll(expr.trim())}>Roll</button>
      </div>
      <div className="die-row">
        {[4, 6, 8, 10, 12, 20, 100].map((s) => <button type="button" key={s} className="die-chip" onClick={() => roll(`1d${s}`)}>d{s}</button>)}
      </div>
      <div className={`dice-result ${last?.critical ? "crit" : last?.fumble ? "fumble" : ""}`}>
        {last ? (
          <>
            {last.label && <div className="muted small">{last.label}</div>}
            <div className="dice-total">{last.total}</div>
            <div className="muted small mono"><Breakdown result={last} /></div>
          </>
        ) : <span className="muted small">Click any dice in the text, or roll here.</span>}
      </div>
      <div className="row gap history-head">
        <button type="button" className="link small muted" onClick={() => setOpen(!open)}>{open ? "▾" : "▸"} History</button>
        <span className="grow" />
        <button type="button" className="link small muted" onClick={clearHistory}>clear</button>
      </div>
      {open && <ul className="roll-list">{recent.map((r) => <HistoryItem key={r.time + r.expression} r={r} />)}</ul>}
    </section>
  );
}
