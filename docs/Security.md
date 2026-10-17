# Security

Status key: **built** = in the code and covered by a test; **built, untested** = in the code, no automated proof yet;
**planned** = not done.

## Trust boundaries
| boundary | rule |
|---|---|
| UI → Rust | The webview can call only the commands in `docs/ui-contract.md`. It has no shell, file, process or network capability of its own (capability file grants only `core:default`, set-theme, set-title). **built** |
| Cloud AI → Rust | Cloud output is **data**. It must parse into the closed `ExecutionPlan` schema; there is no shell action; anything else is rejected before any step runs. **built, tested** (`unreadable_or_unsafe_plans_are_rejected_and_nothing_runs`) |
| Rust → OS | Only `Services` methods run. Each step passes `Policy::check` first. **built, tested** |
| PC → Cloud AI | Only the context the owner allowed, redacted, with a visible ledger (`context_shared`). Local only mode sends nothing. **built, tested** (`local_only_mode_never_reaches_the_cloud`, `sharing_switches_control_exactly_what_leaves_the_pc`) |

## Controls
- **Closed action set, validated arguments.** Paths must be absolute and inside an allowed folder, outside protected
  folders (Windows, Program Files, ProgramData, AppData, `.ssh`, `.aws`, `.gnupg`, and the macOS equivalents). Links must be
  http, https or mailto. App names must be plain names (resolved through Start Menu shortcuts, PATH, or Get-StartApps with the
  name passed in an environment variable, never interpolated into a command line). **built, tested**
- **Confirmations.** Default level is *Destructive*: move, overwrite and Recycle Bin actions wait for an explicit yes
  (120 s timeout counts as no); *Always* asks for every state-changing step; *Off* is available but not the default.
  Declining ends the run. **built, tested**
- **Prompt-injection defences.** The planner prompt says file names, window titles and control text are data; cloud
  providers are run plan-only (no tools, no MCP, no file access); plan size and step count are capped (40 steps); the
  plan cannot widen its own permissions; a replan after a failure goes through the same schema and policy. This lowers the risk; it does not remove it. A malicious window title can still
  *influence* a plan, which is why the permission policy and confirmations sit after the planner. **built; adversarial
  corpus testing planned**
- **Redaction.** Email addresses, long digit runs and token-like strings are masked in anything sent to a cloud
  provider. Pattern-based, so not exhaustive. **built, tested**
- **No credential handling.** Relay never reads, copies or stores Cloud AI login material; it runs the unmodified CLIs,
  which hold their own sign-in. Settings contain no secrets. **by design**
- **Kill switch.** `Ctrl+Alt+Shift+Esc`, the Stop button and the tray item cancel the planner, the step and the provider
  process tree. **built, tested** (unit/integration); real hotkey press is a desktop-only manual check.
- **Local servers.** The local AI engine listens on 127.0.0.1 only; the agent-mode tool server binds 127.0.0.1, needs a
  per-run bearer token, and rejects requests with an `Origin` header. **built, tested**
- **Webview hardening.** Production CSP is `default-src 'self'` with no remote origins and no inline scripts; the dev CSP
  (hot reload websocket) is applied only in `tauri dev`. `withGlobalTauri` is off. **built; CSP is exercised in the
  desktop smoke run, not by an automated header test yet**
- **Audit log.** Tool calls and workflow records (what ran, what was shared, what was approved) are stored locally.
  **built**
- **Test hooks stay out of production.** Remote-debugging and WebDriver hooks exist only behind a cargo feature
  (`e2e`) and an environment variable; the release build is made without it. **planned with the E2E layer**

## Known limits (honest list)
- A cloud model can return a *plausible but wrong* plan. Verification checks catch some of that; they cannot catch a
  plan that does exactly what was asked but not what the owner meant. Confirmations are the backstop for destructive work.
- UI Automation actions (`ui_click`, `ui_set_text`, `type_text`) act on whatever window is in front. A window that appears
  between planning and execution can receive input. Verification checks and confirmations reduce this; nothing eliminates the race.
- Redaction is heuristic. Do not enable *file contents* or *screenshots* sharing for sensitive material.
- Supply chain: Rust dependencies are audited with `cargo audit` and JavaScript with `pnpm audit` in CI (advisory only
  until a clean baseline is recorded). Lockfiles are committed. The Ollama engine download is SHA-256 verified against
  the release checksum file; a missing checksum refuses the install.
- The self-installer needs administrator rights to write to `C:\Program Files`; elevation is requested through the OS
  prompt and the elevated worker accepts a fixed set of operations only.
- macOS: the adapter is new and has not run on a Mac. Permission prompts (Accessibility, Screen Recording) are an OS
  matter and cannot be granted by Relay.

## Reporting
Security issues: open a private advisory on the repository. Do not include credentials or personal data in reports.
