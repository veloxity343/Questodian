// State for the DM screen: the campaign's records, open workspace tabs, what the
// right-hand panel is showing, and resolving [[links]] to records or SRD entries.

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api } from "../api";
import { useToast } from "../context/ui";
import { usePref } from "../lib/hooks";
import type { Campaign, Character, Edition, Kind, KindMap } from "../types";

/** Record kinds that open as workspace tabs. */
export type DocKind = "campaigns" | "sessions" | "scenes" | "npcs" | "monsters" | "notes" | "encounters" | "handouts" | "trackers";
export type ListKind = Exclude<Kind, "campaigns" | "handbook">;
const LIST_KINDS: ListKind[] = [
  "sessions", "scenes", "npcs", "monsters", "notes", "encounters", "trackers", "handouts", "characters",
];

export interface DocRef {
  kind: DocKind;
  id: string;
}

/** What the right panel's View tab shows. */
export type ViewRef =
  | { type: "srd"; id: string }
  | { type: "monster"; id: string }
  | { type: "npc"; id: string }
  | { type: "character"; id: string };

export type RightTab = "reference" | "view" | "notes";

type Lists = { [K in ListKind]: KindMap[K][] };
const EMPTY: Lists = Object.fromEntries(LIST_KINDS.map((k) => [k, []])) as unknown as Lists;

export const KIND_LABEL: Record<DocKind, string> = {
  campaigns: "Campaign", sessions: "Session", scenes: "Scene", npcs: "NPC", monsters: "Monster", notes: "Note",
  encounters: "Encounter", handouts: "Handout", trackers: "Tracker",
};

/** Kind names accepted in [[kind:Name]] links. */
const LINK_KINDS: Record<string, ListKind | "srd"> = {
  npc: "npcs", scene: "scenes", note: "notes", handout: "handouts", map: "handouts", encounter: "encounters",
  session: "sessions", tracker: "trackers", character: "characters", pc: "characters", homebrew: "monsters",
  monster: "srd", spell: "srd", item: "srd", rule: "srd", condition: "srd", srd: "srd",
};
const SRD_CATEGORY: Record<string, string> = {
  monster: "monsters", spell: "spells", item: "magic-items", rule: "rules", condition: "conditions",
};

export function titleOf(kind: Kind, doc: KindMap[Kind] | undefined): string {
  if (!doc) return "";
  if (kind === "sessions") {
    const s = doc as KindMap["sessions"];
    return `${s.number}. ${s.title}`;
  }
  const d = doc as { title?: string; name?: string };
  return d.title || d.name || "Untitled";
}

interface DM {
  campaign: Campaign;
  edition: Edition;
  lists: Lists;
  loaded: boolean;
  reload: (kind?: ListKind) => Promise<void>;
  upsert: <K extends ListKind | "campaigns">(kind: K, doc: KindMap[K]) => void;
  remove: (kind: ListKind, id: string) => void;
  get: <K extends ListKind>(kind: K, id: string) => KindMap[K] | undefined;

  tabs: DocRef[];
  active: DocRef | null;
  open: (ref: DocRef) => void;
  close: (ref: DocRef) => void;
  /** Move tab `ref` to where `target` is (after it when moving right, before it when moving left). */
  moveTab: (ref: DocRef, target: DocRef) => void;
  create: <K extends DocKind & ListKind>(kind: K, data?: Partial<KindMap[K]>) => Promise<KindMap[K]>;

  view: ViewRef | null;
  show: (ref: ViewRef) => void;
  rightTab: RightTab;
  setRightTab: (t: RightTab) => void;

  /** The encounter "Add to encounter" buttons target: the open one, else the running one. */
  encounterId: string;
  followLink: (name: string, kind?: string) => Promise<void>;
}

const DMContext = createContext<DM | null>(null);

export function useDM() {
  const ctx = useContext(DMContext);
  if (!ctx) throw new Error("useDM outside DMProvider");
  return ctx;
}

const same = (a: DocRef | null, b: DocRef | null) => !!a && !!b && a.kind === b.kind && a.id === b.id;

export function DMProvider({ campaign: initial, children }: { campaign: Campaign; children: ReactNode }) {
  const toast = useToast();
  const cid = initial.id;
  const [campaign, setCampaign] = useState(initial);
  const [lists, setLists] = useState<Lists>(EMPTY);
  const [loaded, setLoaded] = useState(false);
  const [tabs, setTabs] = usePref<DocRef[]>(`dm.tabs.${cid}`, []);
  const [active, setActive] = usePref<DocRef | null>(`dm.active.${cid}`, null);
  const [view, setView] = useState<ViewRef | null>(null);
  const [rightTab, setRightTab] = usePref<RightTab>("dm.right", "reference");
  const listsRef = useRef(lists);
  listsRef.current = lists;

  useEffect(() => setCampaign(initial), [initial]);

  const reload = useCallback(
    async (kind?: ListKind) => {
      const kinds = kind ? [kind] : LIST_KINDS;
      try {
        const results = await Promise.all(kinds.map((k) => api.list(k, { campaign_id: cid })));
        // Shared homebrew (no campaign) shows up alongside the campaign's own monsters.
        if (kinds.includes("monsters")) {
          const all = await api.list("monsters");
          const i = kinds.indexOf("monsters");
          results[i] = all.filter((m) => !m.campaign_id || m.campaign_id === cid);
        }
        setLists((prev) => ({ ...prev, ...Object.fromEntries(kinds.map((k, i) => [k, results[i]])) }));
      } catch (e) {
        toast((e as Error).message, "error");
      } finally {
        setLoaded(true);
      }
    },
    [cid, toast],
  );
  useEffect(() => {
    setLoaded(false);
    setLists(EMPTY);
    void reload();
  }, [reload]);

  // Drop tabs whose records no longer exist (deleted elsewhere, or a stale saved layout).
  useEffect(() => {
    if (!loaded) return;
    const exists = (r: DocRef) => r.kind === "campaigns" || (lists[r.kind as ListKind] as { id: string }[]).some((d) => d.id === r.id);
    if (tabs.some((t) => !exists(t))) setTabs(tabs.filter(exists));
    if (active && !exists(active)) setActive(null);
  }, [loaded, lists, tabs, active, setTabs, setActive]);

  const upsert = useCallback(<K extends ListKind | "campaigns">(kind: K, doc: KindMap[K]) => {
    if (kind === "campaigns") {
      setCampaign(doc as Campaign);
      return;
    }
    setLists((prev) => {
      const list = prev[kind as ListKind] as unknown as { id: string }[];
      const d = doc as { id: string };
      const next = list.some((x) => x.id === d.id) ? list.map((x) => (x.id === d.id ? d : x)) : [...list, d];
      return { ...prev, [kind]: next };
    });
  }, []);

  const remove = useCallback(
    (kind: ListKind, id: string) => {
      setLists((prev) => ({ ...prev, [kind]: (prev[kind] as { id: string }[]).filter((x) => x.id !== id) }));
      setTabs((t) => t.filter((r) => !(r.kind === kind && r.id === id)));
    },
    [setTabs],
  );

  const get = useCallback(
    <K extends ListKind>(kind: K, id: string) => (listsRef.current[kind] as KindMap[K][]).find((d) => d.id === id),
    [],
  );

  const open = useCallback(
    (ref: DocRef) => {
      setTabs((t) => (t.some((x) => same(x, ref)) ? t : [...t, ref]));
      setActive(ref);
    },
    [setTabs, setActive],
  );

  const close = useCallback(
    (ref: DocRef) => {
      setTabs((t) => {
        const i = t.findIndex((x) => same(x, ref));
        const next = t.filter((x) => !same(x, ref));
        setActive((a) => (same(a, ref) ? next[Math.min(i, next.length - 1)] ?? null : a));
        return next;
      });
    },
    [setTabs, setActive],
  );

  const moveTab = useCallback(
    (ref: DocRef, target: DocRef) => {
      setTabs((t) => {
        const from = t.findIndex((x) => same(x, ref));
        const to = t.findIndex((x) => same(x, target));
        if (from < 0 || to < 0 || from === to) return t;
        const next = [...t];
        next.splice(to, 0, ...next.splice(from, 1));
        return next;
      });
    },
    [setTabs],
  );

  const create = useCallback(
    async <K extends DocKind & ListKind>(kind: K, data: Partial<KindMap[K]> = {}) => {
      const doc = await api.create(kind, { campaign_id: cid, ...data } as Partial<KindMap[K]>);
      upsert(kind, doc);
      open({ kind, id: doc.id });
      return doc;
    },
    [cid, upsert, open],
  );

  const show = useCallback(
    (ref: ViewRef) => {
      setView(ref);
      setRightTab("view");
    },
    [setRightTab],
  );

  const encounterId = useMemo(() => {
    if (active?.kind === "encounters") return active.id;
    const openEnc = [...tabs].reverse().find((t) => t.kind === "encounters");
    return openEnc?.id || lists.encounters.find((e) => e.active)?.id || "";
  }, [active, tabs, lists.encounters]);

  const followLink = useCallback(
    async (name: string, kind = "") => {
      const want = LINK_KINDS[kind.toLowerCase()];
      const needle = name.trim().toLowerCase();
      if (want !== "srd") {
        const kinds = want ? [want as ListKind] : (["npcs", "scenes", "notes", "handouts", "encounters", "sessions", "trackers", "monsters", "characters"] as ListKind[]);
        for (const k of kinds) {
          const hit = (listsRef.current[k] as KindMap[ListKind][]).find((d) => {
            const t = titleOf(k, d).toLowerCase();
            return t === needle || (k === "sessions" && (d as KindMap["sessions"]).title.toLowerCase() === needle);
          });
          if (!hit) continue;
          if (k === "characters") show({ type: "character", id: hit.id });
          else open({ kind: k as DocKind, id: hit.id });
          return;
        }
      }
      // Not a campaign record: look it up in the rules.
      const res = await api.srdSearch({ q: name, category: SRD_CATEGORY[kind.toLowerCase()], limit: 5, edition: campaign.edition });
      const exact = res.results.find((e) => e.name.toLowerCase() === needle) ?? (res.total === 1 ? res.results[0] : undefined);
      if (exact) show({ type: "srd", id: exact.id });
      else toast(`Nothing called “${name}” yet`, "error");
    },
    [campaign.edition, open, show, toast],
  );

  const value: DM = {
    campaign, edition: campaign.edition, lists, loaded, reload, upsert, remove, get,
    tabs, active, open, close, moveTab, create, view, show, rightTab, setRightTab, encounterId, followLink,
  };
  return <DMContext.Provider value={value}>{children}</DMContext.Provider>;
}

/** Party members: characters in the campaign or linked from its player list. */
export function useParty(): Character[] {
  const { lists, campaign } = useDM();
  return lists.characters.filter((c) => c.campaign_id === campaign.id || campaign.players.some((p) => p.character_id === c.id));
}
