// Ctrl/Cmd+K: search campaign records and the rules, or run a quick action.

import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { api } from "../api";
import { useDebounced } from "../lib/hooks";
import type { KindMap, SrdEntry } from "../types";
import { KIND_LABEL, titleOf, useDM, type DocKind, type ListKind } from "./store";

interface Item {
  key: string;
  label: string;
  badge: string;
  hint?: string;
  run: () => void;
}

const SEARCHABLE: DocKind[] = ["scenes", "npcs", "notes", "encounters", "sessions", "handouts", "monsters", "trackers"];

export function CommandPalette() {
  const dm = useDM();
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const [index, setIndex] = useState(0);
  const [srd, setSrd] = useState<SrdEntry[]>([]);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen((o) => !o);
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);
  useEffect(() => {
    if (open) {
      setQ("");
      setIndex(0);
      requestAnimationFrame(() => input.current?.focus());
    }
  }, [open]);

  const searchSrd = useDebounced((text: string) => {
    if (text.trim().length < 2) return setSrd([]);
    api.srdSearch({ q: text.trim(), limit: 8, edition: dm.edition }).then((r) => setSrd(r.results), () => setSrd([]));
  }, 150);
  useEffect(() => searchSrd(q), [q, searchSrd]);

  const items = useMemo<Item[]>(() => {
    const needle = q.trim().toLowerCase();
    const close = () => setOpen(false);
    const out: Item[] = [];
    for (const kind of SEARCHABLE) {
      for (const d of dm.lists[kind as ListKind] as KindMap[ListKind][]) {
        const label = titleOf(kind as ListKind, d);
        if (needle && !label.toLowerCase().includes(needle)) continue;
        out.push({ key: `${kind}:${d.id}`, label, badge: KIND_LABEL[kind], run: () => { dm.open({ kind, id: d.id }); close(); } });
      }
    }
    out.sort((a, b) => Number(!a.label.toLowerCase().startsWith(needle)) - Number(!b.label.toLowerCase().startsWith(needle)));
    const local = out.slice(0, needle ? 12 : 6);
    const rules = srd.map((e): Item => ({
      key: `srd:${e.id}`, label: e.name, badge: "SRD", hint: e.summary,
      run: () => { dm.show({ type: "srd", id: e.id }); close(); },
    }));
    const actions: Item[] = [
      { key: "new-scene", label: "New scene", badge: "Action", run: () => { void dm.create("scenes", { title: q.trim() || "New Scene" }); close(); } },
      { key: "new-npc", label: "New NPC", badge: "Action", run: () => { void dm.create("npcs", { name: q.trim() || "New NPC" }); close(); } },
      { key: "new-encounter", label: "New encounter", badge: "Action", run: () => { void dm.create("encounters", { name: q.trim() || "New Encounter" }); close(); } },
      { key: "new-note", label: "New note", badge: "Action", run: () => { void dm.create("notes", { title: q.trim() || "New note" }); close(); } },
      { key: "overview", label: "Campaign overview", badge: "Go", run: () => { dm.open({ kind: "campaigns", id: dm.campaign.id }); close(); } },
      { key: "characters", label: "Characters", badge: "Go", run: () => { navigate("/characters"); close(); } },
      { key: "library", label: "Rules library", badge: "Go", run: () => { navigate("/library"); close(); } },
    ].filter((a) => !needle || a.label.toLowerCase().includes(needle) || a.badge === "Action");
    return [...local, ...rules, ...actions];
  }, [q, dm, srd, navigate]);

  if (!open) return null;
  const go = (i: number) => items[i]?.run();
  return (
    <div className="palette-backdrop" onMouseDown={() => setOpen(false)}>
      <div className="palette" role="dialog" aria-label="Search" onMouseDown={(e) => e.stopPropagation()}>
        <input ref={input} type="search" placeholder="Search scenes, NPCs, monsters, spells, rules…" value={q}
          onChange={(e) => { setQ(e.target.value); setIndex(0); }}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") { e.preventDefault(); setIndex((i) => Math.min(items.length - 1, i + 1)); }
            if (e.key === "ArrowUp") { e.preventDefault(); setIndex((i) => Math.max(0, i - 1)); }
            if (e.key === "Enter") { e.preventDefault(); go(index); }
            if (e.key === "Escape") setOpen(false);
          }} />
        <ul>
          {items.map((it, i) => (
            <li key={it.key}>
              <button type="button" className={i === index ? "on" : ""} onMouseEnter={() => setIndex(i)} onClick={() => go(i)}>
                <span className="kind-badge">{it.badge}</span><span className="grow">{it.label}</span>
                {it.hint && <span className="muted small">{it.hint}</span>}
              </button>
            </li>
          ))}
        </ul>
        <div className="palette-foot muted small">↑↓ to move · Enter to open · Esc to close</div>
      </div>
    </div>
  );
}
