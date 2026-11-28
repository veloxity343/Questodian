import { useEffect, useState } from "react";
import { api } from "../api";
import { Board } from "../components/board";
import { LoadError, Loading, Select, type Option } from "../components/ui";
import { useApp } from "../context/app";
import { useDialogs, useToast } from "../context/ui";
import { useLoad } from "../lib/hooks";
import { pickFiles, slug, today } from "../lib/util";
import { daysSince, saveFile, useLastBackup } from "../web/backup";
import { loadGoogle } from "../sync/google";
import {
  backUpNow, connectGoogleDrive, disconnect, forgetKept, keptVersions, restoreBackup, restoreKept, resumeSync, syncNow, useSyncStatus, type Kept,
} from "../sync/runner";
import { checkThisBrowser, reportText, type Check } from "../web/selftest";
import type { Kind } from "../types";

const KINDS: Kind[] = ["campaigns", "sessions", "characters", "handbook"];
const MODES: [string, string][] = [
  ["overwrite", "Overwrite — replace records that have the same id (restoring a backup)"],
  ["skip", "Skip — keep my existing records, only add new ones"],
  ["copy", "Copy — import everything as new records (sharing between DMs)"],
];

export function DataPage() {
  const { meta, campaigns, campaignId, refreshCampaigns } = useApp();
  const toast = useToast();
  const [exportId, setExportId] = useState(campaignId);
  const [mode, setMode] = useState("overwrite");
  const [report, setReport] = useState<string[]>([]);
  const [lastBackup, setLastBackup] = useLastBackup();
  /** A file waiting for a tap to open the Share sheet (see saveFile). */
  const [ready, setReady] = useState<{ name: string; text: string; full: boolean } | null>(null);
  const { data: counts, error, reload } = useLoad(() => Promise.all(KINDS.map(async (k) => [k, (await api.list(k)).length] as const)), []);
  if (error) return <LoadError error={error} />;
  if (!counts) return <Loading />;

  const doImport = async () => {
    const files = await pickFiles({ accept: ".json,.md,.markdown,.txt,application/json", multiple: true });
    if (!files.length) return;
    const lines: string[] = [];
    for (const f of files) {
      try {
        if (/\.(md|markdown|txt)$/i.test(f.name)) {
          const r = await api.importMarkdown([{ filename: f.name, text: f.text }], campaignId || "");
          lines.push(`✔ ${f.name}: handbook entry “${r.created[0].title}”`);
        } else {
          const r = await api.importData(JSON.parse(f.text), mode);
          const imp = Object.entries(r.imported).map(([k, v]) => `${v} ${k}`).join(", ") || "nothing new";
          const skip = Object.entries(r.skipped).map(([k, v]) => `${v} ${k}`).join(", ");
          lines.push(`✔ ${f.name}: imported ${imp}${skip ? ` (skipped ${skip})` : ""}`);
        }
      } catch (e) {
        lines.push(`✘ ${f.name}: ${(e as Error).message}`);
      }
    }
    setReport(lines);
    toast("Import finished");
    await refreshCampaigns();
    reload();
  };
  const save = async (file: { name: string; text: string; full: boolean }) => {
    const result = await saveFile(file.name, file.text);
    setReady(result === "tap-again" ? file : null);
    if (result === "saved" && file.full) setLastBackup(new Date().toISOString());
  };
  const fullBackup = async () =>
    save({ name: `questodian-backup-${today()}.questodian.json`, text: JSON.stringify(await api.exportAll(), null, 2), full: true });
  const exportCampaign = async () => {
    const id = exportId || campaigns[0].id;
    const name = campaigns.find((c) => c.id === id)?.name || "campaign";
    await save({ name: `${slug(name)}.questodian.json`, text: JSON.stringify(await api.exportAll(id), null, 2), full: false });
  };
  const sinceBackup = daysSince(lastBackup);

  return (
    <>
      <header className="page-head"><h1>Import / Export</h1></header>
      <Board id="data" items={[
        { id: "export", title: "Export", node: (
        <section className="panel">
          <h2>Export</h2>
          <p>Everything is plain JSON you can back up, version, or hand to another DM.</p>
          <div className="stack">
            <button type="button" className="btn primary" onClick={fullBackup}>
              ⬇ Full backup (all data)
            </button>
            {ready && (
              <div className="row gap">
                <span className="small">Ready to save:</span>
                <button type="button" className="btn primary" onClick={() => void save(ready)}>Save {ready.name}</button>
              </div>
            )}
            {campaigns.length > 0 && (
              <div className="row gap">
                <Select aria-label="Campaign to export" value={exportId || campaigns[0].id} options={campaigns.map((c): Option => [c.id, c.name])} onChange={setExportId} />
                <button type="button" className="btn" onClick={exportCampaign}>⬇ Export campaign</button>
              </div>
            )}
            <p className="muted small">Single characters, sessions and handbook entries can be exported as JSON or Markdown from their own pages.</p>
          </div>
          <h3>Stored locally</h3>
          <ul className="small">{counts.map(([k, n]) => <li key={k}>{n} {k}</li>)}</ul>
          {import.meta.env.MODE === "web" ? (
            <p className="muted small">
              Stored in this browser, on this device only. Removing the app from the Home Screen or clearing this site's data
              can delete it, so save a full backup now and then (to Files or iCloud Drive, say). Last full backup from this
              browser: {sinceBackup === null ? "none yet" : sinceBackup === 0 ? "today" : sinceBackup === 1 ? "yesterday" : `${sinceBackup} days ago`}.
            </p>
          ) : (
            <p className="muted small">Database: <code>{meta?.database || "?"}</code></p>
          )}
        </section>
        ) },
        { id: "import", title: "Import", node: (
        <section className="panel">
          <h2>Import</h2>
          <p>Accepts Questodian backups, campaign exports, single-record exports (.json), and Markdown files (.md) which become handbook entries.</p>
          <div className="stack">
            {MODES.map(([v, label]) => (
              <label className="check" key={v}>
                <input type="radio" name="mode" value={v} checked={v === mode} onChange={() => setMode(v)} /> {label}
              </label>
            ))}
            <button type="button" className="btn primary" onClick={doImport}>⬆ Choose files to import</button>
          </div>
          {report.length > 0 && (
            <div className="import-report">
              <ul>{report.map((l, i) => <li key={i} className={l.startsWith("✘") ? "bad" : ""}>{l}</li>)}</ul>
            </div>
          )}
        </section>
        ) },
        ...(import.meta.env.MODE === "web"
          ? [{ id: "sync", title: "Sync", node: <SyncPanel /> }, { id: "browser", title: "This browser", node: <BrowserCheck /> }]
          : []),
        { id: "about", title: "About the rules content", width: "full", node: (
        <section className="panel">
          <h2>About the rules content</h2>
          <p className="small">{meta?.srd?.attribution}</p>
          <p className="muted small">{meta?.srd?.source}</p>
          <p className="muted small">Questodian {meta?.version}</p>
        </section>
        ) },
      ]} />
    </>
  );
}

function ago(time: number): string {
  const minutes = Math.round((Date.now() - time) / 60000);
  return minutes < 1 ? "just now" : minutes === 1 ? "a minute ago" : minutes < 60 ? `${minutes} minutes ago` : new Date(time).toLocaleString();
}

/** Sync through the user's own Google Drive (sync/runner.ts). */
function SyncPanel() {
  const toast = useToast();
  const { confirm } = useDialogs();
  const status = useSyncStatus();
  const [busy, setBusy] = useState(false);
  const [, tick] = useState(0);
  // Google's script loads before the tap that signs in, so its window opens straight away.
  useEffect(() => {
    if (navigator.onLine) loadGoogle().catch(() => {});
    const timer = setInterval(() => tick((n) => n + 1), 30000); // keep "… minutes ago" current
    return () => clearInterval(timer);
  }, []);
  /** True if it worked (a failure is shown). */
  const act = async (what: () => Promise<void>) => {
    setBusy(true);
    try {
      await what();
      return true;
    } catch (e) {
      toast(`Sync: ${(e as Error).message}`);
      return false;
    } finally {
      setBusy(false);
    }
  };
  const waiting = status.waiting ? `${status.waiting} change${status.waiting === 1 ? "" : "s"} waiting to be sent.` : "";
  return (
    <section className="panel">
      <h2>Sync</h2>
      {status.phase === "off" ? (
        <>
          <p className="small">
            Keep this device and your others up to date through your own Google Drive. Questodian uses a hidden folder there that only it
            can see, and can't see anything else in your Drive. Without internet, everything works as before and changes wait.
          </p>
          <button type="button" className="btn primary" disabled={busy || !navigator.onLine} onClick={() => act(connectGoogleDrive)}>
            Connect Google Drive
          </button>
        </>
      ) : (
        <>
          <p className="small">
            {status.phase === "syncing" && "Syncing…"}
            {status.phase === "idle" && (status.last ? `Synced ${ago(status.last)}.` : "Connected to Google Drive.")}
            {status.phase === "paused" && "Paused: Google asks Questodian to sign in again every hour. Tap to carry on."}
            {status.phase === "error" && `Couldn't sync: ${status.error}`}
            {status.received ? ` ${status.received} change${status.received === 1 ? "" : "s"} came in.` : ""} {waiting}
          </p>
          {!!status.skipped?.length && (
            <ul className="browser-check small">
              {status.skipped.map((s) => <li key={s} className="bad">Not synced yet: {s}</li>)}
            </ul>
          )}
          <div className="row gap">
            {status.phase === "paused" ? (
              <button type="button" className="btn primary" disabled={busy} onClick={() => act(resumeSync)}>Carry on syncing</button>
            ) : (
              <button type="button" className="btn" disabled={busy || status.phase === "syncing"} onClick={() => act(syncNow)}>Sync now</button>
            )}
            <button type="button" className="btn ghost" disabled={busy} onClick={() => act(disconnect)}>Disconnect</button>
          </div>
          {!!status.replaced && <KeptVersions count={status.replaced} />}
          <h3>Backups in Google Drive</h3>
          <p className="muted small">
            A full backup is saved there once a day when something changed. The last 14 days' are kept, and one a week for 8 weeks before.
            Restoring makes this device's data what the backup holds, and sync takes it to your other devices; what's here now is backed up first.
          </p>
          {status.backupError && <p className="bad small">Couldn't back up: {status.backupError}</p>}
          {status.backups?.length ? (
            <ul className="backup-list small">
              {status.backups.map((b) => (
                <li key={b.name}>
                  <span>
                    {b.made ? b.made.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" }) : new Date(`${b.date}T12:00:00Z`).toLocaleDateString(undefined, { dateStyle: "medium" })}
                    {b.name.includes("before-restore") ? " (before a restore)" : ""}
                  </span>
                  <button type="button" className="btn ghost small" disabled={busy || status.phase === "syncing" || status.phase === "paused"} onClick={async () => {
                    if (await confirm(`Restore the backup from ${b.made ? b.made.toLocaleString() : b.date}? This device's data becomes what it holds (what's here now is backed up first), and sync takes it to your other devices.`, { label: "Restore", danger: true })) {
                      if (await act(() => restoreBackup(b.name))) toast("Restored");
                    }
                  }}>Restore</button>
                </li>
              ))}
            </ul>
          ) : (
            <p className="muted small">None yet: the first is made at the next sync.</p>
          )}
          <button type="button" className="btn small" disabled={busy || status.phase === "syncing" || status.phase === "paused"} onClick={() => act(backUpNow)}>Back up now</button>
        </>
      )}
    </section>
  );
}

const KIND_NAMES: Record<string, string> = {
  campaigns: "Campaign", sessions: "Session", characters: "Character", handbook: "Handbook entry", scenes: "Scene", npcs: "NPC",
  monsters: "Monster", notes: "Note", encounters: "Encounter", trackers: "Tracker", handouts: "Handout", rule_overrides: "Rules data edit",
};

/** Versions replaced (or deleted) by another device's change, kept 30 days to restore. */
function KeptVersions({ count }: { count: number }) {
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const [list, setList] = useState<Kept[] | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (open) keptVersions().then(setList, (e: Error) => toast(e.message, "error"));
  }, [open, count, toast]);
  const act = async (what: () => Promise<void>) => {
    setBusy(true);
    try {
      await what();
      setList(await keptVersions());
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setBusy(false);
    }
  };
  return (
    <details className="kept" open={open} onToggle={(e) => setOpen((e.target as HTMLDetailsElement).open)}>
      <summary className="small">
        {count} version{count === 1 ? "" : "s"} replaced or deleted by another device, kept for 30 days
      </summary>
      {list && (
        <ul className="backup-list small">
          {list.map((k) => (
            <li key={k.n}>
              <span>
                {KIND_NAMES[k.kind] ?? k.kind}: {k.title || "(untitled)"}
                <span className="muted"> · {new Date(k.replaced).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })}{k.exists ? "" : " · deleted"}</span>
              </span>
              <span className="row gap">
                <button type="button" className="btn ghost small" disabled={busy} onClick={() => act(async () => {
                  const image = await restoreKept(k);
                  toast(image ? "Restored" : "Restored, without its image (it couldn't be fetched)");
                })}>Restore</button>
                <button type="button" className="btn ghost small" disabled={busy} onClick={() => act(() => forgetKept(k))}>Dismiss</button>
              </span>
            </li>
          ))}
        </ul>
      )}
    </details>
  );
}

/** The web version's self-test (web/selftest.ts), with a report to copy and send. */
function BrowserCheck() {
  const toast = useToast();
  const [checks, setChecks] = useState<Check[] | null>(null);
  const [running, setRunning] = useState(false);
  const run = async () => {
    setRunning(true);
    try {
      setChecks(await checkThisBrowser());
    } finally {
      setRunning(false);
    }
  };
  const copy = async () => {
    const ok = await navigator.clipboard?.writeText(reportText(checks || [])).then(() => true, () => false);
    toast(ok ? "Report copied" : "Couldn't copy: select the report and copy it");
  };
  const failed = checks?.filter((c) => c.ok === false).length ?? 0;
  return (
    <section className="panel">
      <h2>This browser</h2>
      <p className="small">Checks what the web version needs, here. It adds a test handout and deletes it again; nothing else changes.</p>
      <div className="row gap">
        <button type="button" className="btn" onClick={run} disabled={running}>{running ? "Checking…" : "Check this browser"}</button>
        {checks && <button type="button" className="btn ghost" onClick={copy}>Copy report</button>}
      </div>
      {checks && (
        <>
          <p className={failed ? "bad small" : "small"}>{failed ? `${failed} check${failed === 1 ? "" : "s"} failed.` : "Everything checked works."}</p>
          <ul className="browser-check small">
            {checks.map((c) => (
              <li key={c.name} className={c.ok === false ? "bad" : ""}>
                <span aria-hidden="true">{c.ok === null ? "ℹ" : c.ok ? "✔" : "✘"}</span> {c.name}{c.detail && <span className="muted">: {c.detail}</span>}
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}
