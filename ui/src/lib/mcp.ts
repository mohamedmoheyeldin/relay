// MCP server config validation (Connections settings).

/** Validates the editor contents. Returns the parsed value or an error message. */
export function parseServerJson(text: string, wholeConfig: boolean): { ok: true; value: Record<string, unknown> } | { ok: false; error: string } {
  let v: unknown;
  try {
    v = JSON.parse(text);
  } catch (e) {
    return { ok: false, error: `Not valid JSON: ${e instanceof Error ? e.message : String(e)}` };
  }
  if (!v || typeof v !== "object" || Array.isArray(v)) return { ok: false, error: "Must be a JSON object { … }." };
  const obj = v as Record<string, unknown>;
  if (wholeConfig) {
    for (const [k, s] of Object.entries(obj)) {
      if (!s || typeof s !== "object" || Array.isArray(s)) return { ok: false, error: `"${k}" must be an object.` };
    }
    return { ok: true, value: obj };
  }
  if (typeof obj.command !== "string" && typeof obj.url !== "string") return { ok: false, error: 'A server needs a "command" (local) or a "url" (remote).' };
  return { ok: true, value: obj };
}

