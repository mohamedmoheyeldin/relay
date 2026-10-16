# Relay (Rust rebuild)

Relay is a Windows 11 tray app with one chat window. The user types a task;
Relay hands it to an AI coding CLI (Claude Code, headless) that sees the screen and
drives the mouse and keyboard through tools Relay serves over MCP.
This branch (`rust`) is a from-scratch rewrite. Nothing here is copied or ported
from the previous implementation; only behaviors and ideas were reused.

## Setup, run, test
- Toolchain: Rust stable (>= 1.80), target `wasm32-unknown-unknown`, `trunk`, `cargo-tauri` v2. WebView2 (present on Windows 11).
- Check (must pass before every commit): `cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings; cargo test --workspace`
- Run the app: `cd src-tauri; cargo tauri dev` (runs `trunk serve` for the UI). Build: `cargo tauri build`.
- Tests that touch the real desktop are `#[ignore]`d; run with `cargo test -- --ignored` only when the owner agrees (they move the mouse).
- Branch rule: all of this lives on `rust` only. Never merge or push it to `main`/`dev`.

## Architecture
```
crates/
  relay-protocol  serde types shared by backend and UI (wasm-safe): UiEvent, UiCommand, ids
  relay-os        `Desktop` trait + Windows impl: capture, input, windows, DPI; pure scaling + key parsing
  relay-ai        `AiProvider` trait; Claude Code provider; process lifecycle; stream-json parser
  relay-mcp       tool definitions + safety gate + MCP server (streamable HTTP, loopback, bearer token)
  relay-agent     session orchestrator: provider + tools + safety + events
  relay-ui        Leptos CSR chat UI (compiled to wasm by trunk)
src-tauri         Tauri v2 shell (relay-app): tray, window, commands, global kill hotkey
```
Data flow: UI --Tauri command--> `relay-agent` --> `AiProvider` spawns `claude -p` (prompt on stdin,
`--output-format stream-json --verbose --include-partial-messages`). Claude calls Relay's MCP server
(in-process, 127.0.0.1, random port + token) for desktop tools and for permission prompts
(`--permission-prompt-tool`). Provider events become `UiEvent`s, delivered to the UI via a Tauri event.
Confirmation requests are `UiEvent::Confirm`; the UI answers with `UiCommand::Answer`.

- **OS layer** (`relay-os`): `trait Desktop` (screenshot, displays, mouse, keyboard, windows). Windows impl
  uses the `windows` crate (GDI capture, `SendInput`). Everything platform-neutral (coordinate scaling,
  key-chord parsing) is pure and tested on any OS. Platform code is `cfg(windows)`; other OSes get a stub that errors `Unsupported`.
- **Coordinates**: the model sees screenshots downscaled to a budget (long edge 1568, <= 3.75 MP, never upscaled);
  all tool coordinates are screenshot pixels of the active display. Conversion only in `relay-os::scaling`.
- **AI provider** (`relay-ai`): `AiProvider::run(request) -> RunHandle` yielding a stream of `AiEvent`
  (Init, TextDelta, Text, ToolUse, ToolResult, Done, Error) and a `cancel()`; cancellation kills the whole process tree.
  Typed errors: `NotInstalled`, `NotLoggedIn`, `Timeout`, `Cancelled`, `Spawn`, `Cli`.
- **Safety** (`relay-mcp::safety`): (1) destructive desktop actions need a user confirmation card; (2) Claude's own
  non-read-only tools are gated through the permission-prompt tool; (3) a global hotkey (Ctrl+Alt+Shift+Esc)
  and the tray "Stop" trip the kill switch: child tree is killed, held inputs released, all act-tools refused until the next task.
- **MCP for external services**: user-supplied servers live in `%APPDATA%\Relay\mcp.json` (standard `mcpServers`
  map) and are merged into the `--mcp-config` passed to the CLI next to Relay's own server.

## Conventions
- No `unwrap()`/`expect()` outside tests (lint-enforced). `thiserror` in library crates, `anyhow` in `src-tauri`.
- async with tokio; blocking Win32 work goes through `spawn_blocking`. Logging via `tracing`.
- Tests are written fresh per crate; unit tests inline, integration tests in `crates/*/tests`.
- Never commit personal data, screenshots, logs, `.env`.
- Commits: small, clear messages, no co-author trailers.

## Decisions
- D1 Work in worktree `C:\Projects\relay-rust` (branch `rust`) because `rust` was already checked out there; the old app stays reachable at `C:\Projects\relay` (dev) for reference.
- D2 Old tree removed from the `rust` branch with `git rm` (recoverable from history).
- D3 Desktop tools are served by an in-process HTTP MCP server rather than a stdio child: it shares the kill switch, confirmation channel and screen state with the app, with no IPC layer. Loopback + random bearer token per run.
- D4 Prompt goes over stdin, not argv (length/quoting safety on Windows).
- D5 Conversation continuity via the CLI's `--resume <session_id>` captured from the init event.
- D6 Primary display only for v1 (multi-display selection is in the trait, exposed later).
- D7 Kill hotkey Ctrl+Alt+Shift+Esc (does not collide with Task Manager's Ctrl+Shift+Esc).
- D8 The previous repo's rule to avoid naming the CLI vendor is superseded for this rewrite by the owner's brief (Claude Code is the named first provider). Real CLI names are still confined to `relay-ai/src/claude/`.
- D9 No commit co-author trailers, per the repo's convention and the owner's global rule.
- D10 Per-turn timeouts: 30 min total, 5 min with no output; both configurable.
- D11 The brief forbids mentioning the inspiration product anywhere in the project; none is named.

## Progress
- [ ] M1 workspace, protocol, scaling, keys
- [ ] M2 AI provider + Claude Code + fake-CLI integration tests
- [ ] M3 OS layer (Windows capture/input)
- [ ] M4 MCP server + safety gate
- [ ] M5 agent orchestrator
- [ ] M6 Tauri shell + Leptos UI
- [ ] M7 end-to-end verification
