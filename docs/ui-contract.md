# Relay UI contract v2 (backend <-> React UI)

The UI lives in `ui/` (React + TypeScript + Vite + Tailwind, shadcn/ui, TanStack Query, Zod, React Router, Zustand).
It runs inside a Tauri v2 window. Use `@tauri-apps/api` (`invoke`, `listen`). Command arguments are camelCase
(Tauri converts to the Rust snake_case names). Commands return real JSON values and reject with a string message.
Validate every payload that crosses the boundary with Zod schemas (one `schemas.ts` per area) and fail visibly in
dev, softly in production.

## Architecture the UI represents
React UI -> Rust orchestrator -> task router -> { native OS services | Relay Local AI | cloud provider (Claude Code or
Codex) } -> permissioned native execution -> verification. AI output is a PLAN (a proposal). Only Rust executes, only
steps that pass permission checks run, and a step is "verified" only when its effect was checked afterwards. The UI must
make the difference between "AI proposed", "ran" and "verified" obvious.

## Events (`listen("relay-event")`, payload is an object with a `type` tag)
```ts
type Risk = "reversible" | "destructive";
type ErrorKind = "cli_not_installed" | "not_logged_in" | "timeout" | "cancelled" | "cli" | "internal";
type UiEvent =
  | { type: "run_started"; run_id: number; conv_id: string }
  | { type: "route"; run_id: number; strategy: "native" | "local_plan" | "cloud_plan" | "cloud_agent" | "unavailable";
      provider: string | null; reason: string }
  | { type: "engine"; run_id: number; kind: "local" | "cloud"; name: string }
  | { type: "context_shared"; run_id: number; provider: string; categories: { category: string; items: number }[]; bytes: number }
  | { type: "plan_proposed"; run_id: number; source: "native" | "local" | "claude" | "codex"; objective: string;
      steps: { id: string; title: string; tool: string; risk: "safe" | "sensitive" | "destructive" }[]; notes: string | null }
  | { type: "step_started"; run_id: number; step_id: string }
  | { type: "step_finished"; run_id: number; step_id: string; status: "verified" | "done" | "failed" | "skipped"; summary: string }
  | { type: "assistant_delta"; run_id: number; text: string }
  | { type: "assistant_message"; run_id: number; text: string }
  | { type: "tool_started"; run_id: number; tool_id: string; name: string; summary: string }    // agent mode only
  | { type: "tool_finished"; run_id: number; tool_id: string; ok: boolean; summary: string }
  | { type: "confirm"; id: number; run_id: number; title: string; detail: string; risk: Risk }
  | { type: "confirm_resolved"; id: number; approved: boolean }
  | { type: "notice"; text: string }
  | { type: "error"; run_id: number | null; kind: ErrorKind; message: string }
  | { type: "run_finished"; run_id: number; outcome: "completed" | "cancelled" | "timed_out" | "failed" }
  | { type: "status"; provider: string; ready: boolean; detail: string; busy: boolean; kill_hotkey: string;
      local_model: string | null; route_mode: string }
  | { type: "progress"; id: string; label: string; percent: number | null; done: boolean; error: string | null };
  // progress ids: "local-ai" (managed local AI setup), "installer" (setup/uninstall screens)
```
Typical run: `run_started`, `route`, optional `context_shared`, `plan_proposed`, then per step `step_started` /
`step_finished`, optional `confirm`/`confirm_resolved` in between, a final `assistant_message` (the verified outcome in
words), `run_finished`. Text deltas (agent mode) append to the open assistant message; `assistant_message` replaces it.

## Types
```ts
type ConvSummary = { id: string; title: string; project_id: string | null; pinned: boolean; archived: boolean; created_at: number; updated_at: number };
type Conversation = ConvSummary & { items: unknown[] };          // `items` is opaque: the UI owns its shape
type Project = { id: string; name: string; color: string; instructions: string; created_at: number; updated_at: number };
type ModelInfo = { name: string; size_bytes: number; vision: boolean; tools: boolean };
type Engines = {   // Relay Local AI (managed)
  route_mode: string;   // legacy, ignore
  local: { available: boolean; managed: boolean; base_url: string; models: ModelInfo[]; selected: string | null;
           active: ModelInfo | null; recommended: string; gpu: { name: string; vram_mb: number } | null };
  cloud: { name: string; installed: boolean; logged_in: boolean; detail: string; path: string };   // legacy: use get_hybrid
  setup: { state: "idle" | "running" | "ready" | "failed"; stage: string; percent: number | null; error: string | null };
};
type ProviderInfo = {
  id: "claude" | "codex"; label: string; installed: boolean; logged_in: boolean; version: string | null;
  auth_method: string | null; detail: string;
  caps: { structured_output: boolean; session_resume: boolean; mcp_tools: boolean; streaming_deltas: boolean;
          reasoning_levels: string[]; models: string[] };
  model: string | null; reasoning: string | null;       // the owner's selection (null = CLI default)
};
type Hybrid = {
  privacy_mode: "local_only" | "hybrid_auto" | "cloud_preferred";
  default_provider: "claude" | "codex";
  providers: ProviderInfo[];
  confirm_level: "off" | "destructive" | "always";
  share: { window_titles: boolean; ui_tree: boolean; file_names: boolean; file_contents: boolean; screenshots: boolean };
  allowed_roots: string[]; protected_roots: string[]; disabled_tools: string[];
  tools: { id: string; label: string; risk: "safe" | "sensitive" | "destructive" }[];   // every tool a plan may use
};
type Settings = { approval_mode: "off" | "destructive" | "always"; user_name: string; custom_instructions: string;
                  auto_setup: boolean; hotkey: string; version: string; paths: { data: string; audit: string; logs: string; mcp: string } };
type WorkflowSummary = { id: string; conv_id: string | null; objective: string; strategy: string; provider: string | null;
                         outcome: "completed" | "failed" | "cancelled" | "declined" | "denied" | "timed_out" | "unavailable";
                         steps_total: number; steps_verified: number; created_at: number; updated_at: number };
type Workflow = WorkflowSummary & { plan_source: string; steps: { id: string; title: string; tool: string; risk: string;
                         status: string; summary: string; attempts: number }[]; shared: { category: string; items: number }[];
                         shared_bytes: number; approvals: string[]; errors: string[]; notes: string | null };
type Platform = { os: "windows" | "macos" | "linux"; materials: "mica" | "vibrancy" | "none"; title_bar: "native" | "overlay"; version: string };
```

## Commands
| command | args | returns |
|---|---|---|
| `send_message` | `{ convId, text, attachments?: string[] }` | `{ run_id }` (rejects if busy/empty) |
| `stop_task` | – | void (cancels the run; hotkey and tray do the same) |
| `answer_confirm` | `{ id, approved }` | boolean |
| `get_status` | – | the `status` payload |
| `get_platform` | – | `Platform` (drives window materials, title-bar padding, shortcut labels: Ctrl vs Cmd) |
| `conv_list` / `conv_get` / `conv_create` / `conv_save` / `conv_update` / `conv_move` / `conv_delete` / `search` | as before (see below) | |
| `project_list` / `project_create` / `project_update` / `project_delete` | as before | |
| `get_hybrid` | – | `Hybrid` |
| `set_hybrid` | partial: `{ privacyMode?, defaultProvider?, confirmLevel?, share?: Partial<share>, allowedRoots?, disabledTools?, provider?: { id, model?: string \| null, reasoning?: string \| null } }` | `Hybrid` |
| `refresh_providers` | – | `ProviderInfo[]` (re-probes install and sign-in; slow, show a skeleton) |
| `workflow_list` | `{ convId?: string, limit?: number }` | `WorkflowSummary[]` newest first |
| `workflow_get` | `{ id }` | `Workflow` |
| `get_engines` / `local_ai_setup` / `pull_model` / `set_local_model` | Relay Local AI management as before | |
| `get_settings` / `set_settings` / `mcp_get` / `mcp_set` / `pick_files` / `open_link` / `open_url` | as before | |
| `installer_info` / `installer_install` / `installer_launch` / `uninstall_run` / `app_quit` | as before | |

Unchanged argument shapes (from v1): `conv_list {archived?}`, `conv_get {id}`, `conv_create {projectId?}`,
`conv_save {id, title?, items, searchText}`, `conv_update {id, title?, pinned?, archived?}`,
`conv_move {id, projectId: string|null}`, `conv_delete {id}`, `search {query}` ->
`{conversations: (ConvSummary & {snippet})[], projects: Project[]}`, `project_create {name, color, instructions}`,
`project_update {id, name?, color?, instructions?}`, `project_delete {id}`, `local_ai_setup {action: "start"|"cancel"}`,
`pull_model {model}`, `set_local_model {model}`, `set_settings {approval_mode?, user_name?, custom_instructions?, auto_setup?}`,
`mcp_set {servers}`, `open_link {kind: "data"|"audit"|"logs"|"mcp"}`, `open_url {url}`.

## Behaviour notes
- One run at a time. While `status.busy`, `send_message` rejects.
- Privacy modes: "local_only" never sends task context to a cloud provider; "hybrid_auto" routes by capability,
  complexity and sensitivity; "cloud_preferred" favours the selected provider for reasoning (execution still local).
  `share` decides which categories of local context may be put in a cloud prompt; the `context_shared` event is the
  privacy ledger for each request and must be visible in the task timeline.
- Confirmations (`confirm` events) are blocking dialogs with a countdown (120 s). Denied or timed out means not executed.
- A plan step whose tool is disabled, or whose path is outside `allowed_roots`, is refused by Rust and finishes as
  `failed` with a "Refused: ..." summary.
- The two cloud providers are NOT identical: use `caps` to show/hide controls (for example Codex has no token streaming, so
  show a working indicator rather than streaming text; reasoning levels differ).
- The UI never executes anything itself and never receives a shell: only these commands exist.

## Setup and uninstall screens (the downloaded Setup.exe is the app itself)
Route by `installer_info().mode`: `"app"`, `"install"`, `"uninstall"` before rendering anything else. Fixed 560x620 window
with the native title bar, same theme as the app. Install screen: welcome, install location (C:\Program Files\Relay,
Windows will ask for permission), switches (desktop shortcut, start with Windows), Install, progress (`progress` id
`installer`), done with "Open Relay". Uninstall: confirm, switch "Also delete my chats, settings and the downloaded local
AI (X GB)", progress, done. Mock them in dev via `?mode=install` / `?mode=uninstall`.

## v3 additions (2026-10-17): tags, chat answers, web tasks
These change existing shapes only by adding optional fields and new event types; the UI must tolerate their absence.

```ts
type Via = "native" | "mcp";                       // how a step runs: Relay's own tools, or an MCP server (e.g. the browser)
type StepInfo = { id: string; title: string; tool: string; risk: "safe" | "sensitive" | "destructive";
                  via?: Via; server?: string;      // server = MCP server name, e.g. "playwright"
                  sensitive_data?: boolean };      // true ONLY when the step touches real sensitive info (passwords, ID/bank/card numbers, key files, ...)
type UsedKind = "local_ai" | "cloud_ai" | "native" | "mcp";
type NewEvents =
  | { type: "used"; run_id: number; kind: UsedKind; label: string; detail?: string }   // emitted once per distinct label per run
  | { type: "step_added"; run_id: number; step: StepInfo }                              // web tasks grow step by step
  // route gains: intent
  // { type: "route"; ...; intent?: "chat" | "action" | "web" }
```
- **`used`** drives the tags on a task: `local_ai` (label = model, e.g. "Qwen3 14B"), `cloud_ai` (label = "Claude Code" / "Codex"),
  `native` (label = "Windows"), `mcp` (label = server, e.g. "Playwright browser"). Show them as small coloured chips in the task
  header and on the message they produced: Local AI = green, Cloud AI = blue, Native tools = neutral, MCP = violet. Each step row also
  shows its `via` tag. Chips must be understandable without hover, and not colour-only (icon + text).
- **Chat answers** (`route.intent === "chat"`): there is no plan. The reply arrives as `assistant_delta`s then one `assistant_message`
  (markdown). Render it as a normal assistant message, not inside a task card; show only the engine chip(s). A question must never
  look like a task that "did" something.
- **Web tasks** (`route.intent === "web"`): `plan_proposed` arrives with an empty or short `steps` list, then `step_added` + `step_started`
  / `step_finished` for each browser action as the loop proceeds (can be 5 to 30 steps). Collapse long step lists (show the latest
  step and a count). A run may end with `assistant_message` asking the user to do something (e.g. "sign in to Dice in the Relay
  browser window, then say continue") and `run_finished: completed`.
- **Sensitive**: the word and badge "Sensitive" appears only when `sensitive_data` is true (red/amber, shield icon, text "Sensitive info").
  A `risk` of "sensitive" alone is NOT shown to the user; "destructive" shows as "Changes or deletes data"; confirm cards for it say so.
- **Browser settings**: `get_hybrid` gains `browser: { enabled: boolean; available: boolean; server: string; detail: string }` and
  `set_hybrid` accepts `browserEnabled?: boolean`. Show it in AI & Privacy ("Browser control (Playwright)", with detail text when
  unavailable, e.g. "Node.js was not found"). `mcp_get` / `mcp_set` are unchanged.
- **Code in answers**: fenced code blocks need a language label, a copy button ("Copy" → "Copied"), horizontal scroll (never wrap),
  syntax highlighting, and an accessible name. Inline code stays inline. Long answers stream.

## v4 additions (2026-10-17): appearance, browser mode, system check
Additive like v3: older backends without these commands or fields must not break the UI.

| command | args | returns |
|---|---|---|
| `get_appearance` | – | `Appearance` (an object; `{}` until something was saved) |
| `set_appearance` | `{ patch: object }` | the merged `Appearance`. Keys in `patch` replace stored ones; a `null` value removes the key; at most 64 keys (rejects otherwise) |
| `system_check` | – | `SystemCheck` (slow: probes the CLIs, about 1-3 s; show skeletons) |

```ts
// Stored by the backend without interpretation (settings.json); the UI validates each key on read and falls
// back to the default per key (ui/src/lib/theme.ts normalizeAppearance). Keys the UI writes today:
type Appearance = {
  mode?: "system" | "light" | "dark"; theme?: "relay" | "graphite" | "midnight";
  accent?: "terracotta" | "sage" | "indigo" | "plum" | "ink"; prose?: "sans" | "serif";
  custom?: { light?: { bg?: string; fg?: string }; dark?: { bg?: string; fg?: string } };
  codeThemeLight?: string;   // id from ui/src/lib/codeThemes.ts, e.g. "relay-light", "github-light", "solarized-light"
  codeThemeDark?: string;    // e.g. "relay-dark", "dracula", "tokyo-night"
  codeFont?: string;         // monospace font family name; "" = bundled Geist Mono
  uiFont?: "inter" | "system" | "atkinson";
  textSize?: "small" | "medium" | "large";
  density?: "comfortable" | "compact";
  taskView?: "simple" | "detailed" | "verbose";
  sidebarWidth?: number;     // px, 220..420 (default 280)
};
type SystemCheck = {
  items: { id: "local_ai" | "claude" | "codex" | "node" | "browser" | string; label: string;
           status: "ok" | "warn" | "missing"; detail: string;
           action: null | { kind: "url" | "command"; value: string; label: string } }[];
  ok: number; total: number;
};
// get_hybrid().browser gains:
//   mode?: "yours" | "separate" | "none"   // your own browser via the Playwright extension / a browser Relay keeps for itself / unusable
//   action_url?: string | null             // a page that fixes or improves the setup (extension store page, nodejs.org)
```
- **Appearance** applies instantly (attributes and CSS variables on `<html>`). localStorage `relay.appearance` is only a
  first-paint cache; at start-up the backend copy wins, keys missing there are uploaded once, and every later change is sent
  as a debounced patch of the changed keys. If the backend fails the local cache still applies and the UI says it couldn't save.
- **Browser setting**: AI & Privacy shows `detail`, the active mode ("Uses your browser" / "Separate Relay browser") and,
  when `action_url` is set, a button ("Get the Playwright extension", "Get Node.js") that calls `open_url {url}`.
- **System check**: Settings › System check lists every item with an icon and the status in words, the detail and the action
  (`url` -> `open_url {url: value}`; `command` with value `local_ai_setup` -> `local_ai_setup {action: "start"}`; unknown
  commands are not offered), plus "Check again". Home shows a dismissible banner ("2 things need attention · Review") when
  `local_ai`, `node` or `browser` is not ok; dismissal is remembered until the set of problems changes.
- Mock scenarios: `needs-setup` (Node.js and the extension missing), `browser-separate`, `no-node`.
