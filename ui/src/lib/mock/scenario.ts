// Scenario switches for the mock backend: `?scenario=a,b` (flags) and `?flow=name` (force a run flow).
// Used by Playwright and for trying states by hand in `pnpm dev`.
//
// Flags:
//   all-ready          both cloud providers installed and signed in
//   codex-missing      Codex is not installed
//   claude-signed-out  Claude Code is installed but not signed in
//   local-setup        Relay Local AI is not set up yet (Settings shows the setup and its progress)
//   confirm-off        confirmations off (no approval dialogs)
//   local-only | cloud-preferred   start in that privacy mode
//   empty              no saved chats, projects or task history
//   fast               short delays (tests)
//   slow               long steps (to try Stop)
//   mac | mica         platform: macOS vibrancy + overlay title bar, or Windows Mica
// Flows (?flow=): native, local, cloud, download, verify-fail, refused, declined, cloud-fail, agent,
//   not-installed, not-signed-in, chat

export type Flow =
  | "native"
  | "local"
  | "cloud"
  | "download"
  | "verify-fail"
  | "refused"
  | "declined"
  | "cloud-fail"
  | "agent"
  | "not-installed"
  | "not-signed-in"
  | "chat";

export const FLOWS: readonly Flow[] = [
  "native",
  "local",
  "cloud",
  "download",
  "verify-fail",
  "refused",
  "declined",
  "cloud-fail",
  "agent",
  "not-installed",
  "not-signed-in",
  "chat",
];

export type Scenario = { flags: Set<string>; flow: Flow | null };

export function readScenario(search: string = typeof location !== "undefined" ? location.search : ""): Scenario {
  const q = new URLSearchParams(search);
  const flags = new Set(
    (q.get("scenario") ?? "")
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean),
  );
  const f = q.get("flow");
  return { flags, flow: f && (FLOWS as readonly string[]).includes(f) ? (f as Flow) : null };
}

/** Picks the flow for a message: forced flow, then keywords in the text, then the default. */
export function flowFor(text: string, forced: Flow | null): Flow {
  if (forced) return forced;
  const t = text.toLowerCase();
  if (/\b(not installed)\b/.test(t)) return "not-installed";
  if (/\b(sign in|signed out|login|log in)\b/.test(t)) return "not-signed-in";
  if (/\bverify\b|brightness/.test(t)) return "verify-fail";
  if (/system32|\brefuse/.test(t)) return "refused";
  if (/\bdecline/.test(t)) return "declined";
  if (/offline|fallback|cloud fail/.test(t)) return "cloud-fail";
  if (/e-?mail|inbox|browse|website/.test(t)) return "agent";
  if (/notepad|calculator|^open\b/.test(t)) return "native";
  if (/download/.test(t)) return "download";
  if (/disk|space|storage/.test(t)) return "local";
  if (t.trim().length < 12) return "chat";
  return "cloud";
}
