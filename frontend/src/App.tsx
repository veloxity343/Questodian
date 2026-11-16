import { useEffect, useRef } from "react";
import { Navigate, NavLink, Route, Routes, useLocation, useNavigate, useParams } from "react-router-dom";
import { useApp } from "./context/app";
import { ProjectionView } from "./dm/docs2";
import { DmScreen } from "./dm/screen";
import { usePref } from "./lib/hooks";
import { CampaignList } from "./views/campaigns";
import { CharacterList, CharacterSheet } from "./views/characters";
import { DataPage } from "./views/data";
import { HandbookEditor, HandbookList } from "./views/handbook";
import { Reference } from "./views/reference";
import { Tools } from "./views/tools";

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
  return (
    <header className="topbar">
      <button type="button" className="icon-btn menu-btn" aria-label="Toggle navigator" disabled={!onDm} onClick={() => setNav(!nav)}>☰</button>
      <NavLink className="brand" to="/"><span aria-hidden="true">✦</span> Questodian</NavLink>
      <nav className="modes" aria-label="Main">
        {MODES.map((m) => <NavLink key={m.to} to={m.to} end={m.end} className={({ isActive }) => (isActive ? "on" : "")}>{m.label}</NavLink>)}
      </nav>
      <span className="grow" />
      {onDm && (
        <button type="button" className="search-btn" onClick={() => document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", ctrlKey: true }))}>
          Search <kbd className="key-hint">Ctrl K</kbd>
        </button>
      )}
      <CampaignChip />
      <button type="button" className="icon-btn" onClick={toggleTheme} aria-label="Toggle light/dark theme" title="Toggle theme">◐</button>
    </header>
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
          <Route path="/library/*" element={<Reference />} />
          <Route path="/reference/*" element={<ReferenceRedirect />} />
          <Route path="/handbook" element={<HandbookList />} />
          <Route path="/handbook/:id" element={<HandbookEditor />} />
          <Route path="/tools" element={<Tools />} />
          <Route path="/data" element={<DataPage />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </main>
    </div>
  );
}
