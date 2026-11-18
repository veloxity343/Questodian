// The character builder's rules data: browse it, and change, add or hide anything. Edits are
// kept in the database (and in full exports); the bundled data underneath never changes, so
// any edit can be reset.

import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "../api";
import { Checkbox, EmptyState, LoadError, Loading, NumInput, TextArea, TextInput } from "../components/ui";
import { useDialogs, useToast } from "../context/ui";
import type { Rules } from "../lib/builder";
import { setRules, useRules } from "./builder";

const KINDS: [string, string][] = [
  ["classes", "Classes"], ["species", "Species"], ["backgrounds", "Backgrounds"], ["feats", "Feats"],
  ["armor", "Armor"], ["weapons", "Weapons"], ["masteries", "Masteries"], ["items", "Items"], ["settings", "Settings"],
];
const SETTINGS: [string, string][] = [
  ["point_buy", "Point buy"], ["standard_array", "Standard array"], ["rolled", "Rolled scores"], ["score_cap", "Highest score"],
  ["skills", "Skills and their abilities"], ["ability_uses", "What each ability is for"], ["alignments", "Alignments"], ["fighting_styles", "Fighting styles"],
];

type Entry = Record<string, unknown> & { name: string; source?: string; srd?: boolean };

export function RulesData() {
  const { kind = "classes" } = useParams();
  const { rules, error } = useRules();
  if (error) return <LoadError error={error} />;
  if (!rules) return <Loading />;
  return (
    <>
      <header className="page-head">
        <div>
          <Link to="/characters" className="small">← Characters</Link>
          <h1>Rules data</h1>
        </div>
      </header>
      <p className="muted">
        What the character builder works from (2024 rules). Change anything with Edit: your changes are saved in your data folder, go into full exports,
        and can be reset to the bundled version at any time. Entries marked <span className="tag">not SRD</span> aren't in the System Reference Document.
      </p>
      <nav className="tabs">
        {KINDS.map(([k, label]) => <Link key={k} to={`/rules-data/${k}`} className={k === kind ? "active" : ""}>{label}</Link>)}
      </nav>
      {kind === "settings" ? <SettingsList rules={rules} /> : <EntryList key={kind} kind={kind} rules={rules} />}
    </>
  );
}

function EntryList({ kind, rules }: { kind: string; rules: Rules }) {
  const { confirm, prompt } = useDialogs();
  const toast = useToast();
  const entries = ((rules as unknown as Record<string, Entry[]>)[kind] ?? []);
  const [selected, setSelected] = useState(entries[0]?.name ?? "");
  const [adding, setAdding] = useState<Entry | null>(null);
  const [q, setQ] = useState("");
  const entry = adding ?? entries.find((e) => e.name === selected);
  const hidden = rules.hidden[kind] ?? [];
  const shown = entries.filter((e) => e.name.toLowerCase().includes(q.trim().toLowerCase()));

  const save = async (data: Entry, name: string) => {
    const { source: _source, ...rest } = data;
    setRules(await api.editRule(kind, name, rest));
    setAdding(null);
    setSelected(name);
    toast(`Saved ${name}`);
  };
  const add = async () => {
    const name = (await prompt(`Name of the new ${kind === "species" ? "species" : kind.replace(/s$/, "")}`, { label: "Add" }))?.trim();
    if (!name) return;
    if (entries.some((e) => e.name === name)) return toast(`${name} is already in the list`, "error");
    // Start from a copy of the one on screen, so every field is there to fill in.
    const template = entry ?? entries[0];
    setAdding({ ...structuredClone(template ?? {}), name, source: "added", srd: false } as Entry);
  };
  // A request that fails says so, rather than doing nothing.
  const attempt = async (work: () => Promise<void>) => {
    try {
      await work();
    } catch (err) {
      toast((err as Error).message, "error");
    }
  };
  const reset = async (e: Entry) => {
    const what = e.source === "added" ? `Delete ${e.name}? It's one you added.` : `Reset ${e.name} to the bundled version? Your changes to it are lost.`;
    if (!(await confirm(what, { label: e.source === "added" ? "Delete" : "Reset" }))) return;
    await attempt(async () => {
      setRules(await api.resetRule(kind, e.name));
      if (e.source === "added") setSelected(entries[0]?.name ?? "");
      toast(e.source === "added" ? `Deleted ${e.name}` : `${e.name} is back to the bundled version`);
    });
  };
  const hide = async (e: Entry) => {
    if (!(await confirm(`Hide ${e.name}? The builder stops offering it. Characters already using it keep their choice, but it won't be worked out. You can bring it back below the list.`, { label: "Hide" }))) return;
    await attempt(async () => {
      setRules(await api.editRule(kind, e.name, null));
      setSelected(entries.find((x) => x.name !== e.name)?.name ?? "");
    });
  };
  const restore = async (name: string) => {
    if (!(await confirm(`Bring back ${name}?`, { label: "Bring back", danger: false }))) return;
    await attempt(async () => {
      setRules(await api.resetRule(kind, name));
      setSelected(name);
    });
  };

  return (
    <div className="rules-layout">
      <aside className="panel">
        <div className="row gap">
          <input type="search" className="grow" placeholder="Filter…" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Filter" />
          <button type="button" className="btn small" onClick={add}>+ Add</button>
        </div>
        <ul className="list">
          {shown.map((e) => (
            <li key={e.name}>
              <button type="button" className={`list-link ${!adding && e.name === selected ? "selected" : ""}`} onClick={() => { setAdding(null); setSelected(e.name); }}>
                <span className="grow">{e.name}</span>
                <SourceTags e={e} />
              </button>
            </li>
          ))}
        </ul>
        {hidden.length > 0 && (
          <>
            <h3 className="small-head">Hidden</h3>
            <ul className="list">
              {hidden.map((name) => (
                <li key={name} className="row gap"><span className="grow muted">{name}</span><button type="button" className="btn small" onClick={() => restore(name)}>Bring back</button></li>
              ))}
            </ul>
          </>
        )}
      </aside>
      {entry ? (
        <EntryEditor key={`${entry.name}-${adding ? "new" : entry.source}`} entry={entry} isNew={!!adding} onSave={save} onCancel={() => setAdding(null)}
          onReset={entry.source !== "bundled" && !adding ? () => reset(entry) : undefined}
          onHide={entry.source === "bundled" ? () => hide(entry) : undefined} />
      ) : <EmptyState title="Nothing here" text="Add an entry, or bring back a hidden one." />}
    </div>
  );
}

function SourceTags({ e }: { e: Entry }) {
  return (
    <span className="row gap-sm">
      {e.srd === false && <span className="tag src-tag">not SRD</span>}
      {e.source === "edited" && <span className="tag src-tag">edited</span>}
      {e.source === "added" && <span className="tag src-tag">yours</span>}
    </span>
  );
}

/** One entry: read it, or Edit → change → Done (with a confirmation) to save. */
function EntryEditor({ entry, isNew, onSave, onCancel, onReset, onHide }: {
  entry: Entry;
  isNew: boolean;
  onSave: (data: Entry, name: string) => Promise<void>;
  onCancel: () => void;
  onReset?: () => void;
  onHide?: () => void;
}) {
  const { confirm } = useDialogs();
  const toast = useToast();
  const [editing, setEditing] = useState(isNew);
  const [draft, setDraft] = useState<Entry>(entry);
  const [broken, setBroken] = useState<Set<string>>(new Set());
  // Reset the draft when the saved entry changes (not just when a new object with the same content arrives).
  const saved = JSON.stringify(entry);
  useEffect(() => setDraft(JSON.parse(saved) as Entry), [saved]);
  const changed = JSON.stringify(draft) !== JSON.stringify(entry);

  const done = async () => {
    if (broken.size) return toast(`Fix the field${broken.size > 1 ? "s" : ""} marked in red first: ${[...broken].join(", ")}`, "error");
    if (!changed && !isNew) return setEditing(false);
    if (!(await confirm(`Save your changes to ${entry.name}? The builder uses them for every character straight away.`, { label: "Save", danger: false }))) return;
    try {
      await onSave(draft, entry.name);
      setEditing(false);
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };
  const cancel = async () => {
    if (changed && !(await confirm(`Discard your changes to ${entry.name}?`, { label: "Discard" }))) return;
    setDraft(entry);
    setBroken(new Set());
    setEditing(false);
    if (isNew) onCancel();
  };
  const fields = Object.keys(draft).filter((k) => k !== "name" && k !== "source");

  return (
    <section className="panel rules-entry">
      <div className="panel-head">
        <h2>{entry.name} <SourceTags e={entry} /></h2>
        <div className="row gap wrap">
          {editing ? (
            <>
              <button type="button" className="btn small" onClick={cancel}>Cancel</button>
              <button type="button" className="btn small primary" onClick={done}>Done</button>
            </>
          ) : (
            <>
              {onReset && <button type="button" className="btn small" onClick={onReset}>{entry.source === "added" ? "Delete" : "Reset to bundled"}</button>}
              {onHide && <button type="button" className="btn small" onClick={onHide}>Hide</button>}
              <button type="button" className="btn small" onClick={() => setEditing(true)}>Edit</button>
            </>
          )}
        </div>
      </div>
      <dl className="rules-fields">
        {fields.map((k) => (
          <div key={k}>
            <dt>{k.replace(/_/g, " ")}</dt>
            <dd>
              {editing ? (
                <ValueEditor value={draft[k]} shape={entry[k]} name={k} onChange={(v) => setDraft((d) => ({ ...d, [k]: v }))}
                  onBroken={(bad) => setBroken((s) => { const n = new Set(s); if (bad) n.add(k); else n.delete(k); return n; })} />
              ) : <ValueView value={entry[k]} />}
            </dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

function ValueView({ value }: { value: unknown }) {
  if (value === null || value === undefined || value === "") return <span className="muted">—</span>;
  if (typeof value === "boolean") return <>{value ? "Yes" : "No"}</>;
  if (Array.isArray(value) && value.every((v) => typeof v !== "object")) return <>{value.join(", ")}</>;
  if (typeof value === "object") return <pre className="rules-json">{JSON.stringify(value, null, 1)}</pre>;
  return <>{String(value)}</>;
}

/** An editor fitting the value: text, number, yes/no, a comma list, or JSON for anything nested. */
/** The editor is chosen by the saved value's shape, so it doesn't change while typing (clearing a list, say). */
function ValueEditor({ value, shape, name, onChange, onBroken }: {
  value: unknown;
  shape: unknown;
  name: string;
  onChange: (v: unknown) => void;
  onBroken: (bad: boolean) => void;
}) {
  if (typeof value === "boolean") return <Checkbox checked={value} onChange={onChange} aria-label={name} />;
  if (typeof value === "number") return <NumInput value={value} onChange={onChange} aria-label={name} />;
  if (typeof value === "string") {
    return value.length > 60 || /text|traits/.test(name)
      ? <TextArea rows={3} value={value} onChange={onChange} aria-label={name} />
      : <TextInput value={value} onChange={onChange} aria-label={name} />;
  }
  // A comma list for lists of names; lists with blank places (column labels) keep their positions in JSON.
  if (Array.isArray(shape) && shape.length > 0 && shape.every((v) => typeof v === "string" && v.trim() && !v.includes(","))) {
    return <ListText value={Array.isArray(value) ? (value as string[]) : []} onChange={onChange} name={name} />;
  }
  return <JsonText value={value} onChange={onChange} onBroken={onBroken} name={name} />;
}

function ListText({ value, onChange, name }: { value: string[]; onChange: (v: string[]) => void; name: string }) {
  const [text, setText] = useState(value.join(", "));
  return (
    <TextInput value={text} aria-label={name} onChange={(t) => { setText(t); onChange(t.split(",").map((x) => x.trim()).filter(Boolean)); }} />
  );
}

function JsonText({ value, onChange, onBroken, name }: { value: unknown; onChange: (v: unknown) => void; onBroken: (bad: boolean) => void; name: string }) {
  const [text, setText] = useState(JSON.stringify(value, null, 1));
  const [error, setError] = useState("");
  return (
    <>
      <TextArea className={`rules-json ${error ? "invalid" : ""}`} rows={Math.min(16, text.split("\n").length + 1)} value={text} aria-label={name}
        onChange={(t) => {
          setText(t);
          try {
            onChange(JSON.parse(t));
            setError("");
            onBroken(false);
          } catch (e) {
            setError((e as Error).message);
            onBroken(true);
          }
        }} />
      {error && <small className="bad-text">Not valid JSON: {error}</small>}
    </>
  );
}

function SettingsList({ rules }: { rules: Rules }) {
  const { confirm } = useDialogs();
  const toast = useToast();
  const data = rules as unknown as Record<string, unknown>;
  const save = async (key: string, value: unknown) => {
    setRules(await api.editRule("settings", key, value));
    toast("Saved");
  };
  return (
    <div className="stack">
      {SETTINGS.map(([key, title]) => {
        const edited = rules.edited_settings.includes(key);
        const entry: Entry = { name: title, source: edited ? "edited" : "bundled", value: data[key] };
        return (
          <EntryEditor key={`${key}-${JSON.stringify(data[key])}`} entry={entry} isNew={false}
            onSave={(d) => save(key, d.value)} onCancel={() => {}}
            onReset={edited ? async () => {
              if (!(await confirm(`Reset ${title.toLowerCase()} to the bundled version?`, { label: "Reset" }))) return;
              try {
                setRules(await api.resetRule("settings", key));
              } catch (err) {
                toast((err as Error).message, "error");
              }
            } : undefined} />
        );
      })}
    </div>
  );
}
