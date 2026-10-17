# Architecture

Relay is a local-first desktop operator for Windows 11 (macOS support in progress). The owner types a task;
Relay decides who should think about it (a deterministic shortcut, the local model, or a cloud CLI), turns the
answer into a **plan of typed actions**, validates the plan in Rust, runs it through native OS services, and
then **inspects the machine to check the result** before telling the owner what happened.

```
 React + TypeScript (Tauri webview)          no shell, no files, no network of its own
        │  typed commands + one event stream (docs/ui-contract.md)
        ▼
 src-tauri  (Rust)  commands · settings · tray · hotkey · self-installer
        │
        ▼
 relay-agent::Hybrid  ── route ──► Native shortcut     (open notepad: no AI at all)
        │                         Local plan            (Ollama, structured JSON)
        │                         Cloud plan            (Claude Code CLI or Codex CLI, plan-only)
        │                         Unavailable           (e.g. Local only with no local model)
        ▼
 relay-orchestrator   plan.rs validate · permissions.rs policy · executor.rs run/verify/retry
        │                         context.rs scope + redact what may leave the PC
        ▼
 Services trait ──► SystemServices ──► relay-os (Desktop trait: Windows today, macOS adapter in progress)
        ▼
 Local inspection (windows, files, UI Automation) = verification
```

## Crates
| crate | responsibility |
|---|---|
| `relay-protocol` | serde types shared by backend and UI: `UiEvent`, confirm cards, route/plan/step/context events |
| `relay-os` | `Desktop` trait: capture, input, windows, UI Automation; Windows impl; pure scaling and key parsing; `FakeDesktop` |
| `relay-ai` | `AiProvider` trait, child-process runner (stdin prompt, streaming, timeouts, tree kill), `claude/` and `codex/` adapters, `fake_cli` test binary |
| `relay-mcp` | tool catalogue and safety gate, loopback MCP server with per-run bearer tokens, audit log (used by agent mode) |
| `relay-orchestrator` | closed action set, plan schema and validation, permission policy, router, scoped/redacted cloud context, executor with verification, `SystemServices` |
| `relay-agent` | `Hybrid` engine (the default), legacy `Agent` (agent mode), local model client and engine manager, confirmation bridge |
| `relay-store` | SQLite: chats, projects, search, AI context, workflow history |
| `ui/` | React 19, TypeScript, Vite, Tailwind, Radix/shadcn |
| `src-tauri` | Tauri 2 shell: window, tray, global stop hotkey, commands, installer |

## The hybrid pipeline
1. **Route** (`router::route`): privacy mode, what is installed and signed in, task sensitivity and weight pick one
   strategy. Trivial requests (`open notepad`) take the **Native** path with no AI. The reason is shown in the timeline.
2. **Plan**: the local model (Ollama, JSON-schema constrained) or a cloud provider returns an `ExecutionPlan`.
   Cloud providers run **plan-only**: no tools, no MCP, no file access, structured output schema supplied.
3. **Validate** (`plan::parse_plan` + `plan::validate`): only the closed action set exists (open_app, open_url,
   focus_window, find_files, list_dir, read_text_file, create_dir, move_path, write_text_file, move_to_trash, ui_click,
   ui_set_text, type_text, press_keys, wait_for). There is **no shell action**. Unknown tools and link schemes other than http, https and mailto are rejected, with one retry.
4. **Permission** (`permissions::Policy`): each step is allowed, needs a yes from the owner, or is refused (outside the
   allowed folders, inside a protected folder, tool switched off).
5. **Execute** (`executor::execute_plan`): per-step timeout, retry, cancellation; destructive steps wait for a confirmation card.
6. **Verify**: each step may carry a check (window exists/active, file exists/missing, UI element/text). The executor
   polls the real machine through the same `Services` trait. A step that ran but could not be confirmed is reported as
   **done (unverified)**, never as success.
7. **Recover**: on failure the engine asks for a corrected plan (up to two replans); a local failure can escalate to the
   cloud when privacy mode allows it.
8. **Report**: the final message separates what the AI planned from what Relay verified. A workflow record (objective,
   strategy, provider, steps, what was shared, approvals, errors) is saved for the history screen.

## Providers
Claude Code and Codex are independent adapters behind `AiProvider`; capabilities (`ProviderCaps`) tell the UI and
router what each can do (structured output, resume, streaming deltas, reasoning levels). Both are used only by driving
the unmodified CLI with the owner's own login. Relay never reads credential files and never calls a provider web API.
Status probing (`claude auth status`, `codex login status`) is cached for 30 s.

## Privacy modes
- **Local only**: nothing about the task or desktop leaves the PC; if the local model cannot do it, Relay says so.
- **Hybrid auto** (default): route by capability, complexity and sensitivity.
- **Cloud preferred**: the selected provider plans; execution is still local.
The **share policy** decides which context categories may enter a cloud prompt (window titles on by default; UI tree,
file names, file contents, screenshots off). Everything shared passes through redaction (email addresses, long numbers, token-like strings) and is logged as a `context_shared` event.

## Concurrency and cancellation
One task at a time. Stop (button, tray, or `Ctrl+Alt+Shift+Esc`) cancels the planner, the running step, pending
confirmations, and the provider process tree.

## Persistence
`%LOCALAPPDATA%\dev.relay.operator\` holds `settings.json`, `relay.db` (SQLite, WAL), `engine/` (managed local AI),
`runs/` (audit), `logs/`. Nothing here is sent anywhere.
