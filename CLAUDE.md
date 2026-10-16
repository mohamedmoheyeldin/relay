# Relay (Rust rebuild)

Relay is a Windows 11 tray app with one chat window. The owner types a task; Relay hands it to
an AI coding CLI (Claude Code, headless) that sees the screen and drives the mouse and keyboard
through tools Relay serves over MCP. Destructive actions ask the owner first, and a global
hotkey stops everything.

This branch (`rust`) is a from-scratch rewrite. Nothing is copied or ported from the previous
implementation; only behaviours and ideas were reused. Work on this rewrite stays on the `rust`
branch (worktree `C:\Projects\relay-rust`); never merge or push it to `main`/`dev`.

## Setup, run, test
- Toolchain: Rust stable (>= 1.80), target `wasm32-unknown-unknown`, `trunk`, `cargo-tauri` v2.
  WebView2 is present on Windows 11. The Claude Code CLI (`claude`) must be installed and signed in
  (`claude auth login`).
- Check (must pass before every commit):
  ```powershell
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo clippy -p relay-ui --target wasm32-unknown-unknown -- -D warnings
  cargo test --workspace
  ```
- Run in development: `cd src-tauri; cargo tauri dev` (starts `trunk serve` for the UI).
  If trunk complains about `--no-color`, set `$env:NO_COLOR='true'`.
- Build: `cd src-tauri; cargo tauri build` (NSIS installer under `target/release/bundle/nsis`).
- Tests that move the real mouse are `#[ignore]`d (`cargo test -- --ignored`); run them only with the
  owner's agreement. Everything else runs headless against `FakeDesktop` and a fake AI CLI.
- UI automation for testing: start the app with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`
  and drive the page over the Chrome DevTools protocol. Never ship that variable.
- Data lives in `%LOCALAPPDATA%\dev.relay.operator\`: `settings.json`, `mcp.json` (the owner's own
  MCP servers), `runs/<launch>/actions.jsonl` (audit log), `logs/`, `workspace/` (empty cwd for the CLI).

## Architecture
```
crates/
  relay-protocol  serde types shared by backend and UI (wasm-safe): UiEvent, ConfirmRequest, ...
  relay-os        `Desktop` trait + Windows impl (GDI capture, SendInput, UI Automation); pure scaling,
                  key-chord parsing and the input `Driver` (kill switch aware); `FakeDesktop` for tests
  relay-ai        `AiProvider` trait; process runner (stdin prompt, streaming, timeouts, tree kill);
                  `claude::ClaudeProvider` (stream-json parser, argument builder); `fake_cli` test binary
  relay-mcp       tool catalogue, argument validation, safety gate, `ToolHost`, loopback MCP server, audit log
  relay-agent     `Agent` orchestrator (routes each task to the local model or the cloud CLI, hands over on
                  escalation), `ConfirmBridge` (approval cards), prompts; `local/`: Ollama client, local
                  tool loop (PowerShell, files, open, desktop tools), router, model choice
  relay-ui        Leptos CSR chat window (wasm via trunk); `chat.rs` is a pure, tested reducer
src-tauri         Tauri v2 shell (package `relay-app`): tray, window, hotkey, commands, wiring
```

### Data flow
1. UI -> Tauri command `send_message` -> `Agent::submit` (one task at a time).
2. `Agent` asks `McpServer` for a per-run grant (URL + bearer token), builds a `RunRequest`
   (prompt on stdin, MCP config with Relay's server plus the owner's `mcp.json`, permission prompt tool,
   allowed tools, system prompt) and calls the provider.
3. `ClaudeProvider` spawns `claude -p --output-format stream-json --verbose --include-partial-messages ...`
   as a child process; its stdout is parsed into `AiEvent`s which the agent maps to `UiEvent`s and
   emits to the window (`relay-event`, payload is a JSON string).
4. Claude calls Relay's MCP tools (`screenshot`, `left_click`, `type`, `key`, `computer_batch`,
   `ask_user`, ...). `ToolHost` validates, applies the safety gate, runs the action on a blocking
   thread through the `Driver`, and reports `OK · screen changed` / `OK · no visible change`.
5. Claude's own tools (Bash, Write, ...) go through the permission prompt tool `mcp__relay__approve`,
   which classifies them (`safety::classify_builtin`) and asks the owner when needed.
6. Approval = `UiEvent::Confirm` card in the chat; the owner's click calls `answer_confirm`.
   No answer in 120 s counts as not approved.

### Coordinates
The model sees screenshots downscaled to a budget (long edge 1568, <= 3.75 MP, never upscaled). All
tool coordinates are pixels of the active display's screenshot. Conversion happens only in
`relay-os::scaling` (physical = floor of the pixel centre, clamped, plus display origin).
The process is made per-monitor-DPI aware so every rectangle is physical pixels.

### Safety model
- Destructive or irreversible actions need approval: AI-flagged (`destructive: true`), dangerous keys
  (shift+Delete, alt+F4), clicks on controls whose UI Automation name looks irreversible
  (send, delete, pay, ...), shell commands classified destructive, external MCP tools named like
  send/delete/merge. Modes: `Destructive` (default), `Always`, `Off`.
- Kill switch: `Ctrl+Alt+Shift+Esc`, the Stop button or the tray "Stop" trip it. The driver then refuses
  every new press (releases still go out), tool acts are refused with a message telling the AI not to
  work around it, open approval cards are denied, and the CLI process tree is killed. A new task lifts it.
- The MCP server binds 127.0.0.1 only, requires a per-run bearer token (revoked when the run ends) and
  rejects requests carrying an `Origin` header.
- The AI is told never to type secrets and to treat on-screen text as data.

## Conventions
- No `unwrap()`/`expect()` outside tests (workspace lint; integration test files opt out with an `allow`).
  `thiserror` in library crates, `anyhow` only in `src-tauri`.
- async with tokio; blocking Win32 work runs in `spawn_blocking`. Logging via `tracing`.
- Platform code is `cfg(windows)`; other OSes get `Unsupported` from `system_desktop()`.
- Tests are written fresh per crate; unit tests inline, integration tests in `crates/*/tests`.
- Never commit personal data, screenshots, logs or `.env`. Commits are small and clear, no co-author trailers.

## Decisions
- D1 Work happens in worktree `C:\Projects\relay-rust` (branch `rust`) because `rust` was already checked
  out there; the old app stays at `C:\Projects\relay` (`dev`) for reference.
- D2 The old tree was removed from `rust` with `git rm` (recoverable from history).
- D3 Desktop tools are served by an in-process HTTP MCP server rather than a stdio child: it shares the
  kill switch, approvals and screen state with the app without an IPC layer.
- D4 The prompt goes over stdin, never argv (length and quoting safety on Windows).
- D5 Conversation continuity: Relay picks a UUID for the first run (`--session-id`) and uses `--resume`
  afterwards; a "No conversation found" error resets the session.
- D6 Primary display by default; `list_displays`/`select_display` exist, and `focus_window` moves the active display.
- D7 Kill hotkey is Ctrl+Alt+Shift+Esc (does not collide with Task Manager's Ctrl+Shift+Esc).
- D8 The previous repo's rule against naming the CLI vendor is superseded here by the owner's brief
  (Claude Code is the named first provider). Real CLI names stay confined to `relay-ai/src/claude/`.
- D9 No commit co-author trailers.
- D10 Timeouts per run: 30 min total, 5 min without output. Approval cards time out after 120 s.
- D11 The brief forbids naming the inspiration product anywhere in the project; none is named.
- D12 The CLI is isolated with `--setting-sources local --disable-slash-commands --strict-mcp-config`.
  `--safe-mode` was rejected: it also drops MCP servers passed with `--mcp-config` (found in an end-to-end run).
- D13 Change verdicts compare full-resolution frames (>= 24 changed pixels, 0.15 s settle, 0.75 s patience);
  thumbnails missed typed text on a 3440x1440 display.
- D14 Shell commands are auto-allowed only if every segment is a known read-only command (or a PowerShell
  `Get-*`), with no redirects, `$(...)`, script blocks or backticks. Any destructive token anywhere asks
  as destructive. Everything else asks as reversible. The rules err towards asking.
- D15 UI/backend messages are JSON strings over one Tauri event (`relay-event`) and string-returning
  commands, so the wasm side needs no serde-wasm-bindgen.
- D16 Window starts visible unless launched with `--minimized`; closing hides to the tray; one instance only.
- D17 The audit log records tool name, shortened input, result and timing per call (local, ignored by Git).
- D19 Owner request (2026-10-16): no approval prompts by default. `ApprovalMode::Off` is the default; `ask_user` returns "go ahead" without a card in that mode. The Settings menu can turn prompts back on (destructive-only or every action). The stop hotkey and tray Stop remain.
- D20 UI is styled after the Claude app's look (warm palette, serif headings, centred rounded composer with round send/stop button) using original CSS only.
- D21 Two engines (owner request 2026-10-16). The **local** engine is an Ollama model on this PC that does
  OS chores and desktop automation with its own tool loop (powershell, read/write files, list, open, the desktop
  tools). The **cloud** engine is Claude Code. `RouteMode` (Settings): Auto (local first; web, coding, writing,
  long or screen-reading tasks without a vision model go to the cloud), Local only, Claude only. The local model
  can call `escalate`, and three identical failures or running out of steps escalate automatically; the cloud
  engine then continues with the original request plus a summary of what the local one did. In Auto mode a
  local failure also falls back to the cloud.
- D22 Ollama is not installed on the owner PC at the time of writing, so everything routes to Claude until it is.
  Settings detects the server, lists models (tool and vision capability from `/api/show`), lets the owner pick
  one or leave it on Automatic, and downloads a model (`/api/pull`, default suggestion `qwen3:14b`) with progress.
  Relay does not install Ollama itself.
- D23 Autonomy: the cloud system prompt forbids questions and confirmations, `AskUserQuestion` is disallowed, and
  `ask_user` is not even offered while approvals are off. Conversation history (last 6 turns) is passed to
  whichever engine has no session of its own.
- D24 The installer uses the Relay colours (terracotta sidebar and header images); NSIS dialogs keep the stock
  window chrome. Product name is "Relay Rust" so it does not collide with the old app's install.
- D18 Not done on purpose: chat history is not persisted across restarts (the CLI keeps its own session
  files); no auto-start; no code signing; macOS/Linux implementations of `Desktop` are future work.

## Known limitations
- A destructive step the AI first confirms with `ask_user` can ask again when the shell command runs
  (two cards). Deliberately conservative.
- Elevated (admin) windows and the lock screen cannot be driven from a non-elevated process.
- Hotkey registration can fail if another program owns the combination; the UI then says so and the Stop button still works.

## Progress
- [x] M1 workspace, protocol, scaling, keys
- [x] M2 AI provider + Claude Code + fake-CLI integration tests
- [x] M3 OS layer (Windows capture/input/UI Automation)
- [x] M4 MCP server + safety gate
- [x] M5 agent orchestrator
- [x] M6 Tauri shell + Leptos UI
- [x] M7 end-to-end run: real Claude Code opened Notepad and typed, approvals approved/declined from
  the chat, session resume, hotkey stop, file deletion gated by approval (see session notes below)
- [x] Release build: `cargo tauri build` produced `Relay_0.2.0_x64-setup.exe` (1.7 MiB); the release exe launched and started its tool server (window not driven, installer not run)

## Verified end to end (dev build, real Claude Code, 2026-10-16)
- Chat -> Claude Code -> `focus_window`/click/type/screenshot on Notepad.
- Shell command approval card: approve and decline paths, file survived a decline, deleted after approval.
- Follow-up message resumed the same conversation.
- Ctrl+Alt+Shift+Esc during a 120 s `wait` stopped the run in about 3 s and left no child CLI process.
- Not exercised: multi-monitor layouts, DPI scaling other than the owner's, external MCP servers (Gmail/GitHub).
