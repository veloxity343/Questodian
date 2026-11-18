// The DM screen: navigator on the left, tabbed workspace in the middle, reference on the right.

import { useEffect, useRef, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { Splitter } from "../components/splitter";
import { EmptyState, HpBar, Loading } from "../components/ui";
import { useApp } from "../context/app";
import { useDialogs } from "../context/ui";
import { useDragReorder, usePref } from "../lib/hooks";
import { blankMap } from "../lib/map";
import type { Campaign, KindMap, Scene, SceneStatus } from "../types";
import { MonsterDoc, NoteDoc, NpcDoc, SceneDoc, TrackerDoc } from "./docs";
import { CampaignDoc, HandoutDoc, SessionDoc } from "./docs2";
import { EncounterDoc } from "./encounter";
import { CommandPalette } from "./palette";
import { RightPanel } from "./panel";
import { DMProvider, KIND_LABEL, titleOf, useDM, useParty, type DocKind, type DocRef, type ListKind } from "./store";

export function DmScreen() {
  const { campaigns, campaignId } = useApp();
  const campaign = campaigns.find((c) => c.id === campaignId);
  if (!campaigns.length) {
    return (
      <div className="page">
        <EmptyState title="Start your first campaign" text="The DM screen works on one campaign at a time.">
          <Link className="btn primary" to="/campaigns">Create a campaign</Link>
        </EmptyState>
      </div>
    );
  }
  if (!campaign) return <Loading />;
  return (
    <DMProvider key={campaign.id} campaign={campaign}>
      <Screen campaign={campaign} />
    </DMProvider>
  );
}

function Screen({ campaign }: { campaign: Campaign }) {
  const dm = useDM();
  const [left, setLeft] = usePref("dm.width.left", 300);
  const [right, setRight] = usePref("dm.width.right", 420);
  const [navOpen] = usePref("dm.nav", true);

  // [[links]] anywhere on the screen.
  useEffect(() => {
    const onClick = (e: MouseEvent) => {
      const el = (e.target as HTMLElement).closest?.<HTMLElement>(".ref-link");
      if (!el?.dataset.ref) return;
      e.preventDefault();
      void dm.followLink(el.dataset.ref, el.dataset.kind);
    };
    document.addEventListener("click", onClick);
    return () => document.removeEventListener("click", onClick);
  }, [dm]);

  if (!dm.loaded) return <Loading />;
  const cols = `${navOpen ? `${left}px 6px ` : ""}minmax(0, 1fr) 6px ${right}px`;
  return (
    <div className="dm" style={{ gridTemplateColumns: cols }}>
      {navOpen && <Navigator />}
      {navOpen && <Splitter className="resizer" axis="x" start={() => left} onChange={setLeft} min={220} max={480} onReset={() => setLeft(300)} label="Resize the navigator" />}
      <Workspace campaign={campaign} />
      <Splitter className="resizer" axis="x" start={() => right} onChange={setRight} min={300} max={720} invert onReset={() => setRight(420)} label="Resize the reference panel" />
      <RightPanel />
      <CommandPalette />
    </div>
  );
}

// ------------------------------------------------------------------ navigator (left)

type Section = "sessions" | "scenes" | "npcs" | "monsters" | "notes" | "handouts" | "trackers" | "party";
const SECTIONS: [Section, string][] = [
  ["sessions", "Sessions"], ["scenes", "Scenes"], ["npcs", "NPCs"], ["notes", "Notes"],
  ["handouts", "Handouts"], ["monsters", "Monsters"], ["trackers", "Trackers"], ["party", "Party"],
];
const NEW_LABEL: Partial<Record<Section, string>> = {
  sessions: "Session", scenes: "Scene", npcs: "NPC", notes: "Note", handouts: "Handout", monsters: "Monster", trackers: "Tracker",
};

function Navigator() {
  const dm = useDM();
  const { prompt } = useDialogs();
  const [top, setTop] = usePref<"campaign" | "encounters">("dm.nav.top", "campaign");
  const [section, setSection] = usePref<Section>("dm.nav.section", "scenes");
  const [q, setQ] = useState("");

  const create = async (s: Section) => {
    const kind = s as DocKind & ListKind;
    const label = NEW_LABEL[s];
    if (s === "sessions") {
      const n = dm.lists.sessions.reduce((m, x) => Math.max(m, x.number), 0) + 1;
      const title = await prompt(`New session ${n}`, { label: "Title", value: "" });
      if (title !== null) await dm.create("sessions", { number: n, title: title || `Session ${n}` });
      return;
    }
    const name = await prompt(`New ${label}`, { label: "Name", value: "" });
    if (name === null) return;
    const field = ["scenes", "notes", "handouts"].includes(kind) ? "title" : "name";
    await dm.create(kind, { [field]: name || `New ${label}` } as Partial<KindMap[typeof kind]>);
  };

  // A map is a handout drawn in the app (dm/map.tsx), in the Maps folder.
  const createMap = async () => {
    const name = await prompt("New map", { label: "Name", value: "" });
    if (name === null) return;
    await dm.create("handouts", { title: name || "New map", folder: "Maps", map: blankMap() as unknown as Record<string, unknown> });
  };

  return (
    <nav className="navigator" aria-label="Campaign">
      <div className="nav-top">
        <button type="button" className={top === "encounters" ? "on" : ""} onClick={() => setTop("encounters")}>Encounters</button>
        <button type="button" className={top === "campaign" ? "on" : ""} onClick={() => setTop("campaign")}>Campaign</button>
      </div>
      {top === "encounters" ? <EncounterList /> : (
        <>
          <div className="sub-tabs wrap">
            <button type="button" onClick={() => dm.open({ kind: "campaigns", id: dm.campaign.id })}>Overview ↗</button>
            {SECTIONS.map(([s, label]) => <button type="button" key={s} className={section === s ? "on" : ""} onClick={() => setSection(s)}>{label}</button>)}
          </div>
          <div className="nav-head">
            <span className="eyebrow">{SECTIONS.find(([s]) => s === section)?.[1]} <span className="count">{section === "party" ? "" : (dm.lists[section as ListKind] as unknown[]).length}</span></span>
            {NEW_LABEL[section] && <button type="button" className="link small" onClick={() => create(section)}>+ {NEW_LABEL[section]}</button>}
            {section === "handouts" && <button type="button" className="link small" onClick={createMap}>+ Map</button>}
          </div>
          {section !== "party" && section !== "sessions" && (
            <input type="search" className="nav-filter" placeholder="Filter…" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Filter" />
          )}
          <div className="nav-list">
            <SectionList section={section} q={q.trim().toLowerCase()} />
          </div>
        </>
      )}
    </nav>
  );
}

const STATUS_ORDER: SceneStatus[] = ["active", "planned", "completed", "skipped"];

function Card({ kind, id, title, sub, extra, className = "" }: { kind: DocKind; id: string; title: string; sub?: string; extra?: string; className?: string }) {
  const dm = useDM();
  const on = dm.active?.kind === kind && dm.active.id === id;
  return (
    <button type="button" className={`nav-card ${on ? "on" : ""} ${className}`} onClick={() => dm.open({ kind, id })}>
      <span className="nav-card-title">{title}</span>
      {(sub || extra) && <span className="nav-card-sub">{sub && <span className="caps">{sub}</span>}{extra && <span className="muted"> {extra}</span>}</span>}
    </button>
  );
}

function SectionList({ section, q }: { section: Section; q: string }) {
  const dm = useDM();
  const party = useParty();
  const match = (s: string) => !q || s.toLowerCase().includes(q);
  const sessionLabel = (id: string) => {
    const s = dm.lists.sessions.find((x) => x.id === id);
    return s ? `S${s.number}` : "";
  };
  switch (section) {
    case "sessions":
      return (
        <>{[...dm.lists.sessions].sort((a, b) => b.number - a.number).map((s) => (
          <Card key={s.id} kind="sessions" id={s.id} title={`${s.number}. ${s.title}`} sub={s.status} extra={s.date} className={`status-${s.status}`} />
        ))}</>
      );
    case "scenes": {
      const scenes = dm.lists.scenes.filter((s) => match(`${s.title} ${s.location} ${s.summary}`));
      return (
        <>{STATUS_ORDER.map((st) => {
          const group = scenes.filter((s) => s.status === st);
          if (!group.length) return null;
          return (
            <div key={st} className="nav-group">
              <div className="eyebrow">{st} · {group.length}</div>
              {group.map((s: Scene) => (
                <Card key={s.id} kind="scenes" id={s.id} title={s.title} className={`status-${s.status}`}
                  sub={[sessionLabel(s.session_id), s.location].filter(Boolean).join(" · ")} extra={s.summary.slice(0, 90)} />
              ))}
            </div>
          );
        })}</>
      );
    }
    case "npcs":
      return <>{dm.lists.npcs.filter((n) => match(`${n.name} ${n.role}`)).map((n) => <Card key={n.id} kind="npcs" id={n.id} title={n.name} extra={n.role} />)}</>;
    case "monsters":
      return <>{dm.lists.monsters.filter((m) => match(m.name)).map((m) => <Card key={m.id} kind="monsters" id={m.id} title={m.name} sub={`CR ${m.cr}`} extra={m.kind} />)}</>;
    case "notes":
      return <>{dm.lists.notes.filter((n) => match(`${n.title} ${n.body}`)).map((n) => <Card key={n.id} kind="notes" id={n.id} title={`${n.pinned ? "📌 " : ""}${n.title}`} sub={n.kind === "pin" ? "pin board" : n.kind} />)}</>;
    case "trackers":
      return <>{dm.lists.trackers.filter((t) => match(t.name)).map((t) => <Card key={t.id} kind="trackers" id={t.id} title={t.name} sub={`${t.value} of ${t.max}`} extra={sessionLabel(t.session_id)} />)}</>;
    case "handouts": {
      const items = dm.lists.handouts.filter((h) => match(`${h.title} ${h.folder}`));
      const folders = [...new Set(items.map((h) => h.folder))].sort();
      return (
        <>{folders.map((f) => (
          <div key={f} className="nav-group">
            <div className="eyebrow">{f || "Unfiled"}</div>
            {items.filter((h) => h.folder === f).map((h) => <Card key={h.id} kind="handouts" id={h.id} title={h.title} extra={h.filename ? "" : "no image"} />)}
          </div>
        ))}</>
      );
    }
    case "party":
      return party.length ? (
        <>{party.map((c) => (
          <button type="button" key={c.id} className="nav-card" onClick={() => dm.show({ type: "character", id: c.id })}>
            <span className="nav-card-title">{c.name}</span>
            <span className="nav-card-sub muted">{c.classes.map((k) => `${k.name} ${k.level}`).join(" / ")} · HP {c.hp.current}/{c.hp.max} · AC {c.ac}</span>
            <HpBar current={c.hp.current} max={c.hp.max} />
          </button>
        ))}</>
      ) : <p className="muted small pad">Characters in this campaign appear here. Make them on the <Link to="/characters">Characters</Link> page.</p>;
  }
}

function EncounterList() {
  const dm = useDM();
  const [q, setQ] = useState("");
  const list = dm.lists.encounters.filter((e) => e.name.toLowerCase().includes(q.trim().toLowerCase()));
  return (
    <>
      <div className="nav-head">
        <span className="eyebrow">Encounters <span className="count">{dm.lists.encounters.length}</span></span>
        <button type="button" className="btn small" onClick={() => dm.create("encounters", { name: `Encounter ${dm.lists.encounters.length + 1}`, party_size: Math.max(1, dm.lists.characters.length) || 4 })}>+ New encounter</button>
      </div>
      <input type="search" className="nav-filter" placeholder="Search encounters…" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Search encounters" />
      <div className="nav-list">
        {list.map((e) => (
          <Card key={e.id} kind="encounters" id={e.id} title={e.name} className={e.active ? "status-active" : ""}
            sub={e.active ? `Round ${e.round}` : "Not started"} extra={`${e.combatants.length} combatants`} />
        ))}
      </div>
    </>
  );
}

// ------------------------------------------------------------------ workspace (middle)

/** Tabs share the strip's width like a browser's, shrinking to a minimum (and dropping the kind
 *  badge when narrow). Past that the strip pages with ‹ › instead of scrolling, so the mouse
 *  wheel stays free for the page, and the open tab is always brought into view. */
/** `active` is compared by identity: opening a tab, even the open one, makes a new ref and brings it into view. */
function useTabStrip(count: number, active: DocRef | null) {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  const [ends, setEnds] = useState({ start: true, end: true });
  const update = () => {
    const el = ref.current;
    if (!el) return;
    setWidth(el.clientWidth);
    setEnds({ start: el.scrollLeft <= 1, end: el.scrollLeft + el.clientWidth >= el.scrollWidth - 1 });
  };
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const observer = new ResizeObserver(update);
    observer.observe(el);
    el.addEventListener("scroll", update);
    return () => { observer.disconnect(); el.removeEventListener("scroll", update); };
  }, []);
  useEffect(() => {
    const el = ref.current?.querySelector<HTMLElement>(".doc-tab.on");
    const strip = ref.current;
    if (el && strip) {
      const left = el.offsetLeft - strip.offsetLeft;
      if (left < strip.scrollLeft) strip.scrollLeft = left;
      else if (left + el.offsetWidth > strip.scrollLeft + strip.clientWidth) strip.scrollLeft = left + el.offsetWidth - strip.clientWidth;
    }
    update();
  }, [active, count]);
  const page = (dir: -1 | 1) => ref.current?.scrollBy({ left: dir * ref.current.clientWidth * 0.8, behavior: "smooth" });
  return { ref, narrow: count > 0 && width / count < 160, overflow: !(ends.start && ends.end), ends, page };
}

function Workspace({ campaign }: { campaign: Campaign }) {
  const dm = useDM();
  const navigate = useNavigate();
  const label = (r: DocRef) => r.kind === "campaigns" ? campaign.name : titleOf(r.kind as ListKind, dm.get(r.kind as ListKind, r.id));
  const strip = useTabStrip(dm.tabs.length, dm.active);
  const key = (t: DocRef) => `${t.kind}:${t.id}`;
  const drag = useDragReorder((id, target) => {
    const find = (k: string) => dm.tabs.find((t) => key(t) === k);
    const [a, b] = [find(id), find(target)];
    if (a && b) dm.moveTab(a, b);
  }, strip.ref);
  return (
    <section className="workspace">
      <div className="doc-tab-bar">
      {strip.overflow && (
        <button type="button" className="doc-tab-page" aria-label="Earlier tabs" title="Earlier tabs" disabled={strip.ends.start} onClick={() => strip.page(-1)}>‹</button>
      )}
      <div ref={strip.ref} className={`doc-tabs ${strip.narrow ? "narrow" : ""}`} role="tablist">
        {dm.tabs.map((t) => {
          const on = dm.active?.kind === t.kind && dm.active.id === t.id;
          return (
            <div key={key(t)} data-reorder={key(t)} className={`doc-tab ${on ? "on" : ""} ${drag.dragging === key(t) ? "dragging" : ""}`}
              role="tab" aria-selected={on} title={label(t)}
              onPointerDown={(e) => !(e.target as HTMLElement).closest(".doc-tab-x") && drag.start(key(t), e)}
              onMouseDown={(e) => e.button === 1 && dm.close(t)}>
              <button type="button" className="doc-tab-main" onClick={() => dm.open(t)}>
                <span className="kind-badge">{KIND_LABEL[t.kind]}</span><span className="doc-tab-title">{label(t)}</span>
              </button>
              <button type="button" className="doc-tab-x" aria-label={`Close ${label(t)}`} onClick={() => dm.close(t)}>×</button>
            </div>
          );
        })}
      </div>
      {strip.overflow && (
        <button type="button" className="doc-tab-page" aria-label="Later tabs" title="Later tabs" disabled={strip.ends.end} onClick={() => strip.page(1)}>›</button>
      )}
      </div>
      <div className="doc-scroll">
        {dm.active ? <DocView key={`${dm.active.kind}:${dm.active.id}`} doc={dm.active} /> : (
          <div className="welcome">
            <h1>{campaign.name}</h1>
            <p className="muted">Pick something from the left, press <kbd>Ctrl</kbd> <kbd>K</kbd> to search everything, or start with:</p>
            <div className="row gap wrap">
              <button type="button" className="btn" onClick={() => dm.open({ kind: "campaigns", id: campaign.id })}>Campaign overview</button>
              <button type="button" className="btn" onClick={() => dm.create("scenes", { title: "New Scene" })}>+ Scene</button>
              <button type="button" className="btn" onClick={() => dm.create("encounters", { name: "New Encounter" })}>+ Encounter</button>
              <button type="button" className="btn ghost" onClick={() => navigate("/library")}>Rules library</button>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}

function DocView({ doc }: { doc: DocRef }) {
  const dm = useDM();
  if (doc.kind === "campaigns") return <CampaignDoc />;
  const record = dm.get(doc.kind as ListKind, doc.id);
  if (!record) return <EmptyState title="Not found" text="This record was deleted." />;
  switch (doc.kind) {
    case "scenes": return <SceneDoc initial={record as KindMap["scenes"]} />;
    case "npcs": return <NpcDoc initial={record as KindMap["npcs"]} />;
    case "notes": return <NoteDoc initial={record as KindMap["notes"]} />;
    case "monsters": return <MonsterDoc initial={record as KindMap["monsters"]} />;
    case "trackers": return <TrackerDoc initial={record as KindMap["trackers"]} />;
    case "sessions": return <SessionDoc initial={record as KindMap["sessions"]} />;
    case "handouts": return <HandoutDoc initial={record as KindMap["handouts"]} />;
    case "encounters": return <EncounterDoc initial={record as KindMap["encounters"]} />;
  }
}
