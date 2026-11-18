// Workspace documents: sessions, the campaign overview, and handouts.

import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { EmptyState, NumInput, Select, TextInput, type Option } from "../components/ui";
import { useApp } from "../context/app";
import { useDice } from "../context/dice";
import { useDialogs, useToast } from "../context/ui";
import { useDebounced, useDraft, usePanZoom } from "../lib/hooks";
import { openPlayersWindow, togglePlayersFullscreen } from "../lib/desktop";
import { blankMap, isMap, mapOf, type MapData } from "../lib/map";
import { fmtDate, titleCase } from "../lib/util";
import type { Campaign, Edition, Handout, Session, SessionStatus } from "../types";
import { TrackerWidget } from "./docs";
import { MapEditor, MapTokensFromPlayers, PlayersMap } from "./map";
import { DocHeader, Pills, Section, useDeleteRecord, useRecordDraft } from "./parts";
import { useDM, useParty } from "./store";

const SESSION_STATUSES: SessionStatus[] = ["planned", "active", "done"];
const LOG_KINDS = ["note", "combat", "roll", "loot", "npc", "quest", "lore"];
// Fields the page owns; the log, loot hand-outs and XP change through server actions.
const SESSION_FIELDS = ["title", "date", "status", "number", "prep", "recap", "loot", "attendance"] as const;

// ------------------------------------------------------------------ session

export function SessionDoc({ initial }: { initial: Session }) {
  const dm = useDM();
  const toast = useToast();
  const { setSessionId } = useApp();
  const { doc, edit, setDoc, flush, discard, status } = useDraft<Session>(
    initial,
    async (d) => dm.upsert("sessions", await api.update("sessions", d.id, Object.fromEntries(SESSION_FIELDS.map((k) => [k, d[k]])))),
    { onError: (e) => toast(e.message, "error") },
  );
  const [editing, setEditing] = useState(false);
  const onDelete = useDeleteRecord("sessions", initial.id, `Session ${initial.number}`, discard);
  useEffect(() => setSessionId(initial.id), [initial.id, setSessionId]);
  if (!doc) return null;
  const set = <K extends keyof Session>(k: K) => (v: Session[K]) => edit((d) => void (d[k] = v));
  const run = async (fn: () => Promise<Session>) => {
    try {
      await flush();
      const fresh = await fn();
      setDoc(fresh);
      dm.upsert("sessions", fresh);
      return true;
    } catch (e) {
      toast((e as Error).message, "error");
      return false;
    }
  };

  const scenes = dm.lists.scenes.filter((s) => s.session_id === doc.id).sort((a, b) => a.position - b.position);
  const trackers = dm.lists.trackers.filter((t) => t.session_id === doc.id);
  const encounters = dm.lists.encounters.filter((e) => e.session_id === doc.id);
  const notes = dm.lists.notes.filter((n) => n.session_id === doc.id);
  const move = async (i: number, by: number) => {
    const order = [...scenes];
    const [s] = order.splice(i, 1);
    order.splice(i + by, 0, s);
    await Promise.all(order.map((sc, pos) => (sc.position === pos ? null : api.update("scenes", sc.id, { position: pos }).then((x) => dm.upsert("scenes", x)))));
  };

  return (
    <article className="doc">
      <DocHeader
        kind="sessions" status={status} editing={editing} setEditing={setEditing} onDelete={onDelete} linkName={doc.title}
        title={editing ? (
          <span className="row gap"><NumInput value={doc.number} onChange={set("number")} aria-label="Session number" />
            <TextInput className="title-input" value={doc.title} onChange={set("title")} aria-label="Title" /></span>
        ) : <>Session {doc.number}: {doc.title}</>}
        subtitle={
          <div className="row gap wrap">
            <input type="date" value={doc.date} onChange={(e) => set("date")(e.target.value)} aria-label="Date" />
            <Pills value={doc.status} options={SESSION_STATUSES} onChange={set("status")} labels={{ active: "In progress" }} />
          </div>
        }
      />

      <section className="doc-section">
        <div className="eyebrow row gap">Scenes <button type="button" className="link small"
          onClick={() => dm.create("scenes", { session_id: doc.id, position: scenes.length, title: `Scene ${scenes.length + 1}` })}>+ Scene</button></div>
        {scenes.length ? (
          <ol className="scene-list">
            {scenes.map((s, i) => (
              <li key={s.id} className={`status-${s.status}`}>
                <button type="button" className="list-link" onClick={() => dm.open({ kind: "scenes", id: s.id })}>
                  <span className="grow"><strong>{s.title}</strong>{s.location && <span className="muted small"> · {s.location}</span>}</span>
                  <span className={`tag status-${s.status}`}>{s.status}</span>
                </button>
                <span className="reorder">
                  <button type="button" className="icon-btn" disabled={i === 0} onClick={() => move(i, -1)} aria-label="Move up">▲</button>
                  <button type="button" className="icon-btn" disabled={i === scenes.length - 1} onClick={() => move(i, 1)} aria-label="Move down">▼</button>
                </span>
              </li>
            ))}
          </ol>
        ) : <p className="muted small">No scenes yet. Break the session into scenes to run it beat by beat.</p>}
      </section>

      <section className="doc-section">
        <div className="eyebrow row gap">Trackers <button type="button" className="link small"
          onClick={() => dm.create("trackers", { session_id: doc.id, name: "Clock", max: 6 })}>+ Tracker</button></div>
        <div className="tracker-grid">
          {trackers.map((t) => (
            <div key={t.id} className="tracker-card">
              <TrackerWidget tracker={t} onChange={async (next) => dm.upsert("trackers", await api.update("trackers", t.id, { value: next.value }))} />
              <button type="button" className="link small" onClick={() => dm.open({ kind: "trackers", id: t.id })}>Edit steps</button>
            </div>
          ))}
          {!trackers.length && <p className="muted small">Clocks and counters for this session, e.g. hours until dawn or rounds until the ritual finishes.</p>}
        </div>
      </section>

      <section className="doc-section">
        <div className="eyebrow row gap">Encounters <button type="button" className="link small"
          onClick={() => dm.create("encounters", { session_id: doc.id, name: `Session ${doc.number} encounter` })}>+ Encounter</button></div>
        <div className="row gap wrap">
          {encounters.map((e) => (
            <button type="button" key={e.id} className="btn small" onClick={() => dm.open({ kind: "encounters", id: e.id })}>
              ⚔ {e.name}{e.active ? ` · round ${e.round}` : ""}
            </button>
          ))}
          {!encounters.length && <span className="muted small">None yet.</span>}
        </div>
      </section>

      {notes.length > 0 && (
        <section className="doc-section">
          <div className="eyebrow">Notes</div>
          <div className="row gap wrap">
            {notes.map((n) => <button type="button" key={n.id} className="btn small ghost" onClick={() => dm.open({ kind: "notes", id: n.id })}>{n.title}</button>)}
          </div>
        </section>
      )}

      <Section label="Prep" value={doc.prep} editing={editing} onChange={set("prep")} rows={10}
        placeholder="Strong start, secrets & clues, NPCs, monsters, rewards…" />
      <Section label="Recap" value={doc.recap} editing={editing} onChange={set("recap")} rows={6}
        placeholder="What actually happened, for next time." />
      <SessionLog s={doc} run={run} />
      <LootAndXp s={doc} edit={edit} run={run} />
    </article>
  );
}

function SessionLog({ s, run }: { s: Session; run: (fn: () => Promise<Session>) => Promise<boolean> }) {
  const [kind, setKind] = useState("note");
  const [text, setText] = useState("");
  const add = async () => {
    if (text.trim() && (await run(() => api.log(s.id, text.trim(), kind)))) setText("");
  };
  return (
    <section className="doc-section">
      <div className="eyebrow">Session log</div>
      <div className="row gap log-input">
        <Select value={kind} options={LOG_KINDS.map((k): Option => [k, titleCase(k)])} onChange={setKind} aria-label="Entry type" />
        <input type="text" className="grow" placeholder="What happened? (Enter to add)" aria-label="Log entry"
          value={text} onChange={(e) => setText(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} />
      </div>
      <ul className="log">
        {[...s.log].reverse().slice(0, 50).map((e) => (
          <li key={e.id} className={`log-entry kind-${e.kind}`}>
            <time className="muted small">{fmtDate(e.time)}</time><span className="tag">{e.kind}</span><span className="grow">{e.text}</span>
            <button type="button" className="icon-btn" aria-label="Delete entry"
              onClick={() => run(() => api.update("sessions", s.id, { log: s.log.filter((x) => x.id !== e.id) }))}>✕</button>
          </li>
        ))}
      </ul>
    </section>
  );
}

function LootAndXp({ s, edit, run }: { s: Session; edit: (fn: (d: Session) => void) => void; run: (fn: () => Promise<Session>) => Promise<boolean> }) {
  const party = useParty();
  const { roll } = useDice();
  const toast = useToast();
  const [xp, setXp] = useState("");
  const partyOpts: Option[] = [["", "— give to —"], ...party.map((c): Option => [c.id, c.name])];
  return (
    <section className="doc-section">
      <div className="eyebrow row gap">Loot & XP
        <button type="button" className="link small" onClick={() => edit((d) => void d.loot.push({ name: "", qty: 1, value: "", assigned_to: "" }))}>+ Item</button>
        <button type="button" className="link small" onClick={() => roll("d100", "Treasure (d100)")}>🎲 d100</button>
      </div>
      {s.loot.length > 0 && (
        <table className="table compact">
          <tbody>
            {s.loot.map((item, i) => (
              <tr key={i}>
                <td><TextInput value={item.name} onChange={(v) => edit((d) => void (d.loot[i].name = v))} aria-label="Item" /></td>
                <td><NumInput value={item.qty} onChange={(v) => edit((d) => void (d.loot[i].qty = v))} className="num-input tiny" aria-label="Quantity" /></td>
                <td><TextInput value={item.value} onChange={(v) => edit((d) => void (d.loot[i].value = v))} placeholder="50 gp" aria-label="Value" /></td>
                <td>{item.assigned_to ? <span className="tag">{item.assigned_to}</span> : (
                  <Select value="" options={partyOpts} aria-label="Give to" onChange={async (cid) => {
                    if (cid && (await run(() => api.award(s.id, { loot_index: i, character_id: cid })))) toast("Added to inventory");
                  }} />
                )}</td>
                <td><button type="button" className="icon-btn" aria-label="Remove" onClick={() => edit((d) => void d.loot.splice(i, 1))}>✕</button></td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <div className="row gap wrap">
        <span className="muted small">XP this session: <strong>{s.xp_awarded}</strong> each</span>
        <input type="number" min={0} placeholder="XP each" className="num-input wide-num" value={xp} onChange={(e) => setXp(e.target.value)} aria-label="XP each" />
        <button type="button" className="btn small" disabled={!party.length} onClick={async () => {
          const n = parseInt(xp, 10);
          if (n && (await run(() => api.award(s.id, { xp: n, character_ids: party.map((c) => c.id) })))) {
            toast(`Awarded ${n} XP to ${party.length}`);
            setXp("");
          }
        }}>Award to party</button>
      </div>
    </section>
  );
}

// ------------------------------------------------------------------ campaign overview

export function CampaignDoc() {
  const dm = useDM();
  const toast = useToast();
  const { confirm } = useDialogs();
  const { refreshCampaigns, meta } = useApp();
  const { doc, edit, status } = useDraft<Campaign>(dm.campaign, async (d) => {
    dm.upsert("campaigns", await api.update("campaigns", d.id, d));
    await refreshCampaigns();
  }, { onError: (e) => toast(e.message, "error") });
  const [editing, setEditing] = useState(false);
  if (!doc) return null;
  const set = <K extends keyof Campaign>(k: K) => (v: Campaign[K]) => edit((d) => void (d[k] = v));
  const sessions = [...dm.lists.sessions].sort((a, b) => a.number - b.number);
  const changeEdition = async (v: string) => {
    if (v === doc.edition) return;
    if (await confirm("Switch rules edition? Reference lookups and new monsters will use the other SRD. Existing records stay as they are.", { label: "Switch", danger: false })) {
      set("edition")(v as Edition);
    }
  };
  return (
    <article className="doc">
      <DocHeader kind="campaigns" status={status} editing={editing} setEditing={setEditing}
        title={editing ? <TextInput className="title-input" value={doc.name} onChange={set("name")} aria-label="Name" /> : doc.name}
        subtitle={
          <div className="row gap wrap">
            <Select aria-label="Rules edition" value={doc.edition} onChange={changeEdition}
              options={Object.entries(meta?.editions ?? { "2014": "5e (2014)", "2024": "5.5e (2024)" }) as Option[]} />
            {editing ? <TextInput value={doc.setting} onChange={set("setting")} placeholder="Setting" aria-label="Setting" />
              : doc.setting && <span className="muted">{doc.setting}</span>}
          </div>
        } />
      <Section label="Pitch" value={doc.description} editing={editing} onChange={set("description")} rows={3} />
      <Section label="The truth (DM only)" value={doc.notes} editing={editing} onChange={set("notes")} rows={16} variant="secret" />
      <section className="doc-section">
        <div className="eyebrow row gap">Sessions <button type="button" className="link small"
          onClick={() => dm.create("sessions", { number: (sessions.at(-1)?.number ?? 0) + 1, title: "New Session" })}>+ Session</button></div>
        {sessions.length ? (
          <ol className="scene-list">
            {sessions.map((s) => (
              <li key={s.id}><button type="button" className="list-link" onClick={() => dm.open({ kind: "sessions", id: s.id })}>
                <span className="num">{s.number}</span><span className="grow">{s.title}</span>
                <span className={`tag status-${s.status}`}>{s.status}</span><span className="muted small">{s.date}</span>
              </button></li>
            ))}
          </ol>
        ) : <EmptyState title="No sessions yet" text="Add the first one to start planning." />}
      </section>
      <section className="doc-section">
        <div className="eyebrow row gap">Players <button type="button" className="link small"
          onClick={() => edit((d) => void d.players.push({ name: "", contact: "", character_id: "", notes: "" }))}>+ Player</button></div>
        <table className="table compact">
          <tbody>
            {doc.players.map((p, i) => (
              <tr key={i}>
                <td><TextInput value={p.name} onChange={(v) => edit((d) => void (d.players[i].name = v))} placeholder="Player" aria-label="Player" /></td>
                <td><Select value={p.character_id} aria-label="Character" onChange={(v) => edit((d) => void (d.players[i].character_id = v))}
                  options={[["", "— character —"], ...dm.lists.characters.map((c): Option => [c.id, c.name])]} /></td>
                <td><TextInput value={p.contact} onChange={(v) => edit((d) => void (d.players[i].contact = v))} placeholder="Contact" aria-label="Contact" /></td>
                <td><button type="button" className="icon-btn" aria-label="Remove" onClick={() => edit((d) => void d.players.splice(i, 1))}>✕</button></td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
    </article>
  );
}

// ------------------------------------------------------------------ handout

const PROJECT_CHANNEL = "questodian-project";
interface ViewState { id: string; zoom: number; x: number; y: number }

export function HandoutDoc({ initial }: { initial: Handout }) {
  const dm = useDM();
  const toast = useToast();
  const { confirm } = useDialogs();
  const { doc, edit, setDoc, discard, status } = useRecordDraft("handouts", initial);
  const [editing, setEditing] = useState(false);
  const [view, setView] = useState<ViewState>({ id: initial.id, zoom: 1, x: 0, y: 0 });
  const [casting, setCasting] = useState(false);
  const party = useParty();
  const onDelete = useDeleteRecord("handouts", initial.id, initial.title, discard);
  const channel = useRef<BroadcastChannel | null>(null);
  useEffect(() => {
    channel.current = new BroadcastChannel(PROJECT_CHANNEL);
    return () => channel.current?.close();
  }, []);
  // Same browser: instant via BroadcastChannel. Anywhere else (second desktop window, a TV on
  // the network): through the server, which the players' window polls.
  const publish = useDebounced((v: ViewState) => void api.project(v).catch(() => {}), 120);
  useEffect(() => {
    if (!casting) return;
    channel.current?.postMessage(view);
    publish(view);
  }, [casting, view, publish]);
  if (!doc) return null;
  const set = <K extends keyof Handout>(k: K) => (v: Handout[K]) => edit((d) => void (d[k] = v));
  const upload = async (file: File | undefined) => {
    if (!file) return;
    try {
      const saved = await api.uploadHandout(doc.id, file);
      setDoc(saved);
      dm.upsert("handouts", saved);
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };
  // A handout is an image or a map; switching removes the other, after asking.
  const drawInstead = async () => {
    if (!(await confirm("This removes the image from this handout. Draw a map instead?", { label: "Remove the image" }))) return;
    try {
      await api.removeHandoutImage(doc.id);
      edit((d) => { d.filename = ""; d.mime = ""; d.map = blankMap() as unknown as Record<string, unknown>; });
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };
  const imageInstead = async () => {
    if (await confirm("This removes the drawn map from this handout. Use an image instead?", { label: "Remove the map" })) {
      edit((d) => void (d.map = {}));
    }
  };
  const folders = [...new Set(dm.lists.handouts.map((h) => h.folder).filter(Boolean))];
  const drawn = isMap(doc.map) ? mapOf(doc.map) : null;
  return (
    <article className="doc" onPaste={(e) => !drawn && upload(e.clipboardData.files[0])}>
      <DocHeader kind="handouts" status={status} editing={editing} setEditing={setEditing} onDelete={onDelete} linkName={doc.title}
        title={editing ? <TextInput className="title-input" value={doc.title} onChange={set("title")} aria-label="Title" /> : doc.title}
        subtitle={editing ? (
          <><TextInput list="handout-folders" value={doc.folder} onChange={set("folder")} placeholder="Folder (e.g. Maps / Session 1)" aria-label="Folder" />
            <datalist id="handout-folders">{folders.map((f) => <option key={f} value={f} />)}</datalist></>
        ) : <span className="muted">{doc.folder}</span>}
        extra={doc.filename || drawn ? (
          <>
            <button type="button" className={`btn small ${casting ? "primary" : ""}`}
              onClick={() => { setCasting(true); void api.project(view).then(() => openPlayersWindow()); }}>
              📽 {casting ? "Showing to players" : "Show to players"}
            </button>
            <button type="button" className="btn small ghost" onClick={() => setView((v) => ({ ...v, zoom: 1, x: 0, y: 0 }))}>Reset view</button>
            {drawn ? (
              <button type="button" className="btn small ghost" onClick={imageInstead}>Use an image instead…</button>
            ) : (
              <>
                <label className="btn small ghost upload-btn"><input type="file" accept="image/*" hidden onChange={(e) => upload(e.target.files?.[0])} />Replace image…</label>
                <button type="button" className="btn small ghost" onClick={drawInstead}>Draw a map instead…</button>
              </>
            )}
          </>
        ) : undefined}
      />
      {drawn ? (
        <MapTokensFromPlayers id={doc.id} map={drawn} onTokens={(tokens) => edit((d) => void ((d.map as unknown as MapData).tokens = tokens))}>
          <MapEditor name={doc.title} map={drawn} onChange={(next) => edit((d) => void (d.map = next as unknown as Record<string, unknown>))}
            view={view} onView={(v) => setView((s) => ({ ...s, ...v }))} characters={party} />
        </MapTokensFromPlayers>
      ) : doc.filename ? (
        <PanZoom src={api.handoutUrl(doc)} alt={doc.title} view={view} onView={setView} />
      ) : (
        <label className="dropzone" onDragOver={(e) => e.preventDefault()} onDrop={(e) => { e.preventDefault(); void upload(e.dataTransfer.files[0]); }}>
          <input type="file" accept="image/*" hidden onChange={(e) => upload(e.target.files?.[0])} />
          <strong>Add an image</strong><span className="muted small">Drop a file here, paste one, or click to choose (PNG, JPG, WebP, GIF, SVG).</span>
        </label>
      )}
      <Section label="Notes and key" value={doc.body} editing={editing} onChange={set("body")} rows={8}
        placeholder="Map key, what the players see, secrets…" />
      {!drawn && !doc.filename && (
        <p className="muted small">Or <button type="button" className="link" onClick={() => edit((d) => void (d.map = blankMap() as unknown as Record<string, unknown>))}>draw a map instead</button>.</p>
      )}
    </article>
  );
}

/** Image viewer: wheel to zoom around the cursor, drag to pan. */
function PanZoom({ src, alt, view, onView, interactive = true }: {
  src: string;
  alt: string;
  view: ViewState;
  onView?: (v: ViewState) => void;
  interactive?: boolean;
}) {
  const pan = usePanZoom(view, onView, [0.2, 8]);
  return (
    <div ref={interactive ? pan.wheelRef : undefined} className={`panzoom ${interactive ? "interactive" : ""}`}
      onPointerDown={interactive ? (e) => { e.currentTarget.setPointerCapture(e.pointerId); pan.start(e); } : undefined}
      onPointerMove={interactive ? pan.move : undefined} onPointerUp={pan.end} onPointerCancel={pan.end}>
      <img src={src} alt={alt} draggable={false} style={{ transform: `translate(${view.x}px, ${view.y}px) scale(${view.zoom})` }} />
    </div>
  );
}

/** The players' window: shows whichever handout the DM is casting, mirroring zoom and pan. */
export function ProjectionView({ id = "" }: { id?: string }) {
  const [view, setView] = useState<ViewState>({ id, zoom: 1, x: 0, y: 0 });
  const [handout, setHandout] = useState<Handout | null>(null);
  const seq = useRef(-1);
  useEffect(() => {
    const ch = new BroadcastChannel(PROJECT_CHANNEL);
    ch.onmessage = (e: MessageEvent<ViewState>) => setView(e.data);
    const poll = setInterval(() => {
      api.projection().then((p) => {
        if (p.seq !== seq.current && p.id) {
          seq.current = p.seq;
          setView({ id: p.id, zoom: p.zoom, x: p.x, y: p.y });
        }
      }, () => {});
    }, 500);
    return () => {
      ch.close();
      clearInterval(poll);
    };
  }, []);
  useEffect(() => {
    if (view.id) api.get("handouts", view.id).then(setHandout, () => setHandout(null));
  }, [view.id]);
  useEffect(() => {
    if (handout) document.title = handout.title;
  }, [handout]);
  // A map changes as the DM draws and the party moves: keep it fresh (one timer per map shown).
  const showingMap = !!handout && isMap(handout.map);
  useEffect(() => {
    if (!view.id || !showingMap) return;
    const timer = setInterval(() => api.get("handouts", view.id).then(setHandout, () => {}), 1500);
    return () => clearInterval(timer);
  }, [view.id, showingMap]);
  return (
    <div className="projection">
      <button type="button" className="fullscreen-btn" onClick={() => void togglePlayersFullscreen()} title="Full screen (for a projector or TV)" aria-label="Full screen">⛶</button>
      {handout && isMap(handout.map) ? <PlayersMap handoutId={handout.id} map={mapOf(handout.map)} view={view} />
        : handout?.filename ? <PanZoom src={api.handoutUrl(handout)} alt={handout.title} view={view} interactive={false} />
        : <p className="muted">Waiting for the DM…</p>}
    </div>
  );
}
