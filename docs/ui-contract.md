# Relay UI contract (backend <-> React UI)

The UI lives in `ui/` (React + TypeScript + Vite + Tailwind). It runs inside a Tauri v2 WebView2 window
(Chromium engine, same family as the Electron ChatGPT Windows app). `withGlobalTauri` is on, so the page
may use `window.__TAURI__.core.invoke` and `window.__TAURI__.event.listen`; prefer the npm packages
`@tauri-apps/api` (`invoke`, `listen`). Command arguments are camelCase (Tauri converts to the Rust
snake_case names). All commands return real JSON values (not strings) and reject with a string message.

## Events (`listen("relay-event")`, payload is an object)
```ts
type Risk = "reversible" | "destructive";
type ErrorKind = "cli_not_installed" | "not_logged_in" | "timeout" | "cancelled" | "cli" | "internal";
type UiEvent =
  | { type: "run_started"; run_id: number; conv_id: string }
  | { type: "assistant_delta"; run_id: number; text: string }            // streaming fragment
  | { type: "assistant_message"; run_id: number; text: string }          // full text block; replaces the open streamed one
  | { type: "tool_started"; run_id: number; tool_id: string; name: string; summary: string }
  | { type: "tool_finished"; run_id: number; tool_id: string; ok: boolean; summary: string }
  | { type: "confirm"; id: number; run_id: number; title: string; detail: string; risk: Risk }
  | { type: "confirm_resolved"; id: number; approved: boolean }
  | { type: "run_finished"; run_id: number; outcome: "completed" | "cancelled" | "timed_out" | "failed" }
  | { type: "error"; run_id: number | null; kind: ErrorKind; message: string }
  | { type: "status"; provider: string; ready: boolean; detail: string; busy: boolean; kill_hotkey: string;
      local_model: string | null; route_mode: "auto" | "local_only" | "cloud_only" }
  | { type: "engine"; run_id: number; kind: "local" | "cloud"; name: string }   // which AI is working now
  | { type: "notice"; text: string }                                            // e.g. hand-over to Claude
  | { type: "progress"; id: string; label: string; percent: number | null; done: boolean; error: string | null };
  // progress ids: "local-ai" (managed local AI setup: engine download + model download)
```
Order for one run: `run_started`, optional `engine`, then deltas/messages/tools interleaved, optional `error`,
finally `run_finished`. Text deltas append to the open assistant message; `assistant_message` replaces it and
closes it; a tool starting closes any open assistant message.

## Types
```ts
type ConvSummary = { id: string; title: string; project_id: string | null; pinned: boolean;
                     archived: boolean; created_at: number; updated_at: number };   // unix ms
type Conversation = ConvSummary & { items: unknown[] };   // `items` is opaque: the UI owns its shape
type Project = { id: string; name: string; color: string; instructions: string;
                 created_at: number; updated_at: number };   // color: one of "green" "red" "blue" "yellow" "purple" "orange" "pink" "gray"
type ModelInfo = { name: string; size_bytes: number; vision: boolean; tools: boolean };
type Engines = {
  route_mode: "auto" | "local_only" | "cloud_only";
  local: { available: boolean; managed: boolean; base_url: string; models: ModelInfo[];
           selected: string | null; active: ModelInfo | null; recommended: string;
           gpu: { name: string; vram_mb: number } | null };
  cloud: { name: string; installed: boolean; logged_in: boolean; detail: string; path: string };
  setup: { state: "idle" | "running" | "ready" | "failed"; stage: string; percent: number | null; error: string | null };
};
type Settings = { approval_mode: "off" | "destructive" | "always"; user_name: string; custom_instructions: string;
                  auto_setup: boolean; hotkey: string; version: string;
                  paths: { data: string; audit: string; logs: string; mcp: string } };
```

## Commands
| command | args | returns |
|---|---|---|
| `send_message` | `{ convId: string, text: string, attachments?: string[] }` | `{ run_id: number }` (rejects with a message if busy/empty) |
| `stop_task` | – | void |
| `answer_confirm` | `{ id: number, approved: boolean }` | boolean |
| `get_status` | – | the `status` event payload |
| `conv_list` | `{ archived?: boolean }` | `ConvSummary[]` newest first |
| `conv_get` | `{ id }` | `Conversation` |
| `conv_create` | `{ projectId?: string }` | `ConvSummary` |
| `conv_save` | `{ id, title?: string, items: unknown[], searchText: string }` | void (UI calls it after the user's message is added and after each run finishes) |
| `conv_update` | `{ id, title?, pinned?, archived? }` | `ConvSummary` |
| `conv_move` | `{ id, projectId: string \| null }` (null removes from its project) | `ConvSummary` |
| `conv_delete` | `{ id }` | void |
| `search` | `{ query: string }` | `{ conversations: (ConvSummary & { snippet: string })[]; projects: Project[] }` |
| `project_list` | – | `Project[]` |
| `project_create` | `{ name, color, instructions }` | `Project` |
| `project_update` | `{ id, name?, color?, instructions? }` | `Project` |
| `project_delete` | `{ id }` | void (its chats stay, unassigned) |
| `get_engines` | – | `Engines` |
| `set_route_mode` | `{ mode }` | void |
| `set_local_model` | `{ model: string }` ("" = automatic) | void |
| `local_ai_setup` | `{ action: "start" \| "cancel" }` | void (progress via `progress` events, id `local-ai`) |
| `pull_model` | `{ model: string }` | void (progress id `local-ai`) |
| `get_settings` | – | `Settings` |
| `set_settings` | partial `{ approval_mode?, user_name?, custom_instructions?, auto_setup? }` | `Settings` |
| `mcp_get` | – | `{ path: string; servers: Record<string, unknown>; error: string \| null }` |
| `mcp_set` | `{ servers: Record<string, unknown> }` | void |
| `pick_files` | – | `string[]` (native file dialog; empty if cancelled) |
| `open_link` | `{ kind: "data" \| "audit" \| "logs" \| "mcp" }` | void |

## Behaviour notes
- One run at a time. While `status.busy`, `send_message` rejects.
- The backend keeps each conversation's AI session and recent history, and applies project instructions and
  the user's custom instructions; the UI only stores what it renders.
- Approvals default to "off" (no confirm events). When on, `confirm` events must be shown as a blocking
  dialog/card with Approve and Decline; unanswered ones time out after 120 s (`confirm_resolved`).
- The emergency-stop hotkey and tray "Stop" cancel the run; the UI just sees `run_finished` with `cancelled`.

## Setup and uninstall screens (the downloaded Setup.exe is the app itself)
Relay installs itself: the release `.exe` shows a setup window when it is not running from its install folder, and
`Relay.exe --uninstall` shows a removal window. Use the SAME visual theme as the app (dark ChatGPT-style, same fonts,
accent, buttons, progress bar). The window is a fixed 560x620 (logical px) non-resizable window with the native title bar.
Route by `installer_info().mode`: `"app"` (normal UI), `"install"`, `"uninstall"`. Check it before rendering anything else so the app
shell never flashes in installer mode. Commands:
| command | args | returns |
|---|---|---|
| `installer_info` | – | `{ mode: "app"\|"install"\|"uninstall"; version: string; install_dir: string; installed_version: string \| null; data_dir: string; data_bytes: number }` |
| `installer_install` | `{ desktopShortcut: boolean, startWithWindows: boolean }` | void; emits `progress` events with id `"installer"` (label, percent, done, error); rejects with the error text |
| `installer_launch` | – | void (starts the installed Relay and quits the setup window) |
| `uninstall_run` | `{ deleteData: boolean }` | void; progress id `"installer"` |
| `app_quit` | – | void |
Install screen: welcome (logo, "Install Relay", version, install location text, two switches: "Add a desktop shortcut", "Start Relay when I sign in"), an "Install" primary button, progress view with the live label and bar, then a done view with "Open Relay" (calls installer_launch) and a note that Relay Local AI sets itself up on first launch; if `installed_version` is set show "Update" wording. Uninstall screen: confirm view, a switch "Also delete my chats, settings and the downloaded local AI (X GB)" (format `data_bytes`; default off), "Remove Relay" destructive button, progress, done view with "Close" (app_quit). Mock these in the dev backend too (`?mode=install` / `?mode=uninstall` query for the mock).