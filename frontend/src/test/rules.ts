// The bundled rules data, for tests (the app gets it from the server, with the user's edits).
import { readFileSync } from "node:fs";
import { gunzipSync } from "node:zlib";
import type { Rules } from "../lib/builder";

export const rules = JSON.parse(gunzipSync(readFileSync(new URL("../../../data/rules-2024.json.gz", import.meta.url))).toString()) as Rules;
