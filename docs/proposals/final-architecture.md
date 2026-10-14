# Proposal: Relay final architecture

> Status: adopted 2026-10-14 as D-032 to D-039. Written in response to the owner's revised
> architecture brief (Rust always-on core, speed ladder, local model table,
> cross-platform). Nothing here changes `architecture.md` or `decisions.md`;
> section 9 lists the decisions the plan thread should adopt as new D-entries.

## 1. The answer in one page

**Local AI runs the hands, Claude does the thinking.** The local side is the
main brain *for executing*: it sees, finds, acts, checks and follows playbooks
on its own, with no cloud call. Claude is called when something needs thinking:
planning a task Relay has never done, writing free text, or recovering when the
screen does not match the playbook. This is D-001 unchanged.

| Area | Recommendation |
|---|---|
| Core | A native **Rust daemon (`relay-core`) that is always running**, built in M2. It **wraps** the Python `relay-screen` from M1 instead of replacing it. |
| Hands | Keep Python `relay-screen` (UIA, input, capture, OCR, safety, all verified on the owner's PC) as Relay's Windows hands through v1. Port to Rust only when macOS work starts (M4), behind the same tool contract. |
| Face | Tauri 2 + React + Untitled UI (D-007), a client of the daemon over a local WebSocket. Closing the window never stops a running task. |
| Brain (thinking) | Claude Code headless on the owner's subscription (D-002). No local planning LLM. |
| Brain (executing) | Deterministic playbook runner in `relay-core` + small local vision models for "where is it" and "did it work". |
| Local models | UI-Venus-1.5-8B (grounding) + Qwen3-VL-8B-Instruct (describe/verify/step check), both kept resident at Q4/Q5 in ~12 GB VRAM so there is no swap; unload after 60 s idle. Final quant from the M1-11 benchmark. |
| Speed ladder | Playbook replay first, then API/MCP -> shell/COM -> browser/Electron DOM via CDP -> accessibility tree -> OCR -> local vision -> Claude with a screenshot. Raw coordinates only as the output of a lower rung, never as a plan. |
| Platforms | Windows only through v1. The contract and the daemon are written so macOS can follow in M4; Linux after that, Wayland-first. |

## 2. Where the brief and the current plan disagree

| Topic | Brief says | Current plan says | Recommendation |
|---|---|---|---|
| Cloud AI | "No cloud AI", then the last line says Claude does the thinking | Local hands, cloud brain (D-001, D-002) | **Current plan.** The last line is the owner's latest intent, and a local 8B-30B model plans new tasks far worse than Claude. |
| Local "Brain (planning)" model | A row for a planning model, maybe a large MoE in system RAM | Local model never decides what to do (D-001) | **No local planner.** 32 GB RAM cannot hold a 30B MoE (~18 GB at Q4) and still leave RAM free, which is a standing rule. Planning goes to Claude; replay needs no planner. |
| Core language | Native Rust core, always running | Python relay-core (M2) on top of Python relay-screen | **Rust for the core, Python stays for the hands.** See section 3. |
| Process model | Core keeps capture/input handles open; UI talks over IPC/WebSocket | Claude Code spawns relay-screen over stdio per session | **Brief.** The daemon keeps relay-screen warm and Claude Code connects to it over localhost HTTP, removing the per-run start cost. |
| Ladder order | MCP -> shell/COM -> CDP -> a11y -> OCR+vision -> coordinates | connector -> UIA -> OCR -> VLM -> Claude (D-008), COM and DOM lanes added (D-021) | **Brief's order, plus two rungs**: playbook replay at the top, Claude-with-screenshot above raw coordinates. |
| Accessibility library | xa11y or Terminator | Python `uiautomation`, own UIA thread (D-013, D-026) | **Keep `uiautomation` on Windows.** It is built and verified. Evaluate xa11y for macOS/Linux in M4 (MIT, pre-1.0). Terminator is Windows-first with experimental macOS/Linux, so it buys nothing over what Relay already has. |
| OCR | Windows.Media.Ocr / Vision | Windows.Media.Ocr (D-029) | Same. |
| Capture/input crates | xcap/scap, enigo | mss + own SendInput (D-013) | **Keep on Windows.** Use xcap/enigo (or direct WGC/CGEvent) only in the M4 Rust hands. |
| Browser | Playwright or own extension (Violoop syncs cookies) | Dedicated profile + remote debugging (D-016, open) | **Own extension in the owner's real Brave profile**, no cookie copying. See section 4. |
| Platforms | Windows, macOS, Linux | Windows only (roadmap non-goal) | **Windows through v1**, macOS M4. Section 7. |
| Idle unload | ~60 s | 60 s (D-006) | Same. |

## 3. The stack and the Rust question

### 3.1 Replace or wrap?

**Wrap now, port the hands later, never rewrite for its own sake.**

- The speed that matters is per-step latency: UIA lookups 50-300 ms, vision
  1-3 s, a Claude turn 3-15 s. Python adds well under a millisecond to a
  SendInput call. A Rust rewrite of the hands would not make a step noticeably
  faster; avoiding vision and Claude calls does.
- The one real Python cost is **process start** (interpreter + imports, about
  1-2 s each time Claude Code spawns the stdio server). An always-on daemon
  removes it without rewriting anything.
- relay-screen is verified on the owner's three monitors (DPI, scaling,
  typing quirks, UIA threading, OCR). That knowledge lives in its tests and
  D-017..D-029; throwing it away now would re-open solved bugs.
- What Rust is good for here is what a Python orchestrator is bad at: a small,
  always-on, low-memory process that supervises children, owns sockets, and
  ships as a single signed binary next to Tauri. So the **orchestrator**
  (planned as Python relay-core) becomes Rust instead.

### 3.2 Processes

```
 Brave + Relay extension ─ native messaging ─┐
                                             │
 relay-ui (Tauri 2, React, Untitled UI) ─ WS ┤   localhost only, per-install token
                                             ▼
 ┌──────────────────────── relay-core (Rust, always on, starts at login) ────────────────────────┐
 │ task router + playbook runner + approval broker + audit log                                  │
 │ Claude driver: spawns `claude -p --output-format stream-json` (D-002)                        │
 │ supervisor: relay-screen worker, llama-server (idle unload 60 s), browser bridge             │
 │ MCP endpoint for Claude Code: streamable HTTP on 127.0.0.1 (proxied to relay-screen)         │
 └───────────────┬───────────────────────────────┬──────────────────────────────────────────────┘
                 │ MCP (kept warm)                │ HTTP localhost
                 ▼                                ▼
 relay-screen (Python, M1): UIA, OCR,       llama-server (CUDA, VRAM only)
 SendInput, capture, kill switch            UI-Venus-1.5-8B + Qwen3-VL-8B
```

- **relay-core** owns lifetime and routing. It is the only thing that starts
  at login. It never touches pixels itself in v1.
- **relay-screen** keeps its tool contract (D-009) and safety layer. The kill
  switch and per-action approval stay *inside* relay-screen, next to SendInput,
  so no path can bypass them (D-012, D-019, D-025). The only change: it also
  serves MCP over streamable HTTP on localhost so the core can keep one warm
  instance. Stdio stays for development.
- **Claude Code** is pointed at the core's localhost MCP URL in the
  `--mcp-config` Relay passes to `claude -p`, plus the owner's own connectors.
  The bearer token is Relay's own local secret, never a Claude credential.
- **llama-server** lifecycle moves from relay-screen (M1-09) to the core in
  M2, because the core is the long-lived process and the browser and playbook
  runner also need the models. M1 keeps it in Python as planned; the move is a
  small port.
- **relay-ui** is a pure client. Tauri bundles relay-core and relay-screen as
  sidecars (D-007's sidecar plan, with the core in Rust instead of PyInstaller
  for the orchestrator).

### 3.3 When

| Milestone | Change |
|---|---|
| M1 (now) | Nothing changes. Finish relay-screen in Python, including the runtime manager and benchmark. Add streamable-HTTP serving as a small M1 task or first M2 task. |
| M2 | Build `relay-core` in Rust (instead of Python): daemon, Claude stream-json driver, router, playbook runner, model supervisor, WebSocket API, browser bridge. COM lane runs in relay-screen (Python `pywin32` is the most mature COM path). |
| M3 | Tauri UI as client of the daemon; installer bundles both sidecars. |
| M4 | Rust hands for macOS (and, if it helps, Windows too) behind the same MCP contract, validated by running relay-screen's test suite as contract tests against both. |

## 4. The speed ladder

Always use the highest rung that works. Each rung reports what it did and how
long it took to the audit log, so the router learns which rung works per app.

| # | Rung | Who runs it | Typical cost | Notes |
|---|---|---|---|---|
| 0 | **Playbook replay** | relay-core runner | tens of ms per step + the rung each step uses | No cloud call. Steps store the best selector found last time (UIA path, DOM selector, COM call, keyboard shortcut), with lower rungs as fallbacks. |
| 1 | **Service API via MCP** (Gmail, GitHub, Calendar, Drive) | Claude Code connectors | ms-s, no screen | D-003. Connectors are reached only through Claude Code, so this rung always involves Claude. |
| 2 | **Shell / PowerShell / COM** (Office, Explorer, settings) | Claude Code's shell tool when Claude drives; relay-screen `com_*`/`run_*` tools with approval when replaying | 10-100 ms | Exact and fast. Commands outside an allowlist need approval. |
| 3 | **Keyboard shortcuts** | relay-screen `key` | ~20 ms | Cheapest UI action when the shortcut is known (Ctrl+S, Ctrl+L). Stored in playbooks (D-008). |
| 4 | **Browser and Electron DOM via CDP** | browser bridge in relay-core | 20-200 ms | Playwright-style ARIA snapshots and CSS/role selectors (D-021), form values redacted before anything goes to Claude. |
| 5 | **Accessibility tree** | relay-screen UIA | 50-300 ms | Exact bounds, Invoke/Value patterns, no pixels (D-017, D-026). |
| 6 | **OCR** | relay-screen Windows.Media.Ocr | 50-200 ms, CPU | Finds visible text with boxes. |
| 7 | **Local vision model** | llama-server | 0.5-3 s warm, + load time cold | `locate` (grounding), `verify`, `describe`. Canvas apps, games, remote desktops, icon-only buttons. |
| 8 | **Claude with a screenshot** | Claude Code | 3-15 s | For unknown or unexpected screens. Claude answers by calling the rungs above, not by guessing. |
| 9 | **Raw coordinates** | relay-screen | ~1 ms | Only as the output of rungs 6-8 on the current screenshot, never replayed blindly from an old playbook. |

**Browser lane (decides D-016).** Brave is the owner's browser (D-028) and they
are logged in there. Chrome 136+ ignores `--remote-debugging-port` on the
default profile (Brave follows Chromium, so we expect the same; verify on the
owner's PC), and a separate profile loses the logins. So Relay ships its **own small MV3 extension** for Brave that uses
the `chrome.debugger` API (CDP inside the browser) and talks to relay-core over
native messaging. This works in the real profile with real logins and never
copies cookies (Violoop's cookie sync is exactly what Relay should not do).
The extension is installed once by the owner (developer mode or a local CRX).
Electron apps Relay launches itself can get `--remote-debugging-port`; for
Electron apps already running, rung 5 is the fallback.

## 5. Who does what

| Task | Local (executes) | Claude (thinks) |
|---|---|---|
| A task with a matching playbook | Runs every step, checks each result, handles known detours (popups, slow loads, scrolling to find) | Not called |
| A playbook step whose check fails | Retries once by a lower rung, then stops | Gets the playbook, the step, and a screenshot; decides the fix; Relay updates the playbook |
| A new task | Supplies perception (UIA/DOM/OCR/vision) as tools | Plans, calls Relay's tools, and the successful run is saved as a playbook |
| Free-text fields ("describe your experience") | Types it | Writes it (`--json-schema` structured output) |
| Facts (name, email, address) | Fills them from `profile.yaml` | Never sees more fields than the step needs |
| Services with an API | Nothing | Uses MCP connectors |
| Irreversible actions | Detects them, asks the owner | Never approves; approvals come from the owner only (D-019) |
| Logins | Detects the login page, asks the owner, waits | Not involved; Relay never types passwords |

The local "brain for local automation" is therefore code plus perception
models, not a chat model: a playbook runner that is deterministic where it can
be, and asks Qwen3-VL "does the screen now match this step's expectation?"
where it cannot. That keeps replays fast, cheap, and inside the owner's usage
limits.

## 6. Local models on 16 GB VRAM

About 13.5 GB is free at idle (setup.md). System RAM stays free: no CPU
offload, no model in RAM.

| Role | Model | Quant / VRAM (incl. mmproj) | Why |
|---|---|---|---|
| Grounding: "where is X" (`locate`) | **UI-Venus-1.5-8B** (Apache-2.0) | Q4_K_M ~5.5 GB (Q8_0 ~9 GB) | Best published small grounding model (ScreenSpot-Pro 68.4, D-005). |
| Look and check: `verify`, `describe`, step check in replays | **Qwen3-VL-8B-Instruct** (Apache-2.0) | Q4_K_M ~5.5 GB | Best general description and yes/no checking at this size; official GGUF. |
| Both resident | the two above | ~11-12 GB with a small KV cache | **No swapping.** Swapping two 8B models costs seconds per switch, which a replay alternating `locate` and `verify` would pay on every step. |
| One-model fallback | Holo2-8B (or Holo2-4B) | Q4/Q8 | If the benchmark shows the Q4 pair losing too much accuracy, one model that does both at Q8 beats swapping. |
| Planning | none locally | n/a | Claude plans. A 30B MoE in RAM does not fit the RAM rule and plans worse. Revisit only if "offline mode" becomes a goal. |
| OCR | Windows.Media.Ocr | CPU, no GPU | Already built (D-029). |

This changes D-005's default from "Q8, one model at a time, swap on demand" to
"Q4/Q5, both resident". The M1-11 benchmark decides: if UI-Venus at Q4_K_M
scores within about 2 points of Q8_0 on Relay's own screens, take the resident
pair; otherwise use the Holo2 single-model fallback. Both servers (or one
llama-server with two models, if router mode is stable on the pinned build)
unload together after 60 s idle. If NVML reports less free VRAM than needed (a
game is running), Relay loads only the grounding model or skips straight to
Claude.

## 7. Platforms

- **Windows (v1).** Everything above. UIA, SendInput, mss/WGC,
  Windows.Media.Ocr, COM.
- **macOS (M4).** Rust hands: AX via xa11y (or direct `AXUIElement`),
  ScreenCaptureKit via xcap, CGEvent input, Vision framework OCR, AppleScript as
  rung 2. Needs Screen Recording and Accessibility permissions from the owner,
  and a Mac to test on. Claude Code, llama.cpp (Metal) and the Brave extension
  all work there already.
- **Linux (after macOS).** AT-SPI via xa11y, PipeWire portal capture,
  libei/uinput input. Wayland restricts global input and capture, so expect
  portal prompts and per-compositor gaps.
- **What makes this cheap later:** the MCP tool contract (D-009) is the
  boundary. relay-core, playbooks, the UI and Claude's prompts never see which
  OS is underneath. relay-screen's test suite becomes the contract test for
  every hands implementation.

## 8. Things software cannot match from Violoop's box

Physical approve key: replaced by the approval prompt that synthetic input
cannot click (D-019/D-025) plus phone approvals (ntfy.sh, M3). Status screen:
the tray icon and floating progress chip (M3). Lock-screen "away mode": not
possible from a user-mode process (secure desktop); stays an M4 research item
(separate Windows session).

## 9. Decisions for the plan thread to adopt

Proposed as new entries; none rewrites an old one.

1. **Rust always-on core, Python hands.** `relay-core` is a Rust daemon built in
   M2 (supersedes the Python relay-core in architecture §5). It wraps the M1
   Python `relay-screen` and keeps it warm; relay-screen stays the Windows
   hands through v1. Hands move to Rust only for macOS (M4), behind D-009.
2. **Warm MCP over localhost HTTP.** relay-screen also serves streamable-HTTP
   MCP on 127.0.0.1 with a local token; relay-core keeps one instance running
   and Claude Code connects to it. Kill switch and approvals stay inside
   relay-screen.
3. **Speed ladder** as in section 4 (playbook replay, MCP, shell/COM,
   shortcuts, DOM via CDP, UIA, OCR, local vision, Claude, raw coordinates),
   extending D-008 and D-021.
4. **Browser lane (closes D-016).** Own MV3 extension in the owner's Brave
   profile using `chrome.debugger` + native messaging; no separate profile, no
   cookie sync.
5. **Local models resident together (amends D-005).** UI-Venus-1.5-8B +
   Qwen3-VL-8B at Q4/Q5, both in VRAM, no swapping; Holo2-8B single-model
   fallback; M1-11 decides the quant. No local planning model, nothing in
   system RAM.
6. **Model runtime ownership moves to relay-core in M2** (from M1-09's
   relay-screen runtime manager).
7. **Platform scope.** Windows through v1; macOS in M4 with Rust hands
   (xa11y, xcap, CGEvent, Vision OCR); Linux after macOS.
8. **D-001 confirmed** against the brief's "no cloud AI" line: local executes
   and follows playbooks, Claude plans, writes and recovers.

## Sources

- xa11y (MIT, Rust + Python, Windows UIA / macOS AX / Linux AT-SPI, pre-1.0): https://lib.rs/crates/xa11y , https://pypi.org/project/xa11y/
- Terminator by Mediar AI (MIT, Windows primary, macOS/Linux experimental): https://lib.rs/crates/terminator-rs
- Model scores and sizes: see D-005 and [research/local-models.md](../research/local-models.md).
- Claude Code headless and MCP: [research/claude-integration.md](../research/claude-integration.md).
