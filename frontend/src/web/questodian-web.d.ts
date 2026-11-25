// The core built for a browser (crates/web, `make wasm` writes it to target/web-pkg/). The web
// build points this name there (vite.config.ts); declared here so the typecheck doesn't need it.
declare module "questodian-web" {
  export default function init(): Promise<unknown>;
  /** Set up the browser storage and open the database; returns SQLite's version. */
  export function open(): Promise<string>;
  export class Reply {
    free(): void;
    readonly status: number;
    /** `[[name, value], ...]` as JSON. */
    readonly headers: string;
    readonly body: Uint8Array;
  }
  export function request(method: string, url: string, headers: string, body?: Uint8Array): Promise<Reply>;
}
