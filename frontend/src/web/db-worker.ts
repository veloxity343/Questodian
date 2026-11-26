// Questodian's core in a worker (see boot.ts): opens the database, then answers API requests.

import init, { open, request } from "questodian-web";

let ready: Promise<string> | null = null;

type Message =
  | { id: number; type: "open" }
  | { id: number; type: "request"; method: string; url: string; headers: [string, string][]; body?: ArrayBuffer };

self.onmessage = async ({ data }: MessageEvent<Message>) => {
  try {
    if (data.type === "open") {
      ready ??= init().then(() => open());
      await ready;
      self.postMessage({ id: data.id, ok: true });
      return;
    }
    const reply = await request(data.method, data.url, JSON.stringify(data.headers || []), data.body ? new Uint8Array(data.body) : undefined);
    const body = reply.body;
    const message = { id: data.id, ok: true, status: reply.status, headers: JSON.parse(reply.headers), body };
    reply.free();
    self.postMessage(message, { transfer: [body.buffer] });
  } catch (e) {
    self.postMessage({ id: data.id, ok: false, error: e instanceof Error ? e.message : String(e) });
  }
};
