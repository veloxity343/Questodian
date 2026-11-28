// The sync folder in Google Drive: the app's hidden folder (appDataFolder), which only
// Questodian can see. Files are only ever added there, never changed (see crates/core/src/sync.rs).

import type { Folder } from "./folder";

const DRIVE = "https://www.googleapis.com/drive/v3/";
const UPLOAD = "https://www.googleapis.com/upload/drive/v3/";
/** Google's limit for one-request uploads; larger files go up in two steps. */
const SIMPLE_UPLOAD_MAX = 5 * 1024 * 1024;

/** Google Drive's answer wasn't OK: its status, and a message to show. */
export class DriveError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

async function check(res: Response, what: string): Promise<Response> {
  if (res.ok) return res;
  let detail = "";
  try {
    detail = ((await res.json()) as { error?: { message?: string } }).error?.message || "";
  } catch {
    // not JSON
  }
  const message =
    res.status === 401 ? "Google sign-in has run out" : res.status === 403 || res.status === 429 ? "Google Drive is busy: trying again later" : `${what}: ${res.status} ${detail}`;
  throw new DriveError(res.status, message);
}

export function googleDrive(token: () => string): Folder {
  const send = (url: string, init: RequestInit = {}) => {
    const headers = new Headers(init.headers);
    headers.set("Authorization", `Bearer ${token()}`);
    return fetch(url, { ...init, headers });
  };
  const ids = new Map<string, string>();

  return {
    async list() {
      ids.clear();
      let page = "";
      do {
        const url = `${DRIVE}files?spaces=appDataFolder&pageSize=1000&fields=nextPageToken,files(id,name)${page ? `&pageToken=${encodeURIComponent(page)}` : ""}`;
        const body = (await (await check(await send(url), "listing the Drive folder")).json()) as {
          nextPageToken?: string;
          files: { id: string; name: string }[];
        };
        // A name twice (an upload whose answer was lost, then sent again) is the same file.
        for (const f of body.files) if (!ids.has(f.name)) ids.set(f.name, f.id);
        page = body.nextPageToken || "";
      } while (page);
      return [...ids.keys()];
    },

    async get(name) {
      const id = ids.get(name);
      if (!id) throw new Error(`${name} isn't in the Drive folder`);
      return (await check(await send(`${DRIVE}files/${id}?alt=media`), `fetching ${name}`)).arrayBuffer();
    },

    async put(name, body) {
      const type = body.type || "application/octet-stream";
      const metadata = JSON.stringify({ name, parents: ["appDataFolder"] });
      if (body.size <= SIMPLE_UPLOAD_MAX) {
        const boundary = `questodian${crypto.randomUUID()}`;
        const parts = new Blob([
          `--${boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n${metadata}\r\n--${boundary}\r\nContent-Type: ${type}\r\n\r\n`,
          body,
          `\r\n--${boundary}--`,
        ]);
        const res = await send(`${UPLOAD}files?uploadType=multipart&fields=id`, {
          method: "POST",
          headers: { "Content-Type": `multipart/related; boundary=${boundary}` },
          body: parts,
        });
        ids.set(name, ((await (await check(res, `sending ${name}`)).json()) as { id: string }).id);
        return;
      }
      // Larger: ask for an upload address, then send the bytes there.
      const start = await check(
        await send(`${UPLOAD}files?uploadType=resumable&fields=id`, {
          method: "POST",
          headers: { "Content-Type": "application/json; charset=UTF-8", "X-Upload-Content-Type": type },
          body: metadata,
        }),
        `starting to send ${name}`,
      );
      const address = start.headers.get("Location");
      if (!address) throw new Error(`Google Drive gave no address to send ${name} to`);
      const res = await send(address, { method: "PUT", headers: { "Content-Type": type }, body });
      ids.set(name, ((await (await check(res, `sending ${name}`)).json()) as { id: string }).id);
    },
  };
}
