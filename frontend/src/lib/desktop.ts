// The desktop app's own addresses (src-tauri/src/main.rs). In a browser they don't exist, and
// each of these falls back to what a browser can do.

/** Running inside the desktop app, whose window shows questodian:// (or questodian.localhost on Windows). */
export const isDesktop = () => location.protocol === "questodian:" || location.hostname === "questodian.localhost";

/** Open the players' window at `path`, or point the open one there: a second window to drag onto a projector or TV. */
export function openPlayersWindow(path = `${import.meta.env.BASE_URL}project`) {
  if (isDesktop()) void fetch("/desktop/players", { method: "POST", body: path });
  else window.open(path, "questodian-players", "popup,width=1280,height=800");
}

/** Toggle full screen on the players' window: the desktop app's own, else the browser's. */
export async function togglePlayersFullscreen() {
  if (isDesktop()) {
    const res = await fetch("/desktop/players/fullscreen", { method: "POST" }).catch(() => null);
    if (res?.ok) return;
  }
  if (document.fullscreenElement) await document.exitFullscreen().catch(() => {});
  else await document.documentElement.requestFullscreen?.().catch(() => {});
}

/** A request to the desktop app's update check (`/desktop/updates...`); null in a browser or on failure. */
export async function desktopUpdates<T>(method: string, path = "", body?: unknown): Promise<T | null> {
  if (!isDesktop()) return null;
  try {
    const res = await fetch(`/desktop/updates${path}`, {
      method,
      headers: body === undefined ? {} : { "Content-Type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    return res.ok ? ((await res.json()) as T) : null;
  } catch {
    return null;
  }
}
