// Update checks, which the desktop app does (src-tauri/src/updates.rs): on first launch it asks
// whether to check GitHub for new releases, then checks once a day while that's on. A new
// version gets one pop-up; Settings shows it until you update. In a browser (`questodian serve`)
// there's no desktop app, so none of this shows.

import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import { desktopUpdates } from "../lib/desktop";
import { Modal } from "./ui";

export interface UpdateState {
  /** This app's version. */
  current: string;
  /** null until the first-launch question is answered. */
  check: boolean | null;
  checking: boolean;
  last_check: { when: string; answered: boolean } | null;
  available: { version: string; url: string } | null;
  /** The newest version the pop-up has been shown for. */
  announced: string | null;
  /** Whether this copy can install an update itself, or the release page has to open instead. */
  can_install: boolean;
  /** How far an update's download has got, 0 to 1, while one is being installed. */
  installing: number | null;
  error: string | null;
}

interface Updates {
  /** null in a browser, or until the desktop app answers. */
  state: UpdateState | null;
  setChecking: (on: boolean) => Promise<void>;
  checkNow: () => Promise<void>;
  install: () => Promise<void>;
  openPage: () => Promise<void>;
}

const UpdatesContext = createContext<Updates>({
  state: null,
  setChecking: async () => {},
  checkNow: async () => {},
  install: async () => {},
  openPage: async () => {},
});
export const useUpdates = () => useContext(UpdatesContext);

const call = (method: string, path = "", body?: unknown) => desktopUpdates<UpdateState>(method, path, body);

export function UpdatesProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<UpdateState | null>(null);
  const refresh = useCallback(async () => setState(await call("GET")), []);

  useEffect(() => {
    void refresh();
    // The desktop app checks on its own schedule; look in on it now and then, and when the window comes back.
    const timer = setInterval(refresh, 10 * 60 * 1000);
    window.addEventListener("focus", refresh);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", refresh);
    };
  }, [refresh]);

  // While checking or installing, follow along more closely.
  useEffect(() => {
    if (!state?.checking && state?.installing == null) return;
    const timer = setTimeout(refresh, 700);
    return () => clearTimeout(timer);
  }, [state, refresh]);

  const value: Updates = {
    state,
    setChecking: async (on) => setState((await call("PUT", "", { check: on })) ?? state),
    checkNow: async () => setState((await call("POST", "/check")) ?? state),
    install: async () => setState((await call("POST", "/install")) ?? state),
    openPage: async () => void (await call("POST", "/page")),
  };
  return (
    <UpdatesContext.Provider value={value}>
      {children}
      <UpdatePrompts updates={value} onAnnounced={async (version) => setState((await call("PUT", "", { announced: version })) ?? state)} />
    </UpdatesContext.Provider>
  );
}

/** The first-launch question, and one pop-up for each new version. */
function UpdatePrompts({ updates, onAnnounced }: { updates: Updates; onAnnounced: (version: string) => Promise<void> }) {
  const { state } = updates;
  // Closing the question without answering asks again next launch.
  const [askLater, setAskLater] = useState(false);
  if (!state) return null;
  if (state.check === null) {
    if (askLater) return null;
    return (
      <Modal title="Check for updates?" onClose={() => setAskLater(true)} actions={[
        { label: "No thanks", onClick: () => updates.setChecking(false) },
        { label: "Check daily", primary: true, onClick: () => updates.setChecking(true) },
      ]}>
        <p>Questodian can look for a new version on GitHub once a day and tell you when there is one. It only asks GitHub which version is the latest: nothing about you or your campaigns is sent, and nothing is installed unless you say so.</p>
        <p className="muted small">You can change this, or check by hand, in Settings.</p>
      </Modal>
    );
  }
  const available = state.available;
  if (available && state.check && state.announced !== available.version && state.installing === null) {
    // However it's closed, this version's pop-up has been seen; Settings keeps offering it.
    return (
      <Modal title={`Questodian ${available.version} is out`} onClose={() => void onAnnounced(available.version)} actions={[
        { label: "What's new", onClick: () => updates.openPage().then(() => false) },
        { label: "Not now" },
        { label: state.can_install ? "Update" : "Download", primary: true, onClick: () => (state.can_install ? updates.install() : updates.openPage()) },
      ]}>
        <p>You have {state.current}.{" "}
          {state.can_install
            ? "Update downloads the new version from GitHub, checks it's genuine, installs it and restarts Questodian. Your campaigns and settings are kept."
            : "Download opens the release page, where you can get the new version for this computer. Your campaigns and settings are kept."}</p>
      </Modal>
    );
  }
  return null;
}
