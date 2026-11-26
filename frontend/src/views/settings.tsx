// Settings: update checks (desktop app only) and the theme.

import { Board } from "../components/board";
import { Checkbox } from "../components/ui";
import { useUpdates } from "../context/updates";
import { usePref } from "../lib/hooks";
import { fmtDate } from "../lib/util";

export function Settings() {
  return (
    <>
      <header className="page-head"><h1>Settings</h1></header>
      <Board id="settings" items={[
        { id: "updates", title: "Updates", node: <UpdatesPanel /> },
        { id: "appearance", title: "Appearance", node: <ThemePanel /> },
      ]} />
    </>
  );
}

function UpdatesPanel() {
  const { state, setChecking, checkNow, install, openPage } = useUpdates();
  if (!state) {
    return (
      <section className="panel">
        <h2>Updates</h2>
        <p className="muted">
          {import.meta.env.MODE === "web"
            ? "This web version updates itself: while you're online, a new version downloads in the background, and it's used once every Questodian window has been closed."
            : "The desktop app checks for updates. In a browser, get new versions from the Releases page on GitHub."}
        </p>
      </section>
    );
  }
  const last = state.last_check;
  const status = state.installing !== null
    ? `Downloading ${state.available?.version}… ${Math.round(state.installing * 100)}%`
    : state.checking ? "Checking…"
    : state.available ? `Questodian ${state.available.version} is out (you have ${state.current}).`
    : last?.answered ? `You have the latest version (${state.current}).`
    : last ? "The last check couldn't reach GitHub; it tries again in a few hours."
    : `You have ${state.current}.`;
  return (
    <section className="panel">
      <h2>Updates</h2>
      <label className="check">
        <Checkbox checked={!!state.check} onChange={(on) => void setChecking(on)} /> Check for updates once a day
      </label>
      <p className="muted small">Only asks GitHub which version is the latest. Nothing about you or your campaigns is sent, and nothing is installed unless you say so.</p>
      <p><strong>{status}</strong>{last && <span className="muted small"> Last checked {fmtDate(last.when)}.</span>}</p>
      {state.error && <p className="bad-text">{state.error}</p>}
      <div className="row gap wrap">
        <button type="button" className="btn" disabled={state.checking || state.installing !== null} onClick={() => void checkNow()}>Check now</button>
        {state.available && (
          <>
            <button type="button" className="btn primary" disabled={state.installing !== null} onClick={() => void (state.can_install ? install() : openPage())}>
              {state.can_install ? `Update to ${state.available.version}` : `Download ${state.available.version}`}
            </button>
            <button type="button" className="btn" onClick={() => void openPage()}>What's new</button>
          </>
        )}
      </div>
    </section>
  );
}

function ThemePanel() {
  const [theme, setTheme] = usePref<"" | "light" | "dark">("theme", "");
  return (
    <section className="panel">
      <h2>Appearance</h2>
      <div className="row gap wrap" role="radiogroup" aria-label="Theme">
        {([["", "Dark (default)"], ["light", "Light"]] as const).map(([value, label]) => (
          <label key={value} className="check"><input type="radio" name="theme" checked={(theme || "") === value || (value === "" && theme === "dark")} onChange={() => setTheme(value)} /> {label}</label>
        ))}
      </div>
    </section>
  );
}
