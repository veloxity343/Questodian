import { useState } from "react";
import { api } from "../api";
import { Board } from "../components/board";
import { LoadError, Loading, Select, type Option } from "../components/ui";
import { useApp } from "../context/app";
import { useToast } from "../context/ui";
import { useLoad } from "../lib/hooks";
import { pickFiles, slug, today } from "../lib/util";
import { daysSince, saveFile, useLastBackup } from "../web/backup";
import { checkThisBrowser, reportText, type Check } from "../web/selftest";
import type { ExportBundle, Kind, Unreadable } from "../types";

const KINDS: Kind[] = ["campaigns", "sessions", "characters", "handbook"];
const MODES: [string, string][] = [
  ["overwrite", "Overwrite — replace records that have the same id (restoring a backup)"],
  ["skip", "Skip — keep my existing records, only add new ones"],
  ["copy", "Copy — import everything as new records (sharing between DMs)"],
];

const records = (n: number) => (n === 1 ? "1 record" : `${n} records`);

/** Records that can't be read, which exports leave out. */
function UnreadableList({ records: list }: { records: Unreadable[] }) {
  return (
    <ul>
      {list.map((r, i) => <li key={i}>{r.kind} <code>{r.id || "(no id)"}</code>: {r.error}</li>)}
    </ul>
  );
}

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
  // Loaded on its own, so a problem here can't take the page (and importing) away.
  const { data: check, error: checkError, reload: recheck } = useLoad(() => api.unreadable(), []);
  const unreadable = check ?? [];
  if (error) return <LoadError error={error} />;
  if (!counts) return <Loading />;

  // A backup that leaves records out must say so, not just save.
  const warnIfIncomplete = (bundle: ExportBundle) => {
    const missing = bundle.unreadable?.length || 0;
    if (missing) toast(`Exported, but ${records(missing)} can't be read and ${missing === 1 ? "isn't" : "aren't"} in the file`, "error", 8000);
  };

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
          const missing = r.unreadable?.length || 0;
          if (missing) {
            const which = r.unreadable.map((u) => `${u.kind} ${u.id || "(no id)"}`).join(", ");
            lines.push(`✘ ${f.name} is incomplete: ${records(missing)} couldn't be read when it was made and ${missing === 1 ? "isn't" : "aren't"} in it (${which})`);
          }
        }
      } catch (e) {
        lines.push(`✘ ${f.name}: ${(e as Error).message}`);
      }
    }
    setReport(lines);
    toast("Import finished");
    await refreshCampaigns();
    reload();
    recheck();
  };
  const save = async (file: { name: string; text: string; full: boolean }) => {
    const result = await saveFile(file.name, file.text);
    setReady(result === "tap-again" ? file : null);
    if (result === "saved" && file.full) setLastBackup(new Date().toISOString());
  };
  const fullBackup = async () => {
    const bundle = await api.exportAll();
    warnIfIncomplete(bundle);
    await save({ name: `questodian-backup-${today()}.questodian.json`, text: JSON.stringify(bundle, null, 2), full: true });
  };
  const exportCampaign = async () => {
    const id = exportId || campaigns[0].id;
    const name = campaigns.find((c) => c.id === id)?.name || "campaign";
    const bundle = await api.exportAll(id);
    warnIfIncomplete(bundle);
    await save({ name: `${slug(name)}.questodian.json`, text: JSON.stringify(bundle, null, 2), full: false });
  };
  const sinceBackup = daysSince(lastBackup);

  return (
    <>
      <header className="page-head"><h1>Import / Export</h1></header>
      {checkError && <p className="bad small">Couldn't check for records that can't be read: {checkError.message}</p>}
      {unreadable.length > 0 && (
        <section className="panel warnings" role="alert">
          <h2>{records(unreadable.length)} can't be read</h2>
          <p>
            {unreadable.length === 1 ? "It isn't" : "They aren't"} shown anywhere in Questodian and won't be in backups or
            exports until fixed. Nothing has been deleted: {unreadable.length === 1 ? "it's" : "they're"} still in the database.
          </p>
          <UnreadableList records={unreadable} />
        </section>
      )}
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
        ...(import.meta.env.MODE === "web" ? [{ id: "browser", title: "This browser", node: <BrowserCheck /> }] : []),
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
