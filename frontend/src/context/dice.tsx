// Dice rolling shared by every screen: roll history, macros, the floating
// quick-roll tray, and click-to-roll on any rendered `.dice-link`.

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Link, useLocation } from "react-router-dom";
import { api } from "../api";
import { usePref } from "../lib/hooks";
import { fmtDate } from "../lib/util";
import type { RollEntry, RollResult } from "../types";
import { useApp } from "./app";
import { useToast } from "./ui";

export interface Macro {
  label: string;
  expr: string;
}

const DEFAULT_MACROS: Macro[] = [
  { label: "Attack", expr: "d20+5" },
  { label: "Advantage", expr: "2d20kh1" },
  { label: "Disadvantage", expr: "2d20kl1" },
  { label: "Fireball", expr: "8d6" },
  { label: "Ability score", expr: "4d6dl1" },
];

interface DiceState {
  roll: (expression: string, label?: string) => Promise<RollEntry | null>;
  history: RollEntry[];
  clearHistory: () => void;
  macros: Macro[];
  setMacros: (m: Macro[]) => void;
  logRoll: (r: RollEntry) => Promise<void>;
}

const DiceContext = createContext<DiceState | null>(null);

export function useDice() {
  const ctx = useContext(DiceContext);
  if (!ctx) throw new Error("useDice outside DiceProvider");
  return ctx;
}

/** "2d20kh1 [~~3~~, 19] + 5 = 24" with dropped dice struck through. */
export function Breakdown({ result }: { result: RollResult }) {
  return (
    <span className="breakdown">
      {result.breakdown.split(/(~~-?\d+~~)/).map((p, i) => {
        const m = p.match(/^~~(-?\d+)~~$/);
        return m ? <del key={i}>{m[1]}</del> : p;
      })}
    </span>
  );
}

export function DiceProvider({ children }: { children: ReactNode }) {
  const toast = useToast();
  const { sessionId } = useApp();
  // While the quick roll is open it shows each result itself, so rolls don't pop up a toast.
  const [trayOpen, setTrayOpen] = useState(false);
  const quietRef = useRef(trayOpen);
  quietRef.current = trayOpen;
  const [history, setHistory] = usePref<RollEntry[]>("rolls", []);
  const [macros, setMacros] = usePref<Macro[]>("macros", DEFAULT_MACROS);

  const roll = useCallback(
    async (expression: string, label = "") => {
      try {
        const result = await api.roll(expression);
        const entry: RollEntry = { ...result, label, time: new Date().toISOString() };
        setHistory((h) => [entry, ...h].slice(0, 100));
        const flag = entry.critical ? <span className="tag crit">NAT 20</span> : entry.fumble ? <span className="tag fumble">NAT 1</span> : null;
        if (!quietRef.current) toast(
          <div className="roll-toast">
            <div className="roll-toast-head"><span>{label || expression}</span>{flag}</div>
            <div className="roll-total">{entry.total}</div>
            <Breakdown result={entry} />
          </div>,
          entry.critical ? "success" : entry.fumble ? "error" : "roll",
          4500,
        );
        return entry;
      } catch (e) {
        toast((e as Error).message, "error");
        return null;
      }
    },
    [setHistory, toast],
  );

  const logRoll = useCallback(
    async (r: RollEntry) => {
      if (!sessionId) return toast("Open a session first to log rolls", "error");
      try {
        const text = `${r.label ? r.label + ": " : ""}${r.expression} → ${r.total} (${r.breakdown.replace(/~~/g, "")})`;
        await api.log(sessionId, text, "roll");
        toast("Logged to session");
      } catch (e) {
        toast((e as Error).message, "error");
      }
    },
    [sessionId, toast],
  );

  // Any rendered "dice-link" anywhere in the app rolls on click.
  useEffect(() => {
    const onClick = (e: MouseEvent) => {
      const btn = (e.target as HTMLElement).closest?.<HTMLElement>(".dice-link");
      if (!btn?.dataset.roll) return;
      e.preventDefault();
      const ctx = btn.closest<HTMLElement>("[data-roll-context]")?.dataset.rollContext || "";
      void roll(btn.dataset.roll, ctx);
    };
    document.addEventListener("click", onClick);
    return () => document.removeEventListener("click", onClick);
  }, [roll]);

  return (
    <DiceContext.Provider value={{ roll, history, clearHistory: () => setHistory([]), macros, setMacros, logRoll }}>
      {children}
      <DiceTray open={trayOpen} setOpen={setTrayOpen} />
    </DiceContext.Provider>
  );
}

export function HistoryItem({ r }: { r: RollEntry }) {
  const { roll, logRoll } = useDice();
  return (
    <li className={`roll-item ${r.critical ? "crit" : ""} ${r.fumble ? "fumble" : ""}`}>
      <div className="roll-line">
        <strong className="roll-total-sm">{r.total}</strong>
        <span className="muted">{r.label ? `${r.label} · ${r.expression}` : r.expression}</span>
        <span className="roll-actions">
          <button type="button" className="link" title="Roll again" onClick={() => roll(r.expression, r.label)}>↻</button>
          <button type="button" className="link" title="Add to session log" onClick={() => logRoll(r)}>✎</button>
        </span>
      </div>
      <div className="roll-detail"><Breakdown result={r} /></div>
      {r.time && <time className="muted small">{fmtDate(r.time)}</time>}
    </li>
  );
}

/** The dice on every roller. */
export const DIE_SIDES = [4, 6, 8, 10, 12, 20, 100];

/** The floating quick roll, on every screen but the Tools page (which has the full roller). */
function DiceTray({ open, setOpen }: { open: boolean; setOpen: (fn: (o: boolean) => boolean) => void }) {
  const onTools = useLocation().pathname === "/tools";
  useEffect(() => {
    if (onTools) setOpen(() => false);
  }, [onTools, setOpen]);
  return onTools ? null : <Tray open={open} setOpen={setOpen} />;
}

function Tray({ open, setOpen }: { open: boolean; setOpen: (fn: (o: boolean) => boolean) => void }) {
  const { roll, history, clearHistory } = useDice();
  const [expr, setExpr] = useState("");
  const [showHistory, setShowHistory] = usePref("dice.tray.history", false);
  const input = useRef<HTMLInputElement>(null);
  const last = history[0];
  const recent = useMemo(() => history.slice(1, 30), [history]);

  useEffect(() => {
    if (open) input.current?.focus();
  }, [open]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement;
      const tag = (el.tagName || "").toLowerCase();
      const typing = ["input", "textarea", "select"].includes(tag) || el.isContentEditable;
      if (e.key.toLowerCase() === "r" && !e.metaKey && !e.ctrlKey && !e.altKey && !typing) {
        e.preventDefault();
        setOpen((o) => !o);
      }
      if (e.key === "Escape") setOpen(() => false);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [setOpen]);

  const go = () => expr.trim() && roll(expr.trim());

  return (
    <div className="tray">
      {open && (
        <section className="tray-panel" aria-label="Quick roll">
          <div className="tray-head">
            <strong>Quick roll</strong>
            <Link to="/tools" className="small">Full roller →</Link>
          </div>
          <div className="row gap">
            <input ref={input} type="text" placeholder="e.g. 2d20kh1+5" aria-label="Dice expression" autoComplete="off"
              value={expr} onChange={(e) => setExpr(e.target.value)} onKeyDown={(e) => e.key === "Enter" && go()} />
            <button type="button" className="btn primary" onClick={go}>Roll</button>
          </div>
          <div className="die-row">
            {DIE_SIDES.map((s) => <button type="button" key={s} className="die-chip" onClick={() => roll(`1d${s}`)}>d{s}</button>)}
            <button type="button" className="die-chip" title="Advantage" onClick={() => roll("2d20kh1", "Advantage")}>ADV</button>
            <button type="button" className="die-chip" title="Disadvantage" onClick={() => roll("2d20kl1", "Disadvantage")}>DIS</button>
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
            <button type="button" className="link small muted" aria-expanded={showHistory} onClick={() => setShowHistory(!showHistory)}>{showHistory ? "▾" : "▸"} History</button>
            <span className="grow" />
            {history.length > 0 && <button type="button" className="link small muted" onClick={clearHistory}>clear</button>}
          </div>
          {showHistory && <ul className="roll-list">{recent.map((r) => <HistoryItem key={r.time + r.expression} r={r} />)}</ul>}
        </section>
      )}
      <button type="button" className="tray-fab" aria-label="Dice roller" aria-expanded={open} title="Dice roller (press R)" onClick={() => setOpen((o) => !o)}>
        <span aria-hidden="true">🎲</span>
      </button>
    </div>
  );
}
