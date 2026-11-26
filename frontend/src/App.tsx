import { useEffect, useRef } from "react";
import { Navigate, NavLink, Route, Routes, useLocation, useNavigate, useParams } from "react-router-dom";
import { useApp } from "./context/app";
import { ProjectionView } from "./dm/docs2";
import { DmScreen } from "./dm/screen";
import { usePref } from "./lib/hooks";
import { CampaignList } from "./views/campaigns";
import { CharacterList, CharacterSheet } from "./views/characters";
import { RulesData } from "./views/rules-data";
import { Settings } from "./views/settings";
import { UpdatesProvider, useUpdates } from "./context/updates";
import { DataPage } from "./views/data";
import { HandbookEditor, HandbookList } from "./views/handbook";
import { Reference } from "./views/reference";
import { Tools } from "./views/tools";
import { backupDue, BACKUP_EVERY_DAYS, useLastBackup } from "./web/backup";

const MODES = [
  { to: "/", label: "DM Screen", end: true },
  { to: "/characters", label: "Characters" },
  { to: "/library", label: "Library" },
  { to: "/handbook", label: "Handbook" },
  { to: "/tools", label: "Tools" },
  { to: "/data", label: "Data" },
];

function useTheme() {
  const [theme, setTheme] = usePref<"" | "light" | "dark">("theme", "");
  useEffect(() => {
    if (theme) document.documentElement.dataset.theme = theme;
    else delete document.documentElement.dataset.theme;
  }, [theme]);
  return () => {
    setTheme(theme === "light" ? "dark" : "light"); // dark is the default
  };
}

function CampaignChip() {
  const { campaigns, campaign, setCampaignId, meta } = useApp();
  const navigate = useNavigate();
  return (
    <div className="campaign-chip">
      <span className="eyebrow">Campaign</span>
      <select aria-label="Active campaign" value={campaign?.id ?? ""} onChange={(e) => {
        if (e.target.value === "__manage") return navigate("/campaigns");
        setCampaignId(e.target.value);
      }}>
        {!campaign && <option value="">None yet</option>}
        {campaigns.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
        <option value="__manage">Manage campaigns…</option>
      </select>
      {campaign && <span className="edition-badge" title="Rules edition">{meta?.editions?.[campaign.edition] ?? campaign.edition}</span>}
    </div>
  );
}

function TopBar() {
  const toggleTheme = useTheme();
  const [nav, setNav] = usePref("dm.nav", true);
  const { pathname } = useLocation();
  const onDm = pathname === "/";
  const { campaigns } = useApp();
  const [lastBackup] = useLastBackup();
  const remind = backupDue(lastBackup, campaigns.length > 0);
  return (
    <header className="topbar">
      <button type="button" className="icon-btn menu-btn" aria-label="Show or hide the navigator" disabled={!onDm}
        title={onDm ? "Show or hide the navigator (scenes, NPCs, notes…)" : "The navigator is on the DM screen"} onClick={() => setNav(!nav)}>☰</button>
      <NavLink className="brand" to="/campaigns" title="All campaigns"><span aria-hidden="true">✦</span> Questodian</NavLink>
      <nav className="modes" aria-label="Main">
        {MODES.map((m) => (
          <NavLink key={m.to} to={m.to} end={m.end} className={({ isActive }) => (isActive ? "on" : "")}
            title={m.to === "/data" && remind ? `No full backup from this browser in ${BACKUP_EVERY_DAYS} days` : undefined}>
            {m.label}{m.to === "/data" && remind && <span className="nav-dot" aria-label="(backup due)" />}
          </NavLink>
        ))}
      </nav>
      <span className="grow" />
      {onDm && (
        <button type="button" className="search-btn" onClick={() => document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", ctrlKey: true }))}>
          Search <kbd>Ctrl K</kbd>
        </button>
      )}
      <CampaignChip />
      <button type="button" className="icon-btn" onClick={toggleTheme} aria-label="Toggle light/dark theme" title="Toggle theme">◐</button>
      <SettingsLink />
    </header>
  );
}

/** The settings button, with a dot while a new version is waiting. */
function SettingsLink() {
  const { state } = useUpdates();
  const waiting = !!state?.check && !!state.available;
  return (
    <NavLink to="/settings" className="icon-btn settings-btn" aria-label={waiting ? "Settings: an update is available" : "Settings"}
      title={waiting ? `Settings: Questodian ${state!.available!.version} is out` : "Settings"}>
      ⚙{waiting && <span className="dot" aria-hidden="true" />}
    </NavLink>
  );
}

/** Old links (/sessions/…, /campaigns/:id, /reference/…) still land somewhere sensible. */
function CampaignRedirect() {
  const { id = "" } = useParams();
  const { setCampaignId } = useApp();
  useEffect(() => setCampaignId(id), [id, setCampaignId]);
  return <Navigate to="/" replace />;
}

function ReferenceRedirect() {
  const rest = useParams()["*"] || "";
  return <Navigate to={`/library/${rest}`} replace />;
}

function ProjectRoute() {
  const { id = "" } = useParams();
  return <ProjectionView id={id} />;
}

export default function App() {
  const { pathname } = useLocation();
  const main = useRef<HTMLElement>(null);
  useEffect(() => main.current?.focus({ preventScroll: true }), [pathname]);
  if (pathname === "/project" || pathname.startsWith("/project/")) {
    return (
      <Routes>
        <Route path="/project" element={<ProjectionView />} />
        <Route path="/project/:id" element={<ProjectRoute />} />
      </Routes>
    );
  }
  const onDm = pathname === "/";
  return (
    <UpdatesProvider>
    <div className="app">
      <TopBar />
      <main ref={main} tabIndex={-1} className={onDm ? "app-main dm-main" : "app-main page"}>
        <Routes>
          <Route path="/" element={<DmScreen />} />
          <Route path="/campaigns" element={<CampaignList />} />
          <Route path="/campaigns/:id" element={<CampaignRedirect />} />
          <Route path="/sessions/*" element={<Navigate to="/" replace />} />
          <Route path="/characters" element={<CharacterList />} />
          <Route path="/characters/:id" element={<CharacterSheet />} />
          <Route path="/rules-data" element={<RulesData />} />
          <Route path="/rules-data/:kind" element={<RulesData />} />
          <Route path="/library/*" element={<Reference />} />
          <Route path="/reference/*" element={<ReferenceRedirect />} />
          <Route path="/handbook" element={<HandbookList />} />
          <Route path="/handbook/:id" element={<HandbookEditor />} />
          <Route path="/tools" element={<Tools />} />
          <Route path="/data" element={<DataPage />} />
          <Route path="/settings" element={<Settings />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </main>
    </div>
    </UpdatesProvider>
  );
}
