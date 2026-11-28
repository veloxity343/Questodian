// A cloud folder sync can use: Google Drive now, WebDAV next. Sync only ever lists, reads and
// adds files (crates/core/src/sync.rs explains why that's enough).

export interface Folder {
  /** Every file's name. */
  list(): Promise<string[]>;
  /** A file's bytes, by a name from the last listing. */
  get(name: string): Promise<ArrayBuffer>;
  /** Add a file (its type is the blob's). */
  put(name: string, body: Blob): Promise<void>;
}
