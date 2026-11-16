// Workspace documents: scenes, NPCs, notes, homebrew monsters and trackers.

import { useState } from "react";
import { api } from "../api";
import { Checkbox, Markdown, NumInput, Select, TextInput, type Option } from "../components/ui";
import { useToast } from "../context/ui";
import { ABILITIES, ABILITY_NAMES, modOf, signed } from "../lib/util";
import type { Monster, Note, NoteKind, Npc, Scene, SceneStatus, Tracker } from "../types";
import { DocHeader, MdField, Pills, Section, useDeleteRecord, useRecordDraft } from "./parts";
import { useDM } from "./store";

const SCENE_STATUSES: SceneStatus[] = ["planned", "active", "completed", "skipped"];

// ------------------------------------------------------------------ scene

export function SceneDoc({ initial }: { initial: Scene }) {
  const dm = useDM();
  const { doc, edit, discard, status } = useRecordDraft("scenes", initial);
  const [editing, setEditing] = useState(false);
  const onDelete = useDeleteRecord("scenes", initial.id, initial.title, discard);
  if (!doc) return null;
  const set = <K extends keyof Scene>(k: K) => (v: Scene[K]) => edit((d) => void (d[k] = v));
  const session = dm.lists.sessions.find((s) => s.id === doc.session_id);
  const encounters = dm.lists.encounters.filter((e) => e.scene_id === doc.id);

  return (
    <article className="doc">
      <DocHeader
        kind="scenes" status={status} editing={editing} setEditing={setEditing} onDelete={onDelete} linkName={doc.title}
        title={editing ? <TextInput className="title-input" value={doc.title} onChange={set("title")} aria-label="Scene title" /> : doc.title}
        subtitle={editing ? (
          <div className="row gap wrap">
            <TextInput value={doc.location} onChange={set("location")} placeholder="Location" aria-label="Location" />
            <Select aria-label="Session" value={doc.session_id}
              options={[["", "No session"], ...dm.lists.sessions.map((s): Option => [s.id, `Session ${s.number}: ${s.title}`])]}
              onChange={set("session_id")} />
            <label className="row gap small">Order <NumInput value={doc.position} onChange={set("position")} className="num-input tiny" /></label>
          </div>
        ) : (
          <span className="muted">
            {doc.location || "—"}
            {session && <> · <button type="button" className="link" onClick={() => dm.open({ kind: "sessions", id: session.id })}>Session {session.number}</button></>}
          </span>
        )}
      />
      <Pills value={doc.status} options={SCENE_STATUSES} onChange={set("status")} />
      <Section label="Summary" value={doc.summary} editing={editing} onChange={set("summary")} rows={3}
        placeholder="One or two lines: what this scene is for." />
      <Section label="Read aloud" variant="read-aloud" value={doc.read_aloud} editing={editing} onChange={set("read_aloud")} rows={5}
        placeholder="Boxed text to read to the players." />
      <Section label="DM notes" value={doc.body} editing={editing} onChange={set("body")} rows={18} rollContext={doc.title}
        placeholder="Checks (DC 12 Wisdom), NPCs ([[Captain Vane]]), monsters, secrets, tables… Markdown." />
      {!editing && !doc.summary && !doc.read_aloud && !doc.body && (
        <p className="muted">Nothing here yet. Press <strong>Edit</strong> to write this scene.</p>
      )}
      <section className="doc-section">
        <div className="eyebrow">Encounters</div>
        <div className="row gap wrap">
          {encounters.map((e) => (
            <button type="button" key={e.id} className="btn small" onClick={() => dm.open({ kind: "encounters", id: e.id })}>⚔ {e.name}</button>
          ))}
          <button type="button" className="btn small ghost"
            onClick={() => dm.create("encounters", { name: doc.title, scene_id: doc.id, session_id: doc.session_id })}>
            + Encounter for this scene
          </button>
        </div>
      </section>
    </article>
  );
}

// ------------------------------------------------------------------ NPC

const NPC_ROWS: [keyof Npc, string, string][] = [
  ["looks", "Looks", "Grey robes drifting as if underwater…"],
  ["voice", "Voice and manner", "Soft, formal, old-fashioned."],
  ["wants", "Wants", ""],
  ["secret", "Secret", ""],
  ["knows", "What they know", ""],
];

export function NpcDoc({ initial }: { initial: Npc }) {
  const dm = useDM();
  const toast = useToast();
  const { doc, edit, discard, status, flush } = useRecordDraft("npcs", initial);
  const [editing, setEditing] = useState(false);
  const onDelete = useDeleteRecord("npcs", initial.id, initial.name, discard);
  if (!doc) return null;
  const set = <K extends keyof Npc>(k: K) => (v: Npc[K]) => edit((d) => void (d[k] = v));
  const [refKind, refId] = doc.stat_ref.split(/:(.*)/s);
  const homebrew = refKind === "monster" ? dm.lists.monsters.find((m) => m.id === refId) : undefined;

  const toEncounter = async () => {
    if (!dm.encounterId) return toast("Open an encounter first", "error");
    await flush();
    try {
      dm.upsert("encounters", await api.encounter(dm.encounterId, { action: "add_npc", npc_id: doc.id }));
      toast(`${doc.name} joins the encounter`);
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };

  return (
    <article className="doc">
      <DocHeader
        kind="npcs" status={status} editing={editing} setEditing={setEditing} onDelete={onDelete} linkName={doc.name}
        extra={<button type="button" className="btn small" onClick={toEncounter}>⚔ To encounter</button>}
        title={editing ? <TextInput className="title-input" value={doc.name} onChange={set("name")} aria-label="Name" /> : doc.name}
        subtitle={editing ? (
          <div className="row gap wrap">
            <TextInput value={doc.role} onChange={set("role")} placeholder="Role (Village child, 9)" aria-label="Role" />
            <TextInput value={doc.first_appears} onChange={set("first_appears")} placeholder="First appears" aria-label="First appears" />
          </div>
        ) : (
          <span className="muted">{[doc.role, doc.first_appears && `First appears: ${doc.first_appears}`].filter(Boolean).join(" · ")}</span>
        )}
      />
      <dl className="facts">
        {NPC_ROWS.map(([key, label, ph]) =>
          editing || doc[key] ? (
            <div key={key} className={key === "secret" ? "secret" : ""}>
              <dt>{label}</dt>
              <dd>{editing ? <MdField rows={2} value={doc[key] as string} onChange={set(key) as (v: string) => void} placeholder={ph} aria-label={label} />
                : <Markdown source={doc[key] as string} rollContext={doc.name} />}</dd>
            </div>
          ) : null,
        )}
        <div>
          <dt>Stats</dt>
          <dd>
            {editing ? (
              <div className="stack">
                <TextInput value={doc.stats} onChange={set("stats")} placeholder="Commoner, or 'acts through her holy symbol'" aria-label="Stats" />
                <StatRefPicker value={doc.stat_ref} onChange={set("stat_ref")} />
              </div>
            ) : (
              <span className="row gap wrap">
                {doc.stats || "—"}
                {refKind === "srd" && <button type="button" className="btn small" onClick={() => dm.show({ type: "srd", id: refId })}>Stat block</button>}
                {homebrew && <button type="button" className="btn small" onClick={() => dm.show({ type: "monster", id: homebrew.id })}>{homebrew.name}</button>}
              </span>
            )}
          </dd>
        </div>
      </dl>
      <Section label="Notes from play" value={doc.notes} editing={editing} onChange={set("notes")} rows={6} />
    </article>
  );
}

/** Link an NPC to an SRD monster or homebrew stat block (used for the tracker and the View panel). */
function StatRefPicker({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const dm = useDM();
  const [q, setQ] = useState("");
  const [results, setResults] = useState<{ id: string; name: string; summary: string }[]>([]);
  const search = async (text: string) => {
    setQ(text);
    if (text.trim().length < 2) return setResults([]);
    const r = await api.srdSearch({ q: text, category: "monsters", limit: 8, edition: dm.edition });
    setResults(r.results);
  };
  const label = value.startsWith("srd:") ? value.slice(4).replace("monsters/", "SRD: ")
    : value.startsWith("monster:") ? `Homebrew: ${dm.lists.monsters.find((m) => `monster:${m.id}` === value)?.name ?? "?"}` : "No stat block";
  return (
    <div className="stat-ref">
      <div className="row gap small"><span className="muted">{label}</span>{value && <button type="button" className="link" onClick={() => onChange("")}>clear</button>}</div>
      <div className="row gap">
        <input type="search" placeholder="Link an SRD monster (commoner, bandit…)" value={q} onChange={(e) => search(e.target.value)} />
        <Select aria-label="Homebrew stat block" value="" onChange={(v) => v && onChange(`monster:${v}`)}
          options={[["", "…or homebrew"], ...dm.lists.monsters.map((m): Option => [m.id, m.name])]} />
      </div>
      {results.length > 0 && (
        <ul className="list pick-list">
          {results.map((r) => (
            <li key={r.id}><button type="button" className="list-link" onClick={() => { onChange(`srd:${r.id}`); setResults([]); setQ(""); }}>
              <span className="grow">{r.name}</span><span className="muted small">{r.summary}</span>
            </button></li>
          ))}
        </ul>
      )}
    </div>
  );
}

// ------------------------------------------------------------------ note

const NOTE_KINDS: NoteKind[] = ["general", "session", "quest", "pin"];

export function NoteDoc({ initial }: { initial: Note }) {
  const dm = useDM();
  const { doc, edit, discard, status } = useRecordDraft("notes", initial);
  const [editing, setEditing] = useState(!initial.body);
  const onDelete = useDeleteRecord("notes", initial.id, initial.title, discard);
  if (!doc) return null;
  const set = <K extends keyof Note>(k: K) => (v: Note[K]) => edit((d) => void (d[k] = v));
  return (
    <article className="doc">
      <DocHeader
        kind="notes" status={status} editing={editing} setEditing={setEditing} onDelete={onDelete} linkName={doc.title}
        title={editing ? <TextInput className="title-input" value={doc.title} onChange={set("title")} aria-label="Title" /> : doc.title}
        subtitle={
          <div className="row gap wrap">
            <Pills value={doc.kind} options={NOTE_KINDS} onChange={set("kind")} labels={{ pin: "Pin board" }} />
            {doc.kind === "quest" && <label className="check small"><Checkbox checked={doc.done} onChange={set("done")} /> Done</label>}
            <label className="check small"><Checkbox checked={doc.pinned} onChange={set("pinned")} /> Pinned</label>
            {editing && (
              <Select aria-label="Session" value={doc.session_id}
                options={[["", "No session"], ...dm.lists.sessions.map((s): Option => [s.id, `Session ${s.number}`])]} onChange={set("session_id")} />
            )}
          </div>
        }
      />
      <Section label="" value={doc.body} editing={editing} onChange={set("body")} rows={20} rollContext={doc.title} />
    </article>
  );
}

// ------------------------------------------------------------------ homebrew monster

export function MonsterDoc({ initial }: { initial: Monster }) {
  const dm = useDM();
  const toast = useToast();
  const { doc, edit, discard, status, flush } = useRecordDraft("monsters", initial);
  const [editing, setEditing] = useState(false);
  const onDelete = useDeleteRecord("monsters", initial.id, initial.name, discard);
  if (!doc) return null;
  const set = <K extends keyof Monster>(k: K) => (v: Monster[K]) => edit((d) => void (d[k] = v));
  const toEncounter = async () => {
    if (!dm.encounterId) return toast("Open an encounter first", "error");
    await flush();
    dm.upsert("encounters", await api.encounter(dm.encounterId, { action: "add_monster", monster_id: doc.id }));
    toast(`${doc.name} added`);
  };
  return (
    <article className="doc">
      <DocHeader
        kind="monsters" status={status} editing={editing} setEditing={setEditing} onDelete={onDelete} linkName={doc.name}
        extra={<button type="button" className="btn small" onClick={toEncounter}>⚔ To encounter</button>}
        title={editing ? <TextInput className="title-input" value={doc.name} onChange={set("name")} aria-label="Name" /> : doc.name}
        subtitle={editing
          ? <TextInput value={doc.kind} onChange={set("kind")} placeholder="Medium undead, neutral evil" aria-label="Size, type, alignment" />
          : <em className="muted">{doc.kind}</em>}
      />
      {editing ? (
        <div className="form-grid stat-form">
          <label className="field"><span>AC</span><NumInput value={doc.ac} onChange={set("ac")} /></label>
          <label className="field"><span>HP</span><NumInput value={doc.hp} onChange={set("hp")} /></label>
          <label className="field"><span>HP dice</span><TextInput value={doc.hp_roll} onChange={set("hp_roll")} placeholder="5d8 + 15" /></label>
          <label className="field"><span>Speed</span><TextInput value={doc.speed} onChange={set("speed")} /></label>
          <label className="field"><span>Initiative</span><NumInput value={doc.init_bonus} onChange={set("init_bonus")} /></label>
          <label className="field"><span>CR</span><TextInput value={doc.cr} onChange={set("cr")} /></label>
          <label className="field"><span>XP</span><NumInput value={doc.xp} onChange={set("xp")} /></label>
          {ABILITIES.map((a) => (
            <label className="field" key={a}><span>{a.toUpperCase()}</span>
              <NumInput value={doc.abilities[a] ?? 10} onChange={(v) => edit((d) => void (d.abilities[a] = v))} aria-label={ABILITY_NAMES[a]} />
            </label>
          ))}
        </div>
      ) : (
        <StatSummary m={doc} />
      )}
      <Section label="Traits and actions" value={doc.body} editing={editing} onChange={set("body")} rows={14} rollContext={doc.name}
        placeholder={"**Rope Lash (action):** Melee +5, reach 10 ft. 8 (1d8 + 3) bludgeoning…"} />
      <Section label="DM notes" value={doc.notes} editing={editing} onChange={set("notes")} rows={4} />
    </article>
  );
}

export function StatSummary({ m }: { m: Monster }) {
  return (
    <div className="statline">
      <div className="stat-row">
        <span><b>AC</b> {m.ac}</span><span><b>HP</b> {m.hp}{m.hp_roll ? ` (${m.hp_roll})` : ""}</span>
        <span><b>Speed</b> {m.speed}</span><span><b>Initiative</b> {signed(m.init_bonus)}</span>
        <span><b>CR</b> {m.cr} ({m.xp.toLocaleString()} XP)</span>
      </div>
      <div className="ability-strip">
        {ABILITIES.map((a) => (
          <div key={a}><span>{a.toUpperCase()}</span><b>{m.abilities[a] ?? 10}</b><small>{signed(modOf(m.abilities[a] ?? 10))}</small></div>
        ))}
      </div>
    </div>
  );
}

// ------------------------------------------------------------------ tracker

/** A clock: click a step to set the count. Steps can carry labels ("Entering the nave"). */
export function TrackerWidget({ tracker, onChange }: { tracker: Tracker; onChange: (t: Tracker) => void }) {
  const set = (value: number) => onChange({ ...tracker, value: Math.max(0, Math.min(tracker.max, value)) });
  const next = tracker.steps[tracker.value]?.label;
  return (
    <div className="tracker-widget">
      <div className="tracker-head">
        <strong>{tracker.name}</strong>
        <span className="tracker-count">{tracker.value} <small>of {tracker.max}</small></span>
        <button type="button" className="btn small" onClick={() => set(tracker.value - 1)} aria-label="Back one">−</button>
        <button type="button" className="btn small primary" onClick={() => set(tracker.value + 1)} aria-label="Advance">+</button>
      </div>
      <div className="clock">
        {Array.from({ length: tracker.max }, (_, i) => (
          <button type="button" key={i} className={`clock-step ${i < tracker.value ? "done" : ""}`}
            title={tracker.steps[i]?.label || `Step ${i + 1}`} onClick={() => set(i < tracker.value ? i : i + 1)}>
            {i + 1}
          </button>
        ))}
      </div>
      {next && tracker.value < tracker.max && <div className="muted small">Next: {next}</div>}
      {tracker.value >= tracker.max && <div className="tracker-full small">Complete</div>}
    </div>
  );
}

export function TrackerDoc({ initial }: { initial: Tracker }) {
  const dm = useDM();
  const { doc, edit, discard, status } = useRecordDraft("trackers", initial);
  const [editing, setEditing] = useState(false);
  const onDelete = useDeleteRecord("trackers", initial.id, initial.name, discard);
  if (!doc) return null;
  const set = <K extends keyof Tracker>(k: K) => (v: Tracker[K]) => edit((d) => void (d[k] = v));
  return (
    <article className="doc">
      <DocHeader kind="trackers" status={status} editing={editing} setEditing={setEditing} onDelete={onDelete} linkName={doc.name}
        title={editing ? <TextInput className="title-input" value={doc.name} onChange={set("name")} aria-label="Name" /> : doc.name} />
      <TrackerWidget tracker={doc} onChange={(t) => edit((d) => void (d.value = t.value))} />
      {editing && (
        <div className="stack">
          <div className="row gap wrap">
            <label className="row gap">Steps <NumInput value={doc.max} min={1} max={100} onChange={(v) => edit((d) => {
              d.max = Math.max(1, Math.min(100, v));
              d.steps = Array.from({ length: d.max }, (_, i) => d.steps[i] ?? { label: "" });
            })} /></label>
            <Select aria-label="Session" value={doc.session_id}
              options={[["", "Whole campaign"], ...dm.lists.sessions.map((s): Option => [s.id, `Session ${s.number}`])]} onChange={set("session_id")} />
          </div>
          <ol className="step-labels">
            {Array.from({ length: doc.max }, (_, i) => (
              <li key={i}><TextInput value={doc.steps[i]?.label ?? ""} placeholder={`What happens at ${i + 1}`}
                onChange={(v) => edit((d) => {
                  d.steps = Array.from({ length: d.max }, (_, j) => d.steps[j] ?? { label: "" });
                  d.steps[i] = { label: v };
                })} /></li>
            ))}
          </ol>
        </div>
      )}
      <Section label="Notes" value={doc.notes} editing={editing} onChange={set("notes")} rows={4} />
    </article>
  );
}
