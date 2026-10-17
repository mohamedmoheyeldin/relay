# Final report (branch `new_app`, 2026-10-16)

## What exists
A Rust + Tauri + React desktop operator with a hybrid AI pipeline (native shortcut, local model, Claude Code, Codex),
validated typed plans, folder/tool permissions, confirmation of destructive steps, redacted minimal cloud context, native
execution with on-machine verification, replan/escalate, persisted workflow history, a Relay design-system UI, a
self-installing single executable, and an MCP server (`Relay.exe --mcp`) with a Claude Code plugin.

## Verified on this PC
- Rust: 343 tests pass, clippy `-D warnings` and fmt clean. UI: lint, typecheck, build, 170 unit/component tests,
  22 browser E2E and 22 visual baselines pass (mock backend).
- Real app (release build, real WebView2): Setup window and app window render; 6/6 desktop E2E checks pass (shell,
  strict CSP, backend commands, settings round trip, rejection of bad settings, Local-only refusal).
- Real Claude Code: planned and ran a task through the full pipeline; used `relay-mcp` as an MCP client (list, decline,
  screen-control read tools). Native approval dialog: Yes works, timeout refuses.

## Not verified (read this)
- Elevated install to `C:\Program Files\Relay` and uninstall were not run. An older Relay 0.1.15 is installed on this PC;
  Setup will offer to update it.
- macOS: compile-checked for Apple targets only. No Mac run, no menus/vibrancy/dmg.
- Codex CLI: not installed here; only scripted tests. ChatGPT cannot use the MCP server (needs a public HTTPS endpoint).
- Local model planning with a real model, the multi-GB engine download, the real stop hotkey, Mica, high-contrast mode,
  screen readers, WebdriverIO, GitHub Actions runs, performance budgets, adversarial prompt-injection corpus.
- Real destructive runs through the app UI (approve/decline cards) were covered by tests and the dialog check, not clicked
  through the app window.

## Known gaps / next steps
Retry-one-step command; structured refused-step and approval fields (UI currently parses prefixes); Mica/vibrancy and
`transparent` window; CI desktop job; budget checks; install test on a clean VM; macOS hardware pass; unsigned binaries
(SmartScreen warning).

## Decision needing the owner
Confirmation default is "destructive" (supersedes the earlier no-prompts request). Screen control in MCP is on by default.
