// Signing in to Google for sync, in the browser (the web version): Google's sign-in script
// gives an access token for an hour, and no lasting one. A new one needs a tap (browsers block
// the popup otherwise); with the app already allowed, the popup closes by itself in about a
// second. Tested on desktop Chrome, iPad Safari and the iPad Home Screen app.

/** Questodian's OAuth client for the web version (public by design: Google identifies the site
 *  by its origin, registered with the client). */
export const GOOGLE_WEB_CLIENT = "177586812233-1f6utln2a5759dqjb62r6c0vf53tf2br.apps.googleusercontent.com";
/** Only the app's own hidden folder in the user's Drive: nothing else there is visible to it. */
export const DRIVE_SCOPE = "https://www.googleapis.com/auth/drive.appdata";

const TOKEN_KEY = "questodian-google-token";
/** Stop using a token a little before Google does. */
const MARGIN_MS = 60_000;

interface TokenResponse {
  access_token?: string;
  expires_in?: number | string;
  scope?: string;
  error?: string;
  error_description?: string;
}
interface Gis {
  accounts: {
    oauth2: {
      initTokenClient(config: {
        client_id: string;
        scope: string;
        callback: (response: TokenResponse) => void;
        error_callback?: (error: { type: string; message?: string }) => void;
      }): { requestAccessToken(overrides?: { prompt?: string }): void };
      revoke(token: string, done?: () => void): void;
    };
  };
}

let gis: Promise<Gis> | null = null;

/** Google's sign-in script, loaded when first needed (load it before the tap that signs in, so
 *  the popup opens straight from the tap). */
export function loadGoogle(): Promise<Gis> {
  gis ??= new Promise<Gis>((resolve, reject) => {
    const script = document.createElement("script");
    script.src = "https://accounts.google.com/gsi/client";
    script.async = true;
    script.onload = () => {
      const google = (window as { google?: Gis }).google;
      if (google?.accounts?.oauth2) resolve(google);
      else reject(new Error("Google's sign-in didn't load properly"));
    };
    script.onerror = () => reject(new Error("couldn't reach Google (offline?)"));
    document.head.appendChild(script);
  }).catch((e) => {
    gis = null; // try again next time
    throw e;
  });
  return gis;
}

interface Stored {
  token: string;
  expires: number;
}

function stored(): Stored | null {
  try {
    const value = JSON.parse(localStorage.getItem(TOKEN_KEY) || "null") as Stored | null;
    return value && value.expires - MARGIN_MS > Date.now() ? value : null;
  } catch {
    return null;
  }
}


/** Sign in, or get a new token: call straight from a tap. `first` shows Google's account
 *  choice; otherwise the popup only shows a screen if Google needs one. */
export async function signInToGoogle(first: boolean): Promise<string> {
  const google = await loadGoogle();
  const response = await new Promise<TokenResponse>((resolve, reject) => {
    const client = google.accounts.oauth2.initTokenClient({
      client_id: GOOGLE_WEB_CLIENT,
      scope: DRIVE_SCOPE,
      callback: (r) => (r.error ? reject(new Error(r.error_description || r.error)) : resolve(r)),
      error_callback: (e) =>
        reject(new Error(e.type === "popup_closed" ? "the Google window was closed" : e.type === "popup_failed_to_open" ? "the Google window was blocked" : e.message || e.type)),
    });
    client.requestAccessToken({ prompt: first ? "select_account" : "" });
  });
  if (!response.access_token) throw new Error("Google gave no access");
  if (!response.scope?.split(" ").includes(DRIVE_SCOPE)) throw new Error("Questodian wasn't allowed its Drive folder: tick the box when Google asks");
  const expires = Date.now() + Number(response.expires_in || 3600) * 1000;
  try {
    localStorage.setItem(TOKEN_KEY, JSON.stringify({ token: response.access_token, expires }));
  } catch {
    // Private browsing: the token lasts while the page is open.
  }
  memory = { token: response.access_token, expires };
  return response.access_token;
}

/** If storage is unavailable, the token is kept here instead. */
let memory: Stored | null = null;

/** The current access token, if there's one that hasn't run out. */
export function currentGoogleToken(): string | null {
  return stored()?.token ?? (memory && memory.expires - MARGIN_MS > Date.now() ? memory.token : null);
}

/** Forget the token (Google refused it): the next sync needs a tap to sign in again. */
export function forgetGoogleToken() {
  memory = null;
  try {
    localStorage.removeItem(TOKEN_KEY);
  } catch {
    // nothing kept
  }
}

/** Forget the token, and tell Google to withdraw it. */
export async function signOutOfGoogle() {
  const token = currentGoogleToken();
  memory = null;
  try {
    localStorage.removeItem(TOKEN_KEY);
  } catch {
    // nothing kept
  }
  if (token) {
    try {
      (await loadGoogle()).accounts.oauth2.revoke(token);
    } catch {
      // offline: the token runs out within the hour anyway
    }
  }
}
