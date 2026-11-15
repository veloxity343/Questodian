// Building blocks shared by the DM screen's document views.

import { useMemo, useRef, useState, type ReactNode, type TextareaHTMLAttributes } from "react";
import { api } from "../api";
import { Markdown, SaveStatus } from "../components/ui";
import { useDialogs, useToast } from "../context/ui";
import { useDraft } from "../lib/hooks";
import type { KindMap } from "../types";
import { KIND_LABEL, useDM, type DocKind, type ListKind } from "./store";

// ------------------------------------------------------------------ Markdown field with [[link]] suggestions

interface Suggestion {
  label: string;
  kind: string;
}

function useSuggestions(): Suggestion[] {
  const { lists } = useDM();
  return useMemo(() => {
    const out: Suggestion[] = [];
    const add = (kind: ListKind, label: string) => out.push({ kind, label });
    for (const n of lists.npcs) add("npcs", n.name);
    for (const s of lists.scenes) add("scenes", s.title);
    for (const n of lists.notes) add("notes", n.title);
    for (const h of lists.handouts) add("handouts", h.title);
    for (const e of lists.encounters) add("encounters", e.name);
    for (const m of lists.monsters) add("monsters", m.name);
    for (const t of lists.trackers) add("trackers", t.name);
    for (const c of lists.characters) add("characters", c.name);
    return out.filter((s) => s.label);
  }, [lists]);
}

const SINGLE: Record<string, string> = {
  npcs: "NPC", scenes: "Scene", notes: "Note", handouts: "Handout", encounters: "Encounter", monsters: "Homebrew",
  trackers: "Tracker", characters: "PC",
};

/**
 * A textarea for Markdown. Typing `[[` suggests campaign records to link; Enter or
 * Tab picks, Esc closes just the suggestions.
 */
export function MdField({ value, onChange, rows = 6, ...rest }: Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "value" | "onChange"> & {
  value: string;
  onChange: (v: string) => void;
}) {
  const all = useSuggestions();
  const ref = useRef<HTMLTextAreaElement>(null);
  const [query, setQuery] = useState<string | null>(null);
  const [index, setIndex] = useState(0);

  const matches = useMemo(() => {
    if (query === null) return [];
    const q = query.toLowerCase();
    // Names that start with what you typed first, then a word inside the name, then anywhere.
    const rank = (label: string) => {
      const l = label.toLowerCase();
      return l.startsWith(q) ? 0 : l.split(/[\s'’-]+/).some((w) => w.startsWith(q)) ? 1 : 2;
    };
    return all
      .filter((s) => s.label.toLowerCase().includes(q))
      .sort((a, b) => rank(a.label) - rank(b.label) || a.label.localeCompare(b.label))
      .slice(0, 8);
  }, [all, query]);

  const scan = (el: HTMLTextAreaElement) => {
    const before = el.value.slice(0, el.selectionStart);
    const m = before.match(/\[\[([^\]\n|]{0,40})$/);
    setQuery(m ? m[1] : null);
    setIndex(0);
  };
  const pick = (s: Suggestion) => {
    const el = ref.current;
    if (!el) return;
    const start = el.selectionStart;
    const before = el.value.slice(0, start).replace(/\[\[([^\]\n|]{0,40})$/, `[[${s.label}]]`);
    const after = el.value.slice(start).replace(/^[^\]\n]*\]\]/, "");
    onChange(before + after);
    setQuery(null);
    requestAnimationFrame(() => {
      el.focus();
      el.setSelectionRange(before.length, before.length);
    });
  };

  return (
    <div className="md-field">
      <textarea
        ref={ref}
        rows={rows}
        {...rest}
        value={value ?? ""}
        onChange={(e) => {
          onChange(e.target.value);
          scan(e.target);
        }}
        onKeyDown={(e) => {
          if (matches.length) {
            if (e.key === "ArrowDown" || e.key === "ArrowUp") {
              e.preventDefault();
              setIndex((i) => (i + (e.key === "ArrowDown" ? 1 : matches.length - 1)) % matches.length);
              return;
            }
            if (e.key === "Enter" || e.key === "Tab") {
              e.preventDefault();
              pick(matches[index]);
              return;
            }
          }
          if (e.key === "Escape" && query !== null) {
            e.stopPropagation();
            setQuery(null);
          }
        }}
        onBlur={() => setTimeout(() => setQuery(null), 150)}
      />
      {matches.length > 0 && (
        <ul className="suggest" role="listbox">
          {matches.map((s, i) => (
            <li key={`${s.kind}-${s.label}`} role="option" aria-selected={i === index}>
              <button type="button" className={i === index ? "active" : ""} onMouseDown={(e) => { e.preventDefault(); pick(s); }}>
                <span className="kind-badge">{SINGLE[s.kind]}</span> {s.label}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

// ------------------------------------------------------------------ sections and headers

/** A labelled block that reads as rendered Markdown and edits as a textarea, in the same place. */
export function Section({ label, value, editing, onChange, rows = 5, placeholder, variant, rollContext }: {
  label: string;
  value: string;
  editing: boolean;
  onChange: (v: string) => void;
  rows?: number;
  placeholder?: string;
  variant?: "read-aloud" | "secret";
  rollContext?: string;
}) {
  if (!editing && !value.trim()) return null;
  return (
    <section className={`doc-section ${variant ?? ""}`}>
      <div className="eyebrow">{label}</div>
      {editing ? (
        <MdField value={value} onChange={onChange} rows={rows} placeholder={placeholder} aria-label={label} />
      ) : (
        <Markdown source={value} rollContext={rollContext} />
      )}
    </section>
  );
}

export function Pills<T extends string>({ value, options, onChange, labels }: {
  value: T;
  options: readonly T[];
  onChange: (v: T) => void;
  labels?: Partial<Record<T, string>>;
}) {
  return (
    <div className="pills" role="radiogroup">
      {options.map((o) => (
        <button type="button" key={o} role="radio" aria-checked={o === value} className={`pill ${o === value ? "on" : ""} pill-${o}`} onClick={() => onChange(o)}>
          {labels?.[o] ?? o.charAt(0).toUpperCase() + o.slice(1)}
        </button>
      ))}
    </div>
  );
}

/** Autosaving draft of one record that keeps the DM store's copy (lists, tab titles) current. */
export function useRecordDraft<K extends ListKind>(kind: K, initial: KindMap[K]) {
  const dm = useDM();
  const toast = useToast();
  return useDraft<KindMap[K]>(
    initial,
    async (doc) => {
      const saved = await api.update(kind, doc.id, doc);
      dm.upsert(kind, saved);
    },
    { onError: (e) => toast(e.message, "error") },
  );
}

export function DocHeader({ kind, title, subtitle, editing, setEditing, status, onDelete, extra, linkName }: {
  kind: DocKind;
  title: ReactNode;
  subtitle?: ReactNode;
  editing?: boolean;
  setEditing?: (v: boolean) => void;
  status?: string;
  onDelete?: () => void;
  extra?: ReactNode;
  linkName?: string;
}) {
  const toast = useToast();
  return (
    <header className="doc-head">
      <div className="doc-title">
        <span className="kind-badge">{KIND_LABEL[kind]}</span>
        <h1>{title}</h1>
        {subtitle && <div className="doc-sub">{subtitle}</div>}
      </div>
      <div className="doc-actions">
        {status && <SaveStatus status={status} />}
        {extra}
        {linkName && (
          <button type="button" className="btn ghost small" title="Copy a [[link]] to paste into notes"
            onClick={() => navigator.clipboard?.writeText(`[[${linkName}]]`).then(() => toast("Link copied"))}>
            Copy link
          </button>
        )}
        {setEditing && (
          <button type="button" className={`btn small ${editing ? "primary" : ""}`} onClick={() => setEditing(!editing)}>
            {editing ? "Done" : "Edit"}
          </button>
        )}
        {onDelete && <button type="button" className="btn small danger" onClick={onDelete}>Delete</button>}
      </div>
    </header>
  );
}

/** Confirm, delete, and clean up the DM store. */
export function useDeleteRecord(kind: ListKind, id: string, name: string, discard: () => void) {
  const dm = useDM();
  const { confirm } = useDialogs();
  const toast = useToast();
  return async () => {
    if (!(await confirm(`Delete “${name}”? This can't be undone.`))) return;
    discard();
    try {
      await api.remove(kind, id);
      dm.remove(kind, id);
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };
}
