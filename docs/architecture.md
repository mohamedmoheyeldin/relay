# Relay architecture

> Status: living document. Last revised 2026-10-14 (M1 planning).
> Decisions referenced as `D-0xx` live in [decisions.md](decisions.md).

## 1. What Relay is

Relay is a "second user" for one Windows PC. It sees the screen and drives the
keyboard and mouse to finish tasks on its owner's behalf. It is the software
counterpart of the RefBox hardware box: instead of an external capture/HID
device with an 8B model on an NPU, Relay runs on the owner's own GPU and borrows
its judgment from the owner's Claude subscription through Claude Code.

**Local hands, cloud brain.**

| Local (RTX 5080, on this PC) | Cloud (Claude via Claude Code) |
|---|---|
| Perception: screenshots, UI Automation tree, OCR, local vision model | Planning tasks it has not seen before |
| Execution: click, type, keys, scroll, drag | Writing text for judgment fields |
| Following saved playbooks step by step | Recovering when a step fails or the screen is unexpected |
| Safety: kill switch, approval gates, audit log | Using MCP connectors (Gmail, Calendar, Drive, GitHub ...) |

The local model never "thinks" about what to do next. Claude never clicks
pixels directly: it calls Relay's tools, and Relay does the clicking.

## 2. Hard constraints

1. **Subscription, not API keys.** The only path to Claude is the unmodified,
   official `claude` CLI logged in with the owner's own subscription, run
   interactively or headless (`claude -p`). Relay never calls the Anthropic API,
   never reads, stores or forwards the OAuth token, and is never shipped to other
   people (D-002).
2. **Services via MCP, pixels last.** Anything with an API goes through Claude
   Code's MCP connectors. Screen automation is for apps and sites with no API.
3. **Cheapest reliable target first.** connector/API -> UI Automation -> OCR ->
   local vision model -> escalate to Claude with a screenshot (D-008).
4. **Self-contained locally.** Relay ships and manages its own Python runtime,
   model runtime and model weights. The owner should not have to install
   Ollama, Python or CUDA by hand (D-004, D-005).
5. **GPU idles when unused.** Model is loaded on demand, unloaded after ~60 s
   idle, VRAM only, no CPU offload (D-006).
6. **Humans gate the risky parts.** Logins are always done by the owner.
   Irreversible actions need approval unless allowlisted. Moving the mouse into
   the top-left corner stops everything (D-011, D-012).

## 3. Component map

```
                          ┌────────────────────────────────────────┐
  owner ── chat ────────▶ │ relay-ui  (M3: Tauri tray + chat)       │
                          │  approvals · login prompts · activity   │
                          └───────────────┬────────────────────────┘
                                          │ local IPC (WebSocket)
                          ┌───────────────▼────────────────────────┐
                          │ relay-core (M2: orchestrator + router)  │
                          │  task -> steps -> lanes                 │
                          │  service | screen | judgment            │
                          │  profile.yaml · playbook memory         │
                          └──────┬─────────────────────┬───────────┘
              playbook replay    │                     │ claude -p --output-format stream-json
              (no cloud call)    │                     ▼
                                 │       ┌──────────────────────────────┐
                                 │       │ Claude Code (official CLI,     │
                                 │       │ owner's subscription login)    │
                                 │       │  + MCP connectors (Gmail, ...) │
                                 │       └──────────────┬───────────────┘
                                 │                      │ MCP (stdio)
                          ┌──────▼──────────────────────▼───────────┐
                          │ relay-screen (M1: MCP server, Python)    │
                          │  computer toolset members + batch        │
                          │  UIA · OCR · local VLM · playbooks       │
                          │  kill switch · approval gate · audit log │
                          └──────┬──────────────┬───────────────────┘
                                 │ SendInput    │ HTTP (localhost)
                                 ▼              ▼
                          Windows desktop   llama-server (CUDA, VRAM only)
                          (3 monitors)      grounding / describe / verify model
```

M1 builds only the bottom box and proves it against interactive Claude Code.
relay-core and relay-ui sit on top later without changing relay-screen's tool
contract.

## 4. relay-screen (M1)

A stdio MCP server, registered with Claude Code as `relay-screen`. Tools show
up in Claude Code as `mcp__relay-screen__<tool>`.

### 4.1 Computer toolset members

Relay mirrors the 17 members of Claude's `computer_toolset_20260801` with the
same names and parameters, so prompts, playbooks and Claude's own habits carry
over unchanged:

`screenshot`, `zoom`, `left_click`, `right_click`, `middle_click`,
`double_click`, `triple_click`, `left_click_drag`, `mouse_move`,
`left_mouse_down`, `left_mouse_up`, `cursor_position`, `scroll`, `type`, `key`,
`hold_key`, `wait`.

Semantics copied from the API docs (see
[research/claude-integration.md](research/claude-integration.md)):

- **Coordinates are screenshot pixels** of the last full screenshot of the
  active display, origin top-left. After a `zoom`, coordinates are still in the
  full screenshot's space. Relay scales back to physical pixels and offsets into
  the virtual desktop itself (D-009).
- **Screenshot budget.** Long edge <= 1568 px by default (configurable up to
  2576 px / 3.75 MP). A 3440x1440 monitor becomes 1568x656. `zoom` captures the
  region at full resolution and fits it to the same budget.
- **Results.** `screenshot`/`zoom` return an image; other members return short
  text (`OK`, or the coordinates for `cursor_position`).
- **Batch.** MCP has no notion of the API's multi-block batch, so Relay adds a
  `batch` tool taking an ordered list of `{name, input}` actions. It runs them
  in order and stops at the first failure: succeeded actions report their
  result, the failed one reports `is_error` with a description, and every later
  one reports exactly `Not executed: an earlier computer action in this turn failed.`
  The approval gate runs before *each* action in a batch, not once per batch.

Relay-only additions:

| Tool | Purpose |
|---|---|
| `list_displays` / `select_display` | Multi-monitor: which display screenshots and coordinates refer to |
| `uia_tree`, `uia_find`, `uia_invoke`, `uia_set_value` | Accessibility tree: exact control bounds and actions without vision |
| `find_text` | Windows OCR: locate visible text, return boxes in screenshot pixels |
| `locate` | Local VLM grounding: "the Submit button" -> coordinate |
| `describe`, `verify` | Local VLM: summarize a region; check an expectation after an action |
| `playbook_list`, `playbook_load`, `playbook_save` | Markdown playbooks |
| `approve` | Approval prompt used by headless runs via `--permission-prompt-tool` |
| `request_login` | Pause and ask the owner to log in, wait until they confirm |

### 4.2 Internal modules

```
relay_screen/
  server.py          MCP server, tool registration
  dpi.py             per-monitor-v2 DPI awareness (must run before anything else)
  displays.py        monitor enumeration, virtual-desktop geometry
  scaling.py         screenshot px <-> physical px math (pure, unit tested)
  capture.py         screenshots and zoom (mss), resize/encode
  input/             SendInput mouse + keyboard, key-name parsing
  actions.py         member semantics, batch runner, halt text
  uia.py             UI Automation queries and patterns
  ocr.py             Windows.Media.Ocr
  vlm/               local model client, per-model prompt/coordinate adapters
  runtime/           llama-server lifecycle: download, start, idle-unload
  safety/            kill switch, approval policy, audit log
  playbooks.py       load/save/match playbooks
  config.py          settings (relay.toml), paths
```

### 4.3 Safety layer (runs on every action)

1. **Kill switch.** A watcher thread polls the cursor; if it sits in the
   top-left 8x8 px of the primary display, Relay aborts the current action and
   refuses further actions until re-armed. Checked again immediately before each
   SendInput call. A global hotkey (Ctrl+Alt+Pause) does the same.
2. **Approval policy.** Every action is classified `read` (screenshot, zoom,
   uia_tree, find_text, describe, cursor_position) or `act`. In M1 every `act`
   needs approval (milestone rule). From M2 only `irreversible` actions (submit,
   send, pay, delete, detected by target label and playbook annotations) need
   approval unless the app/site is allowlisted.
   - Interactive Claude Code: approvals are Claude Code's own permission prompts
     (act tools are not in the allow list, see `.claude/settings.json`).
   - Headless `claude -p`: `--permission-prompt-tool mcp__relay-screen__approve`,
     which shows a native prompt in M1 and the tray UI in M3.
3. **Audit log.** Each action appends a JSON line (time, tool, input, result,
   approval, duration) to `runs/<run-id>/actions.jsonl`, with before/after
   thumbnails. Runs are gitignored personal data.
4. **Logins.** Relay never types passwords. When a login page is detected it
   calls `request_login` and waits for the owner.

### 4.4 Local model runtime

- **Runtime:** a pinned `llama-server.exe` CUDA 13 build that Relay downloads
  into `%LOCALAPPDATA%\Relay\runtime` and supervises (D-004). Ollama is
  supported as an optional backend if the owner already runs it.
- **Models:** downloaded on first use into `%LOCALAPPDATA%\Relay\models` with
  SHA-256 checks (D-005). Default plan: one 8B GUI-grounding model at Q8_0
  (~9 GB) fully offloaded (`-ngl 99`, flash attention, q8 KV cache); the final
  pick comes from Relay's own benchmark (issue M1-11).
- **Idle unload:** the server process is stopped 60 s after the last request,
  which returns 100% of its VRAM. Cold start from NVMe is expected to be a few
  seconds and will be measured.
- **VRAM check:** before loading, NVML reports free VRAM; if a game or another
  app holds it, Relay picks a smaller quant or declines and escalates to Claude.

## 5. relay-core (M2)

Python orchestrator that turns a request into steps and gives each a lane:

| Lane | Who does it | Example |
|---|---|---|
| service | Claude Code + MCP connector | read a Google Sheet, draft a Gmail reply |
| screen | relay-screen, following a playbook if one matches | fill a web form |
| judgment | Claude Code, text only, `--json-schema` output | write "describe your experience" |

Claude Code runs headless: `claude -p --output-format stream-json --verbose
--mcp-config relay.mcp.json --strict-mcp-config --permission-mode default
--permission-prompt-tool mcp__relay-screen__approve`. relay-core parses the
stream (system/init, assistant, user tool_result, result), forwards activity to
the UI and keeps `session_id` for `--resume`.

Fact fields come from a local `profile.yaml` (name, email, phone, address);
only the fields a step needs are sent to Claude.

## 6. relay-ui (M3)

Tauri 2 tray app with one chat window, React + Untitled UI React (D-007).
relay-core and llama-server ship as Tauri sidecars. Shows the activity log,
approval prompts, login prompts and final screenshots. Phone approvals via
ntfy.sh.

## 7. Playbooks

A playbook is markdown with YAML front matter, one per app/site task:

```markdown
---
id: notepad-save-file
app: notepad.exe
match: {window_title: "Notepad"}
inputs: [text, path]
irreversible_steps: []
last_success: 2026-10-20
---
## Steps
1. `key` ctrl+n (UIA: Document "Text editor")
2. `type` {{text}}
3. `key` ctrl+s; expect dialog "Save as"
...
## Pitfalls
- Windows 11 Notepad opens tabs; ctrl+n makes a new tab, not a window.
```

Steps prefer UIA selectors and fall back to text/vision targets. A run that
follows a playbook makes no cloud call unless a step's `verify` fails, then it
escalates to Claude with the screenshot and the playbook. Playbooks are
personal data and gitignored, except `playbooks/examples/`.

## 8. Canonical flow (form example)

1. Router sees "fill the form at <site>"; no connector exists -> screen lane.
2. Open preferred browser (`profile.yaml`; the owner uses Brave Origin only, D-028)
   and navigate.
3. `verify` detects a login page -> `request_login` -> owner logs in -> continue.
4. Fact fields from `profile.yaml` typed via UIA/OCR/VLM targets.
5. Judgment fields: field label + page context + profile -> Claude text -> typed.
6. Submit is irreversible -> approval -> submit.
7. Screenshot saved as proof, playbook saved, model unloads after idle.

## 9. Known limits of a software operator (vs. RefBox hardware)

- **Secure desktop:** UAC prompts, the lock screen and Ctrl+Alt+Del cannot be
  seen or driven by a user-mode process. RefBox's "lock your screen and walk
  away" mode relies on HDMI capture + USB HID. Relay asks the owner instead.
- **Elevated windows (UIPI):** a non-elevated Relay cannot send input to admin
  windows. Relay runs non-elevated by design and asks the owner.
- **Shared mouse:** Relay and the owner use the same cursor. Relay shows a
  "Relay is driving" overlay (M3) and the kill switch always wins. Running
  tasks in a separate Windows session is an M4 research item.
- **Prompt injection:** screen text can contain instructions. Relay's tools
  return what is on screen as data; approval gates and allowlists bound the
  damage.
