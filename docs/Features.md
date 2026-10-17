# Features

Status: **works** = exercised on this PC; **built** = implemented and unit/integration tested, not seen in a real run;
**unverified** = written but never run; **planned** = not started.

## Chat and tasks
- Chats, projects with instructions, search, pin/archive/rename, long-chat virtualisation: **works** (real app run for the
  shell and tasks; persistence covered by `relay-store` tests).
- Task timeline: route badge with the reason, "Shared with ..." ledger, "Proposed by ..." plan with step states, confirmation
  card with countdown, verdict, details log: **works** (native and Claude Code tasks in the real app).
- Task history: stored per workflow (objective, strategy, provider, steps, what was shared, approvals, errors): **built**.

## Hybrid AI
- Modes Local only / Hybrid auto / Cloud preferred: **works** (switching persisted; Local only refuses honestly without a model).
- Providers: Claude Code **works** (real plan-only run); Codex **built** (CLI not installed here, scripted CLI tests only).
  Default provider, per-provider model and reasoning, availability and sign-in status, fallback to the other provider: **built**.
- Native shortcut path (no AI) for trivial requests: **works**. Local model planner (managed Ollama): **built**, not run
  with a model on this PC in this round. Replan and escalate on failure: **built**.
- Verified outcomes: file/window/UI checks after steps; unverifiable steps reported as "ran but not verified": **works**.
- Not done: usage/cost display (not exposed by the CLIs in plan-only mode), retry-one-step, token streaming for Codex
  (the CLI emits whole items).

## Permissions and privacy
- Allowed and protected folders, per-tool switches, confirmation level (default: ask before destructive steps): **works**.
- Share policy per category (window titles on by default; UI tree, file names, file contents, screenshots off) with
  redaction: **built**; the cloud prompt in the live run carried no desktop context when sharing was off.
- Emergency stop `Ctrl+Alt+Shift+Esc`, Stop button, tray item: **built**; the real hotkey press was not exercised.

## Relay as an MCP server
- `Relay.exe --mcp` / `relay-mcp.exe`, verified actions, optional screen control, native approval dialog: **works** with
  real Claude Code. Claude Code plugin: **built** (validates). Codex, Cursor: **unverified**. ChatGPT: not possible
  without a public tunnel (see `docs/MCP.md`).

## Shell and platform
- Tray, single instance, hide-to-tray, self-installing Setup that installs to `C:\Program Files\Relay` with one UAC prompt:
  Setup window renders in real WebView2 (**works**); the elevated install itself: **unverified** in this round.
- Windows 11 Mica and macOS vibrancy: **planned** (the UI is ready for translucent surfaces; the window is opaque and
  `get_platform` reports `materials: none` honestly).
- macOS: desktop adapter compiles for both Apple targets (**unverified** on hardware); menus, notifications, dmg: **planned**.
- Local AI auto-install with SHA-256 verification: **built** (tested against local fake servers; the multi-GB download was
  not run).
