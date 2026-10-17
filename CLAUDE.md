# Relay (branch `new_app`)

Relay is a local-first desktop operator for Windows 11 (macOS in progress). The owner types a task; Relay decides who
should think about it (a built-in shortcut, a local model, or a cloud CLI: Claude Code or Codex), turns the answer into a
validated plan of typed actions, runs it natively, and checks the result on the machine before reporting. It also runs as an
MCP server so Claude Code and similar clients can use the same checked tools.

Branch `new_app` is a production rebuild branched from `rust` (worktree `C:\Projects\relay-rust`). Never modify `main`,
`dev` or `rust` from here; never merge or push to them. Commit and push `new_app` only when the owner asks. No co-author
trailers on commits unless the owner asks. Never name the inspiration product anywhere. Read `docs/Architecture.md` first.

## Setup, run, test
- Toolchain: Rust stable, Node 24 (fnm: `fnm use 24`), pnpm 10+, WebView2 (Windows 11), `cargo-tauri` v2. Claude Code CLI
  signed in for real cloud runs; Codex CLI optional.
- Checks (must pass before every commit):
  ```powershell
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  cd ui; pnpm lint; pnpm typecheck; pnpm test; pnpm build
  ```
  More UI layers: `pnpm test:e2e-browser`, `pnpm test:visual` (mock backend). See `docs/Testing-Strategy.md`.
- Build: `cd src-tauri; cargo tauri build --no-bundle` -> `target\release\relay-app.exe`. That one file is installer, app
  and MCP server: outside `C:\Program Files\Relay` it shows Setup; `--run` forces app mode; `--mcp` is the MCP server.
- Desktop E2E (real window, opens it on the owner's PC; run only when asked or during the owner's requested test cycle):
  `cargo tauri build --no-bundle -- --features e2e` then `cd ui; pnpm test:desktop`. Never ship the `e2e` feature.
- Tests that move the real mouse are `#[ignore]`d. Everything else is headless (FakeDesktop, fake AI CLI, mock services).
- Data: `%LOCALAPPDATA%\dev.relay.operator\` (`settings.json`, `relay.db`, `engine/`, `runs/`, `logs/`).

## Crates
`relay-protocol` shared event types · `relay-os` Desktop trait (Windows impl, macOS adapter compile-checked only) ·
`relay-ai` provider trait, Claude and Codex adapters, `fake_cli` · `relay-mcp` tool host, safety gate, loopback server
(agent mode) · `relay-orchestrator` plans, policy, router, context/redaction, executor, `SystemServices`, `HybridConfig` ·
`relay-agent` `Hybrid` engine (default), legacy `Agent`, local model client + engine manager · `relay-store` SQLite ·
`relay-mcp-server` stdio MCP server (`relay-mcp.exe`, also `Relay.exe --mcp`) · `src-tauri` shell · `ui/` React.
Contract between UI and backend: `docs/ui-contract.md`. Design tokens: `docs/Design-System.md`.

## Rules that must hold
- AI output is untrusted data. It must parse into the closed `ExecutionPlan`; there is no shell action. Every step passes
  `Policy::check` (allowed/protected folders, disabled tools, confirmation level) before it runs.
- Default confirmation level is Destructive (this supersedes the earlier "no prompts" request on `rust`, because the brief
  requires confirmation of destructive actions). Reading file contents also asks.
- Cloud providers run plan-only (no tools, no MCP). Only the context the share policy allows leaves the PC, redacted, with a
  `context_shared` event. Local only never contacts a cloud provider.
- Never read, copy or store Cloud AI credentials; only drive the unmodified CLIs. Never call a provider web API.
- Report outcomes honestly: a step that ran but could not be checked is "ran, not verified", never success. Anything not
  run on real hardware (macOS, Codex, the elevated install) is documented as unverified.
- Test hooks stay behind the `e2e` cargo feature. Production CSP has no remote origins and no unsafe script sources.
- No `unwrap()`/`expect()` outside tests. Windows-only code is `cfg(windows)`. pnpm only for the UI. No personal data,
  screenshots of private content, logs or `.env` in Git (`reports/` is ignored except its README).

## Decisions (append only)
- D1-D17 from the `rust` branch still hold: worktree layout, in-process loopback MCP server for agent mode, prompt via
  stdin, UUID sessions with `--resume`, kill hotkey Ctrl+Alt+Shift+Esc, CLI isolation flags (`--setting-sources local
  --disable-slash-commands --strict-mcp-config`), full-resolution change verdicts, conservative shell classification,
  JSON strings over one Tauri event, hide to tray, audit log.
- D20 UI styling moved from imitating another app to the Relay design system (`docs/Design-System.md`).
- D25 One executable is installer, app and MCP server; install target `C:\Program Files\Relay` via an elevated worker.
- D30 Hybrid engine: route -> plan (native / local / cloud) -> validate -> permission -> execute -> verify -> recover.
- D31 Cloud plans are structured JSON with a schema; no tools are given to the cloud model while planning.
- D32 Confirmation default is Destructive; sensitive reads (file contents) also ask.
- D33 Desktop E2E drives the real WebView2 over CDP; the debug switch is stripped unless built with feature `e2e`.
- D34 MCP server: verified actions plus optional screen control; native OS dialog for approvals; `resume_after_stop`
  lifts the stop only on the user's instruction. ChatGPT is not supported (needs a public HTTPS endpoint).
- D35 `HybridConfig` and `final_message` live in `relay-orchestrator` so the app and the MCP server share them.
- D36 Messages are classified first: questions and code requests are answered in words (chat path, streamed, nothing runs);
  PC actions use plan -> validate -> execute -> verify; website tasks use the browser loop. A question must never open pages.
- D37 Website tasks drive a real browser through the Playwright MCP server (`relay-mcp-client`, `npx @playwright/mcp@<pinned>`),
  one observed step at a time (snapshot -> one action -> check). Irreversible clicks (apply, submit, pay, delete, ...) ask the
  owner; password/card/ID fields are never typed; repeated identical actions stop the run; at most 30 steps per run.
- D38 Preferred browser is the owner's own (their sign-ins) via the official Playwright MCP Bridge extension (`--extension`);
  fallback is a separate Chrome/Edge profile. Brave Origin cannot be used with a fresh profile (purchase screen), so on PCs
  with only Brave Origin the extension is required. Relay never reads the browser profile beyond checking the extension folder.
- D39 Cloud defaults: website tasks prefer the cloud model in Hybrid; browser steps and plans use a mid model at low effort, written answers a
  mid model at medium effort, unless the owner picked a model. Transient CLI errors (sign-in refresh races, overload) retry once.
- D40 "Sensitive" means real sensitive information (passwords, keys, ID/card numbers, key files), detected in
  `relay-orchestrator::sensitivity`; ordinary file reads no longer ask. Tags (`used` events) are emitted only for components that
  actually produced the result.
- D41 Replans never repeat an effect that already happened in the task (same action), and one plan may open at most two pages.
