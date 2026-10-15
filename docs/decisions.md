# Decisions log

Append-only. Each entry: context, decision, consequences. Supersede an entry by
adding a new one that names it; do not rewrite history. Status is `accepted`,
`proposed` (needs data or owner input) or `superseded by D-0xx`.

---

## D-001 Split of responsibilities: local hands, cloud brain
*2026-10-14 · accepted · from the project brief*

Local AI does perception and execution only (read the screen, find elements,
click, type, follow playbooks). Claude does all judgment (planning new tasks,
writing free text, recovering when stuck). The local model is never asked to
decide what to do; Claude is never asked to emit raw clicks without Relay's
tools in between.

## D-002 Cloud via the owner's subscription through the official Claude Code CLI
*2026-10-14 · accepted*

Relay reaches Claude only by running the unmodified `claude` binary (interactive
or `claude -p`) under the owner's own login, with Relay plugged in as an MCP
server. Relay never calls the Anthropic API, never uses `--bare` (which ignores
the subscription and needs an API key), and never reads, stores, or forwards
OAuth tokens. Personal use on the owner's own machine only.

Why: Anthropic's legal/compliance page reserves OAuth/subscription auth for
"ordinary use of Claude Code and other native Anthropic applications", forbids
third parties to "collect, store, or intermediate Claude.ai credentials", and
says the rules do not "prevent an end user from signing in to the unmodified
Claude Code binary with their own Claude subscription". Driving that binary for
yourself is our reading of what is allowed; it is an inference, not an explicit
statement. Relay must never be distributed for others to use on a subscription.

Consequence: Claude Code is the one external runtime Relay cannot bundle. Relay
detects it (`claude --version`, currently 2.1.296 on the owner's PC) and its
login state, and links to the official installer if missing.

## D-003 Services via MCP, screen automation for everything else
*2026-10-14 · accepted · from the project brief*

Gmail, Calendar, Drive, GitHub, etc. go through Claude Code's MCP connectors.
Screen automation is reserved for apps/sites with no API.

## D-004 Local model runtime: bundled llama.cpp server, Ollama optional
*2026-10-14 · accepted (revisit after M1-11 benchmark)*

Context: the brief names Ollama (keep_alive ~60 s) but also asks that
"everything local" be one software with no external dependencies. Ollama is not
installed on the owner's PC today. The best GUI-grounding models are community
fine-tunes shipped as GGUF + mmproj pairs that Ollama's own Qwen-VL engine may
not import cleanly; llama.cpp loads them directly.

Decision: Relay downloads a pinned `llama-server.exe` CUDA 13 build (RTX 50 /
Blackwell needs CUDA >= 12.8) into `%LOCALAPPDATA%\Relay\runtime`, verifies its
checksum, and supervises it on a private localhost port. An `OllamaBackend`
adapter stays available for owners who already run Ollama.

The brief's "keep_alive ~60 s" becomes: Relay stops the llama-server process
after 60 s idle, which frees all VRAM, and restarts it on demand.

Consequences: Relay owns runtime updates (pin + checksum per release). Both are
MIT licensed, so redistribution is fine with attribution.

## D-005 Local model choice for a 16 GB RTX 5080, VRAM only
*2026-10-14 · proposed (decided by M1-11 benchmark)*

Constraints: whole model + mmproj + KV cache in VRAM (no CPU offload, leave
system RAM free); ~13.5 GB VRAM is free at idle on the owner's PC; screenshots
are large (3440x1440 ultrawides).

Candidates (published scores, not yet verified by us, measured on unquantized
weights):

| Model | ScreenSpot-Pro | OSWorld-G | License | Notes |
|---|---|---|---|---|
| UI-Venus-1.5-8B (Qwen3-VL base) | 68.4 | n/a | Apache-2.0 | best small grounding model; community GGUF |
| Holo2-8B (Qwen3-VL-8B-Thinking) | 58.9 | 70.1 | Apache-2.0 | also does navigation/verify; quants linked |
| Holo2-4B | 57.2 | 69.4 | Apache-2.0 | half the VRAM |
| Qwen3-VL-8B-Instruct | 49.9 | 54.8 | Apache-2.0 | official GGUF, best general description |
| UI-TARS-1.5-7B | 49.6 | 52.8 | Apache-2.0 | previous generation |
| Qwen2.5-VL-7B (brief's scaffold) | lower | lower | Apache-2.0 | superseded |

Sources: POINTS-GUI-G comparison tables (arXiv 2602.06391), model cards on
Hugging Face (inclusionAI/UI-Venus-1.5-8B, Hcompany/Holo2-8B).

Proposed default: **UI-Venus-1.5-8B at Q8_0 (~9 GB)** for `locate`, and
**Qwen3-VL-8B-Instruct at Q8_0** for `describe`/`verify`. Only one 8B model is
resident at a time (two do not fit in 16 GB with KV cache); Relay swaps on
demand. If swapping is too slow, fall back to a single model that does both
(Holo2-8B) or a 4B pair. M1-11 measures accuracy at Q4/Q8 on Relay's own
screens plus a ScreenSpot-Pro subset, latency, and VRAM, then this entry is
updated.

Rejected: OmniParser v2 (detector is AGPL-3.0 and scores 39.6 on
ScreenSpot-Pro); Fara-7B (browser-only agent).

## D-006 GPU idles when unused
*2026-10-14 · accepted · from the project brief*

Model loads on demand and unloads after 60 s idle (see D-004 for mechanism).
Before loading, Relay reads free VRAM via NVML and refuses to load into shared
system memory.

## D-007 UI stack: Tauri 2 + React + Untitled UI React
*2026-10-14 · accepted*

Context: the owner suggested Untitled UI React unless RefBox's own stack could
be identified. Inspection of the installed RefBox 0.7.36 (file names, Electron
version, and package.json dependency names only; no code extracted or copied)
shows Electron 40.9.3 + Vite + Vue 3 + ant-design-vue, Inter / JetBrains Mono
fonts, ~703 MB installed.

Decision: Relay does not copy RefBox's stack. Tauri 2 (WebView2, small
install, first-class sidecar support for the Python backend and llama-server)
with React + TypeScript + Untitled UI React, which matches the owner's TS
background. Electron stays acceptable if a Tauri blocker appears.

Design cues worth borrowing (not assets): warm light theme with one accent
color, mode badge, top bar with agenda/runs, device/connection chip, tray icon,
approval card with Run / Always allow / Cancel.

## D-008 Layered targeting, cheapest first
*2026-10-14 · accepted · from the project brief*

connector/API -> UIA accessibility tree -> Windows OCR -> local VLM -> Claude
with a screenshot. RefBox's published routing (script/API 10 ms, keyboard
shortcut 20 ms, accessibility handle 50 ms, GUI click 200 ms) matches the same
idea; Relay adds "keyboard shortcut" as a preferred action form inside
playbooks.

## D-009 Tool contract mirrors computer_toolset_20260801
*2026-10-14 · accepted*

relay-screen exposes the 17 toolset members with identical names and
parameters, plus a `batch` tool with the documented stop-on-first-failure
semantics and exact halt text `Not executed: an earlier computer action in this turn failed.`
Coordinates are screenshot pixels of the active display; Relay scales to
physical pixels and offsets into the virtual desktop. Default screenshot long
edge 1568 px (fits Claude Code's MCP image handling and the 25k-token MCP output
cap), max 2576 px / 3.75 MP per the toolset docs.

Multi-monitor (owner has 2x 3440x1440 + 2560x720): one display is "active";
`list_displays`/`select_display` switch it. Default active display is the one
holding the foreground window.

## D-010 Python 3.12 via uv, managed by Relay
*2026-10-14 · accepted*

The owner's system Python is 3.14; several ML/Windows wheels lag new Python
releases. Relay pins Python 3.12 with uv (`.python-version`), uses a uv
workspace for the monorepo, and ships a frozen runtime (PyInstaller `--onedir`)
in M3. Dev tooling: ruff, pytest, pyright (basic).

MCP Python SDK: pin `mcp>=1.28,<2` (`FastMCP`). v2.3.0 renamed it to
`MCPServer` two weeks ago; migrate once the ecosystem settles.

## D-011 Approval gates
*2026-10-14 · accepted*

M1: every state-changing action needs approval (milestone rule). Interactive
runs use Claude Code's own permission prompts (only read-only relay-screen
tools are allowlisted in `.claude/settings.json`). Headless runs use
`--permission-prompt-tool mcp__relay-screen__approve`. In a `batch`, approval is
checked before each action. From M2: approval only for irreversible actions
unless the app/site is allowlisted. Logins are always done by the owner;
Relay never types passwords.

## D-012 Kill switch and personal data
*2026-10-14 · accepted*

Kill switch: cursor in the top-left 8x8 px of the primary display, or
Ctrl+Alt+Pause, aborts and disarms Relay. Checked by a watcher thread and before
every input event. Relay implements its own SendInput layer, so this does not
depend on pyautogui's fail-safe.

Personal data never enters git: playbooks, `profile.yaml`, `.env`, runs,
screenshots. Secrets go to Windows Credential Manager (via `keyring`) or `.env`.

## D-013 Input and capture libraries
*2026-10-14 · accepted*

- DPI: `SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)` at process start,
  before any capture/UI import.
- Input: own ctypes `SendInput` layer (absolute moves normalized over the
  virtual desktop, Unicode typing via `KEYEVENTF_UNICODE`). Replaces the
  scaffold's pyautogui.
- Capture: `mss` for M1 (works across the NVIDIA and AMD iGPU outputs; the
  owner's 2560x720 strip is on the iGPU). `dxcam` / Windows Graphics Capture
  only if latency matters later.
- UIA: `uiautomation` package (thin, fast). OCR: Windows.Media.Ocr via `winocr`.

## D-014 Branching and history
*2026-10-14 · accepted · owner instruction*

`main` holds stable releases only. `dev` holds all ongoing work. Feature
branches open PRs into `dev`; `dev` is promoted to `main` at milestone ends.
Design history lives in `docs/` (this log, architecture, roadmap, session log).

## D-015 Fresh start instead of the old scaffold
*2026-10-14 · accepted*

The brief mentions an existing mss + pyautogui + Ollama qwen2.5vl:7b scaffold.
It is not in the repo and there is no `C:\Projects\relay` on the owner's PC, so
relay-screen starts fresh, keeping the same ideas (mss capture, local VLM) with
the replacements in D-004, D-005 and D-013.

## D-016 Browser automation lane (open question for M2)
*2026-10-14 · proposed*

Most real tasks happen in a browser. Chromium exposes an accessibility tree to
UIA but it is shallow and slow on big pages. RefBox bundles playwright-core
for this reason. Option for M2: launch the owner's preferred Chromium browser
(Edge is the default handler; Brave is installed; Chrome is not) with a
dedicated profile and remote debugging on localhost, and add a DOM lane between
UIA and vision. Trade-off: a separate browser profile means logins are not
shared with the owner's everyday profile. Decide in M2 with real data from the
M1 form demo.

## D-017 Wake up accessibility trees while Relay is armed
*2026-10-14 · accepted · from the RefBox research thread (PR #18)*

Chromium/Electron apps only build a full UIA tree when they believe assistive
technology is running. While Relay is armed it sets `SPI_SETSCREENREADER` (and
restores the previous value on exit or crash recovery). When Relay launches a
browser itself it adds `--force-renderer-accessibility`. This makes the UIA
layer (D-008) useful in Electron apps without bundling a screen reader such as
NVDA (GPL). Tracked in #7.

## D-018 Every act reports what changed
*2026-10-14 · accepted · from the RefBox research thread*

After each state-changing action relay-screen re-reads the target cheaply
(UIA value/focus where available, otherwise a pixel diff of the region around
the target) and appends a one-line verdict to the tool result, e.g.
`OK · target changed` or `OK · no visible change`. The local VLM `verify` stays
on demand. This catches silent misses early without a cloud round trip.
Tracked in #5.

## D-019 Approvals cannot be given by synthetic input
*2026-10-14 · accepted · from the RefBox research thread*

The approval prompt is Relay's own top-most window and ignores injected input
(events flagged `LLMHF_INJECTED`/`LLKHF_INJECTED`, including Relay's own
SendInput events), so the agent can never click "Approve" on itself. Tracked
in #6; the M3 tray UI keeps the same rule.

## D-020 Playbooks are Claude Code skill compatible
*2026-10-14 · accepted (amends architecture §7) · from the RefBox research thread*

Playbook front matter uses the Claude Code `SKILL.md` keys (`name`,
`description`) plus Relay's own keys (`app`, `match`, `inputs`,
`irreversible_steps`, `last_success`). Relay replays them locally without a
cloud call; when escalation is needed, Claude Code can load the same file as a
skill. Tracked in #12.

## D-021 More API lanes before pixels
*2026-10-14 · accepted for M2 · from the RefBox research thread*

Extends D-003/D-008: Office apps are driven through COM automation (Word,
Excel, Outlook) before screen automation; browsers get the DOM lane from D-016
using Playwright's `aria_snapshot()` over CDP, with form field values redacted
before anything is sent to Claude. Both are M2 work.

## D-022 Rejected for now: OmniParser icon fallback; deferred: PP-OCR
*2026-10-14 · accepted*

OmniParser v2 stays out even as an optional out-of-process helper: its detector
is AGPL-3.0 and it scores 39.6 on ScreenSpot-Pro against ~68 for the planned
grounding model (D-005), so it adds licence risk for little gain. Revisit in M4
only if the benchmark (#11) shows icon-only targets failing. PP-OCRv4 stays the
optional faster/denser OCR from D-013, to be added only if Windows OCR misses
text in #8.

## D-023 Typing: real key presses first, Unicode packets for the rest, paced
*2026-10-14 · accepted · measured on the owner's PC*

Windows 11 Notepad garbled `KEYEVENTF_UNICODE` packets for ordinary letters,
even with pacing, while a plain Tk text box received them fine. `type` now
sends every character the active keyboard layout can produce (`VkKeyScanW`)
as a virtual-key press with any shift state it needs, uses Unicode packets
only for characters outside the layout (✓, é on a US layout, emoji as
surrogate pairs), and sends one character per SendInput call with an 8 ms
pause. Verified on the owner's PC: exact read-back of ASCII, ✓, é/è and 😀 in
Notepad and in the Tk check; cursor moves land with 0 px error on all three
monitors. Clipboard paste remains an option for very long text (not needed yet).

## D-024 Change verdicts wait for animations
*2026-10-14 · accepted · measured on the owner's PC*

The D-018 verdict compared the screen once, 0.15 s after an act. Windows shell
surfaces animate: the Start menu and its search box were reported as
"no visible change" on the owner's PC although they had opened, and closing
Start with Escape too. The verdict now looks again every 0.1 s and stops at the
first change; only after 0.75 s with nothing changed does it say
"no visible change". Acts that change something cost no extra time; acts that
change nothing cost up to 0.6 s more. Verified with `scripts/check_batch.py`:
every step of the Start → "notepad" → Return batch now reports "screen changed".

## D-025 Relay's own prompt is the approval gate
*2026-10-14 · accepted (amends D-011) · M1-06*

D-011 left interactive approvals to Claude Code's permission prompts. Two problems
showed up while building M1-06: a `batch` is one tool call, so Claude Code could
only approve it as a whole, and Claude Code's prompt lives in a terminal that
Relay can type into, so Claude could approve its own actions (against D-019).

Relay's own prompt is therefore the gate for every act, in single tools and in
each step of a batch. It is a small top-most window in a separate process that
drops injected mouse and keyboard input aimed at it, shows the action in words
plus a screenshot with the target circled, and gives focus back to the window
that had it. Buttons: Approve, Approve rest of batch (batches only), Deny. No
answer in 120 s counts as Deny. The relay-screen act tools are allowlisted in
`.claude/settings.json` so the owner is not asked twice. The `approve` tool
(for `claude -p --permission-prompt-tool`) allows relay-screen tools for the
same reason and shows any other tool in the same prompt.
`RELAY_APPROVAL=off` turns the per-act prompt off; it is never the default.

Kill switch details: Relay refuses to move the mouse into the 8x8 px corner
itself, so a cursor found there was put there by the owner. When stopped, key
and button releases still go through so nothing stays held. Reads
(screenshot, zoom, cursor_position) keep working. Re-arming needs the owner's
answer in the same prompt (`rearm` tool).

Audit log: `runs/<run-id>/actions.jsonl` (cwd or `RELAY_RUNS_DIR`), one line
per action with input, approval, result or error and duration, plus 480 px
before/after thumbnails of the active display for acts.

## D-026 UI Automation runs on one thread and hands out element ids
*2026-10-14 · accepted · M1-07*

`uia_tree` and `uia_find` give every element they list a short id (`e12`) that
`uia_invoke` and `uia_set_value` take, so Claude never has to describe an element
twice. Ids stay valid for the server's lifetime (last 5,000 kept); a vanished
element gives a clear error. All COM calls run on one dedicated thread, because
UIA element pointers belong to the thread that created them and MCP calls
arrive on varying worker threads. Listings are capped at 400 elements, 24k
characters (about 6-8k tokens) and 6 s, with a note telling Claude how to narrow
the search. With no `window` given, the search covers the foreground window plus
the other visible top-level windows of the same process, so menus and dialogs
that open as separate windows are found. `uia_invoke` uses the control's own
action (Invoke, then Toggle, Expand/Collapse, Select) and moves no mouse. The
UIA tools go through the same kill switch, approval prompt and audit log as the
computer tools, and can be used inside `batch`. The screen-reader flag (D-017) is
set when the UIA backend starts and restored at exit.

## D-027 UIA listings name the display and skip layout noise
*2026-10-14 · accepted · M1-07, amends D-026*

The live check on the owner's three monitors showed two gaps. An element on a
monitor other than the active one was listed only as "not on the active
display", which left Claude no way forward. It now reads "on display N, not the
active one (select_display N)". `uia_tree` also skips unnamed elements with no
automation id when they are hidden (no bounds) or are bare layout containers
(Pane, Group, Custom, Text with no action). Their children are still listed one
level up, and `depth` still counts real levels. A filtered listing hides
nothing. On example.com in Edge this cut the depth-12 tree from about 5.5k to
2.6k tokens without losing any control that has a name, an id or an action.
`uia_find` is unchanged.

## D-028 The owner's browser is Brave Origin
*2026-10-14 · accepted · corrects the browser facts behind D-016 and D-017*

The owner uses Brave Origin only. Earlier notes said Edge was the default. The
Edge browser is no longer installed, though `EdgeCore` (a Windows component)
still has an `msedge.exe` and the https handler still names Edge. Brave Origin
shows a purchase screen in a fresh profile, so a throwaway profile is not an
option for it. Live checks open one new window in the owner's own Brave profile
and close only that window. Brave builds a full UIA tree for page content while
the screen-reader flag (D-017) is set, with no extra flags. For M2's browser lane
(D-016), a dedicated Brave profile would need the owner to restore their
purchase in it first.

## D-029 OCR calls Windows.Media.Ocr directly
*2026-10-14 · accepted (amends D-013) · M1-08*

D-013 named the `winocr` package. It is a 40-line wrapper with no license field,
so `find_text` uses the `winrt-Windows.Media.Ocr` projections (MIT) directly.
The engine uses the owner's profile languages and falls back to en-US. Captures
larger than the engine's limit (about 2600 px) are split into overlapping tiles,
and each line is kept only by the tile that owns its center. Regions under 1200
px are upscaled 2x, because small UI text reads better bigger. Queries match
spans of consecutive words. They are fuzzy by default (similarity of 0.8 or
more, so OCR slips like "Cancle" still match) and exact with `fuzzy=false`.
Results give centers in screenshot pixels and the OCR time.

## D-030 Relay discovers installed apps instead of using fixed paths
*2026-10-14 · accepted · owner request*

The owner asked that Relay find installed programs itself. Two Relay-only tools
do it. `list_apps(query, refresh)` is a read. `launch_app(name, args)` is an act,
behind the approval prompt like any other. The catalog merges three sources at
run time:
- Start menu apps from `Get-StartApps`, Store apps included. Known-folder AppIDs
  are resolved to their program path.
- Registered App Paths (HKCU, HKLM and WOW6432Node).
- Registered browsers (`Clients\StartMenuInternet`). The https `UserChoice`
  handler is marked as the default.

Start menu names win, browsers fill in exe paths, and App Paths add what is left.
The scan is cached for 5 minutes, and a name that is not found triggers one
rescan. Names resolve exact first, then by program name, prefix, word and fuzzy.
A name that stays ambiguous returns the closest candidates instead of guessing.
`"browser"` means the default browser, unless that browser has no Start menu
entry and exactly one other browser does. That covers this PC (D-028): the
handler still names EdgeCore, and Brave Origin is the browser the owner uses.

Launching goes through `shell:AppsFolder` when there are no args, the same way a
Start menu click does, so Store apps work too. With args, the program is started
directly, detached and outside Claude Code's job object where Windows allows it.
Relay only launches apps from the catalog, never an arbitrary path. Afterwards it
waits up to 10 s for the app's window. That is a new window, the window that came
to the front, or one of the app's windows whose title changed (a browser adding a
tab). Relay brings the window to the front with AttachThreadInput, because the
server is a background process, and reports the window's title and its display.
Live checks use the catalog too (`scripts/check_apps.py`; `check_uia.py` finds
Brave Origin this way).

## D-031 Claude merges its own PRs into dev and main
*2026-10-14 · accepted · owner request · amends D-014*

The owner doesn't want to merge PRs by hand or mark drafts ready. From now on
Claude opens PRs ready for review and merges them into `dev` once CI is green.
It then merges `dev` into `main` through a dev-to-main PR, also on green CI, and
closes the issues each PR resolves. `main` still only receives code through
`dev`. Live checks that open windows on the owner's PC run only when the owner
asks for one; green CI is the gate for merging.

## D-032 relay-core is an always-on Rust daemon wrapping the Python hands
*2026-10-14 · accepted · owner's revised brief, proposals/final-architecture.md §3 · supersedes the Python relay-core in architecture §5*

relay-core is a Rust daemon built in M2. It starts at login, supervises its
children, owns the local sockets and ships as one binary next to Tauri. It
wraps the Python relay-screen from M1 and keeps it running, and does not
replace it. relay-screen stays the Windows hands through v1: it is verified
on the owner's PC, and Python adds well under a millisecond to an action.
The real per-step costs are vision and Claude calls. The hands move to Rust
only for macOS (M4), behind the same tool contract (D-009), with
relay-screen's tests used as contract tests.

## D-033 relay-screen also serves MCP over localhost HTTP
*2026-10-14 · accepted · proposals/final-architecture.md §3.2*

relay-screen also serves streamable-HTTP MCP on 127.0.0.1, protected by a
local token that belongs to Relay and is never a Claude credential.
relay-core keeps one instance running, and Claude Code connects to it through
the `--mcp-config` Relay passes. This removes the 1-2 s Python start-up on
every run. Stdio stays for development. The kill switch, approvals and audit
log stay inside relay-screen, next to SendInput, so no route can bypass them
(D-012, D-019, D-025). This is a small task at the end of M1 or the start
of M2.

## D-034 The speed ladder
*2026-10-14 · accepted · extends D-008 and D-021 · proposals/final-architecture.md §4*

Relay always uses the highest rung that works:
0. Playbook replay.
1. Service APIs through MCP connectors.
2. Shell, PowerShell or COM.
3. Keyboard shortcuts.
4. Browser and Electron DOM through CDP.
5. The accessibility tree.
6. OCR.
7. Local vision models.
8. Claude with a screenshot.
9. Raw coordinates.

Raw coordinates are only ever the output of rungs 6 to 8 on the current
screen, never replayed from an old playbook. Every step logs which rung it
used and how long it took, so the router learns which rung works for each app.

## D-035 Browser lane: Relay's own Brave extension
*2026-10-14 · accepted · closes D-016 · proposals/final-architecture.md §4*

Relay ships a small MV3 extension that the owner installs once in their real
Brave Origin profile (D-028). It uses `chrome.debugger` for CDP inside the
browser and talks to relay-core over native messaging. Logins stay where they
are: no separate profile and no cookie copying. Chromium ignores
`--remote-debugging-port` on the default profile, so this is the way to reach
the owner's logged-in sessions. Electron apps that Relay launches itself can
get a debugging port. Electron apps that are already running fall back to the
accessibility tree.

## D-036 Both local vision models stay resident at Q4/Q5
*2026-10-14 · accepted (amends D-005) · proposals/final-architecture.md §6*

UI-Venus-1.5-8B handles grounding (`locate`). Qwen3-VL-8B-Instruct handles
`describe`, `verify` and replay step checks. Both stay in VRAM together at
Q4/Q5, about 11-12 GB, so a replay that alternates `locate` and `verify`
never pays a model swap. Holo2-8B is the single-model fallback if the M1-11
benchmark shows the Q4 pair losing more than about 2 points against Q8.
Relay runs no local planning model and keeps nothing in system RAM. Both
models unload after 60 s idle. When NVML reports too little free VRAM, Relay
loads only the grounding model or goes straight to Claude.

## D-037 relay-core owns the model runtime from M2
*2026-10-14 · accepted · proposals/final-architecture.md §3.2*

M1-09 builds the llama-server runtime manager in relay-screen, as planned.
In M2 it moves to relay-core. relay-core is the long-lived process, and the
playbook runner and the browser lane need the models too.

## D-038 Platforms: Windows through v1, then macOS, then Linux
*2026-10-14 · accepted · proposals/final-architecture.md §7*

v1 is Windows only. macOS comes in M4 with Rust hands (xa11y or AXUIElement,
ScreenCaptureKit, CGEvent, Vision OCR, AppleScript), tested on a real Mac.
Linux follows macOS, Wayland first, using AT-SPI and portals. The MCP tool
contract is the boundary that keeps relay-core, playbooks, the UI and
Claude's prompts the same on every OS.

## D-039 D-001 confirmed against the revised brief
*2026-10-14 · accepted · proposals/final-architecture.md §5*

The owner's brief opened with "no cloud AI" but ended with "local AI is the
main brain for local automation, Claude does the thinking". Relay keeps
D-001. The local side executes: the playbook runner, perception and checks,
with no cloud call. Claude plans new tasks, writes free text and recovers
when a check fails. Successful runs are saved as playbooks, so later runs
need no Claude call.

## D-040 Always-on perception in relay-core
*2026-10-14 · accepted · M2 · proposals/final-architecture.md (revised in #31) · will replace D-024 polling*

In M2, relay-core keeps a running picture of the screen:
- a Windows Graphics Capture frame stream for each monitor;
- settle and change events computed from frame diffs;
- a UIA event watcher for focus, structure and property changes;
- an overlay that shows what Relay is about to touch.

relay-screen uses these events instead of capturing and polling after each act.
The "what changed" verdicts (D-018) come from the stream, so they are faster
and need no fixed waits. Until then, D-024 polling stays.

## D-041 Text first to Claude
*2026-10-14 · accepted · proposals/final-architecture.md (revised in #31)*

What Relay reports to Claude leads with text: the UIA or ARIA elements, the
focused control and the window title. A screenshot is attached only when text
is not enough, such as canvas apps, unknown layouts, or when Claude asks for
one. Text is cheaper for the owner's usage limits and more precise.

## D-042 Models load at task start and unload 60 s after it ends
*2026-10-14 · accepted · refines D-006 and D-036*

The vision models load when a task that may need them starts, so the first
`locate` does not pay a cold start in the middle of a run. They unload 60 s
after the task ends, not 60 s after their last call.

## D-043 Optional uiAccess helper for elevated windows
*2026-10-14 · accepted · M3 · optional*

Windows blocks input from a normal process into elevated (admin) windows.
In M3, Relay can ship a small optional helper with `uiAccess=true`, installed
under Program Files and signed with a certificate the owner creates on their
own machine. The helper forwards input to elevated windows. It is off by
default, and the same kill switch and approvals apply to it. Relay never asks
for admin rights for everything else.

## D-044 Target: one download that sets up the whole app
*2026-10-14 · accepted · owner request · reshapes the roadmap*

The owner wants a complete, RefBox-like product: download one installer,
click through it, and have everything work, with no other app to set up by
hand. v1.0 is defined by that experience:
- **One installer** (`Relay-Setup.exe`, the Tauri NSIS bundle). It ships the
  app (D-007), the Rust relay-core (D-032), relay-screen frozen with
  PyInstaller `--onedir` (D-013), and the pinned llama.cpp CUDA build (D-004).
  Python, uv and the repo are not needed on the machine.
- **A first-run wizard in the app.** It checks the GPU and free VRAM, sets up
  Claude Code (D-045), lets the owner pick and download the local model (D-046),
  helps them install the Brave extension (D-035), and runs a short
  self-test with the owner watching. It can be re-run from Settings.
- **Everything else lives in Settings** (D-048). No config files to edit, and
  no terminal.
- **Updates from inside the app**: Tauri's updater reads signed releases that
  CI builds from tags on `main`.

The personal-use rule (D-002) stays. The installer is built for the owner's
machine, and each install signs in to Claude Code with its own account.

## D-045 Claude Code is set up by Relay, not bundled
*2026-10-14 · accepted · refines D-002*

Relay can't ship the Claude Code binary or touch its credentials (D-002), but
it can make setting it up feel like part of Relay. The wizard runs
`claude --version`. If Claude Code is missing, Relay runs Anthropic's official
Windows installer, unmodified, from Anthropic's own URL, and shows its
progress. Relay then opens `claude` in a terminal pane inside the app, where
the owner signs in with their own subscription. Relay only checks that it
worked (`claude -p` with a trivial prompt) and never reads the token. Settings
shows the Claude Code version and login state, with "Update" and "Sign in
again" buttons that run the official commands.

## D-046 The owner picks the local UI model in Settings
*2026-10-14 · accepted · amends D-005 and D-036*

Settings > Local AI lists a curated model catalog:
- UI-Venus-1.5-8B
- Holo2-8B
- Holo2-4B
- Qwen3-VL-8B-Instruct
- newer GUI models as they appear.

Each entry shows:
- its role: "where is it" (grounding), "check / describe", or both;
- its download size and VRAM need at each quant;
- a fit badge from the detected GPU (fits / tight / won't fit with the other
  model loaded);
- its score and latency from Relay's own benchmark on the owner's screens
  (M1-11), when available.

One option is marked recommended, from the benchmark and the GPU. Picking a
model downloads it with resume and SHA-256 checks, and Relay switches to it
without a restart. The owner can also add a custom GGUF (model plus mmproj)
from disk or a Hugging Face URL. A button runs the benchmark again for any
installed model. D-036's resident pair stays the default. The picker lets the
owner override it, and nothing is downloaded until they pick or accept the
recommendation.

Model files live in a models folder the owner can change in Settings. The
default is `%LOCALAPPDATA%\Relay\models`, and Settings shows how much space
each model uses and can delete it.

## D-047 One MCP hub for Claude and Relay
*2026-10-14 · accepted · owner request*

Settings > Connections is the one place to manage MCP servers. It does not
keep its own copy of the list: it reads and writes Claude Code's MCP config
through the official `claude mcp list/add/remove/get` commands, at user
scope. Whatever the owner adds there is what Claude sees, and the other way
round.
- **Relay's own servers** (relay-screen through relay-core D-033, the browser
  bridge) are registered there by the installer and shown as built in. They
  can be paused but not deleted.
- **Local MCP servers** (stdio or HTTP) can be added from a short catalog of
  well-known servers or by hand. Their secrets go into Windows Credential
  Manager and reach the server as environment variables at launch. They are
  never written to the config file or the repo.
- **claude.ai connectors** (Gmail, Calendar, Drive, ...) belong to the owner's
  Claude account. They are listed with their state, and "Manage" opens
  claude.ai's connector settings. Relay cannot add them for the owner.
- relay-core's playbook runner can call local MCP servers from this same list
  directly when it replays a step, so a playbook doesn't need Claude for a step
  it already knows.

## D-048 Every setting lives in the app
*2026-10-14 · accepted*

Settings covers:
- General: start at login, keep running in the tray, language.
- Claude: version, sign-in state, model and effort passed to `claude -p`.
- Local AI: the model picker (D-046), the idle unload time (D-042), the models
  folder.
- Connections (D-047).
- Browser: Brave extension status and the browser to use.
- Safety: the approval mode, the kill switch corner and hotkey, which actions
  always need approval, phone approvals.
- Playbooks: list, edit, delete.
- Privacy: where the audit logs and screenshots are kept, and how long.
- Updates.
- Diagnostics: the `doctor` checks, the log folder, a "copy report" button.

Settings are stored in `%APPDATA%\Relay\settings.json`, which relay-core owns.
The app edits them through relay-core's API, never by writing the file
directly.

## D-049 Live checks on the owner's PC run only on request
*2026-10-14 · accepted · owner instruction · amends the autonomy note in D-031*

Checks that open windows or move the mouse on the owner's PC (`scripts/check_*.py`)
run only when the owner asks for one. Green CI is the gate for merging into
`dev` and `main`. The first-run self-test (D-044) is the owner-started check
for releases.

## D-050 Local checks only, no GitHub Actions; pnpm for the app
*2026-10-14 · accepted · owner instruction · amends D-031, D-044 and D-049*

The owner wants no GitHub Actions for tests or checks. `.github/workflows/ci.yml`
is removed. Every check runs locally, and `uv run python scripts/check.py`
(ruff, format check, pyright, pytest without `desktop` tests) must pass before
every push and every merge. That replaces "green CI" as the merge gate in D-031
and D-049. Windows-only tests that open no windows can run on the owner's PC.
Tests and checks that open windows still run only when the owner asks. The
release installer (D-044) is also built and checked locally and then attached
to a GitHub Release. No Actions workflow builds it.

The Tauri + React app uses **pnpm** as its package manager. That covers
`pnpm create tauri-app`, `pnpm install`, `pnpm tauri dev` and
`pnpm tauri build`, with `pnpm-lock.yaml` committed. npm and yarn are not used.

## D-051 "Auto-merge when ready" through a local-checks status
*2026-10-14 · accepted · owner request · builds on D-050*

The owner wants "Auto-merge when ready" on PRs. GitHub only offers it when
something is still pending on the PR, so a ruleset on `dev` and `main` requires
a commit status named `local-checks`. Nothing in GitHub Actions produces it:
`uv run python scripts/check.py --report` runs the local checks (D-050) and
posts `local-checks` = success or failure on the pushed HEAD commit, using the
GitHub CLI's login. A PR with auto-merge on merges as soon as a local run
reports success.

The ruleset requires no reviews, and repository admins are on its bypass list.
Claude's own PRs go through the GitHub connector, which has no way to post a
commit status, so Claude runs the same checks in its session and merges
directly under that bypass (D-031). If Claude's sessions ever get a way to post
statuses, they will use `--report` and auto-merge instead.

Ruleset settings (Settings > Rules > Rulesets > New branch ruleset):
- name: `dev and main`;
- enforcement: Active;
- bypass list: Repository admin;
- target branches: `dev` and `main`;
- rule: "Require status checks to pass", with the check `local-checks`.

## D-052 Runtime and model pins come from the publisher, recorded locally
*2026-10-14 · accepted · M1-09*

Relay does not hard-code llama.cpp build numbers or model checksums. On
install it asks the publisher:
- llama.cpp: the GitHub release (the latest unless a tag is given). It takes
  the Windows CUDA zip with the newest CUDA version the driver supports
  (NVML), never older than 12.8, plus the matching `cudart` zip. Each file is
  checked against the `sha256:` digest GitHub publishes for the asset.
- Models: a Hugging Face GGUF repo. `main` is resolved to a commit, and each
  file is checked against the SHA-256 Hugging Face publishes for it. The
  weights file is the requested quant; the projector prefers F16/BF16 because
  a quantized projector costs grounding accuracy.

What was installed (tag or commit, URL, SHA-256, size) is written to
`installed.json` in the Relay home. `relay-screen doctor --verify` re-hashes
the model files against it. Downloads resume from a `.part` file and appear
only after they verify. Zip entries that would land outside the runtime
folder are refused.

Only publisher-owned repos go into the catalog. A checksum from a lookalike
repo proves nothing, so the GUI models (UI-Venus, Holo2) have no source until
M1-11 picks one. Qwen3-VL-8B-Instruct uses Qwen's own GGUF repo. The owner can
still add any repo as a custom model (D-046).

llama-server runs with `CREATE_NO_WINDOW`, so loading a model never opens a
console window on the owner's screen (D-049). Its output goes to
`logs/llama-<model>.log`; load, unload and first-answer timings go to
`logs/runtime.jsonl`.

## D-053 relay-core is a library the app links, for now
*2026-10-14 · accepted · app thread, PR #46 · amends D-032 for M3*

relay-core is a Rust library that the Tauri app links and runs in its own
process. The always-on relay-core daemon from D-032 wraps that same library
in M2. Until then, nothing runs when the app is closed.

## D-054 relay-screen ships as embedded CPython, not a PyInstaller freeze
*2026-10-14 · accepted · app thread, PR #46 · replaces the PyInstaller part of D-044 and D-013*

The installer ships the embeddable CPython 3.12 (from python.org's NuGet
package, SHA-256 checked when it is built) with relay-screen and its
site-packages beside it. A plain interpreter avoids the freeze's import and
antivirus surprises, and the bundle uses the same code the tests run.
Uninstall removes the whole Python folder, `__pycache__` included.

## D-055 Setup downloads the model runtime and models; the installer stays small
*2026-10-14 · accepted · app thread, PR #46 · amends D-044*

Relay-Setup.exe (about 26 MB) does not bundle llama.cpp or any model. The
first-run wizard downloads both through relay-screen's runtime (D-052), so
the CUDA build matches the driver and only the model the owner picks is
fetched.

## D-056 How the installer is built
*2026-10-14 · accepted · app thread, PR #46*

The installer cross-builds on Linux with the `x86_64-pc-windows-gnu` target,
so a cloud session can produce it. On Windows it builds with MSVC.
`scripts/check.py` also runs `cargo fmt --check`, `cargo clippy`,
`cargo test` and `tsc` whenever cargo and pnpm are installed (D-050).

## D-057 Unsigned installer; updates come as a new installer in Downloads
*2026-10-14 · accepted · app thread, PR #46 · amends D-044 until #35*

The installer is unsigned for now. To update, the app downloads the newer
Relay-Setup.exe from GitHub Releases into the owner's Downloads folder and
runs it. Tauri's signed updater replaces this once code signing exists
(issue #35).

## D-058 The app drives the model runtime through relay-screen's CLI
*2026-10-14 · accepted · app thread, PR #46 · M3 bridge to D-037*

Settings > Local AI calls the M1-09 commands (`relay-screen models list
--json`, `models download`, `runtime install`) and reads their `--json`
progress lines, so there is one store and one `installed.json`, not a second
copy in the app. relay-screen reads the models folder from settings.json
`local_ai.models_dir`; the old top-level `models_folder` still works. This
moves into relay-core with the runtime in M2 (D-037).

Not read by relay-screen yet (to do in M1-10/M1-11):
- `local_ai.grounding` and `local_ai.describe` (`{model_id, quant}`): the
  supervisor still uses the catalog defaults (D-036);
- `local_ai.idle_unload_secs`: the supervisor still uses 60 s (D-042);
- `safety.always_ask` and `phone_approvals`.

## D-059 llama.cpp: newest release that has a Windows CUDA build
*2026-10-14 · accepted · app thread, PR #46 · amends D-052*

GitHub's "latest" release of ggml-org/llama.cpp is v0.6.0, which has no
Windows binaries. With no tag given, relay-screen now reads
`/releases?per_page=30` and takes the newest release that has a Windows CUDA
build the driver supports. On the owner's PC that installs b11624 with
CUDA 13.4.

## D-060 Browser lane for now: a pinned Playwright MCP on Relay's own profile
*2026-10-15 · accepted · app thread, PR #52 · defers D-035's extension*

The browser lane is a `relay-browser` MCP server: `@playwright/mcp@0.0.83`,
pinned, started with `--executable-path` set to the Chromium browser chosen
in Settings > Browser (on auto, Brave Origin). It uses Relay's own profile in
`%LOCALAPPDATA%\Relay\browser-profile`, because Chromium refuses automation
on the everyday profile. The owner signs in to sites in that profile once.
This replaces D-035's extension step for now. The extension can come back
later if the owner wants Relay to use their everyday logged-in profile.

## D-061 Read-only browser tools run without a prompt
*2026-10-15 · accepted · app thread, PR #52 · refines D-025*

relay-screen's `approve` tool lets the browser lane's tools that only look
(snapshot, screenshot, console, network, wait) run without asking. Every
other browser tool still goes through the approval prompt, the same as
relay-screen's act tools. `approve` returns one plain text block with no
`structuredContent`: Claude Code rejected the structured form with
"Permission prompt tool returned an invalid result".

## D-062 In-app Claude sign-in through the official CLI, out of sight
*2026-10-15 · accepted · app thread, PR #52 · refines D-045*

Sign-in runs `claude auth login --claudeai` (or `--console`) in a hidden
pseudo-terminal. The app shows the link from its output, takes the code the
owner pastes back, and polls `claude auth status` until it reports signed
in. The token stays inside Claude Code; Relay only reads the signed-in flag
(D-002).

## D-063 Relay installs per machine into Program Files
*2026-10-15 · accepted · owner request · app thread, PRs #56/#57 · amends D-044*

Relay-Setup.exe installs for all users into `C:\Program Files\Relay` and asks
for admin rights once during setup. The owner asked for this location.
Relay 0.1.0 installed per user, so the installer's pre-install hook removes
any per-user 0.1.0 copy (`C:\relay`), along with its `HKCU\...\Uninstall\Relay` and
`HKCU\Software\Relay` keys. Per-user data (`%LOCALAPPDATA%\Relay` for the
runtime, models and browser profile, and `%APPDATA%\Relay\settings.json`)
stays where it is. D-057's update flow is unchanged: a newer
Relay-Setup.exe downloads into Downloads and runs.

## D-064 The approval prompt is a plain Win32 window
*2026-10-15 · accepted · app thread, PR #60 · implements D-019 and D-025*

`relay_screen/safety/prompt.py` builds the approval prompt with ctypes and
Win32 directly, not tkinter. The embedded CPython that the installer ships
(D-054) has no tkinter, so before this change every gated call was refused
without a prompt ever appearing. The window keeps D-019's low-level input
hooks, which ignore injected clicks and keys. `python -m
relay_screen.safety.prompt --check` builds the window off-screen, and the
doctor and the app's self-test run that check as "Approval prompt".

## D-065 A prompt that can't open is not a denial
*2026-10-15 · accepted · app thread, PR #60 · refines D-025*

When the approval prompt can't be shown, relay-screen raises
`PromptUnavailableError`. Claude is told the owner was not asked, not that
the owner said no, so it reports a setup problem instead of giving up on the
task as refused. The call is still not run.

## D-066 Browser navigation runs freely; the rest can be allowed per request
*2026-10-15 · accepted · app thread, PR #60 · extends D-061*

On relay-browser, `browser_navigate` and `browser_navigate_back` run without
a prompt, along with D-061's read-only tools. Every other browser tool
(click, type, fill, and so on) still asks. The prompt has a third button,
"Allow the browser for this request", which approves the remaining browser
tools until that `claude -p` run ends. relay-screen's desktop act tools are
not covered by that button.

## D-067 The local model can do UI tasks on its own when Claude is unavailable
*2026-10-15 · accepted · owner request · amends D-032..D-043 (and D-001's "local hands only")*

The owner: UI automation has to work with the local AI alone, so Relay still
does mouse, keyboard and browser-window tasks when Claude can't be reached
(signed out, offline, out of quota). Until now nothing worked without Claude.

Decision:
- Claude stays the preferred brain whenever it is available. When it isn't,
  the app hands the task to Relay's **local operator**:
  `relay-screen agent "task" --json`.
- The local operator is a plan-act loop on the local vision model. Each step
  it takes a screenshot, sends the model the task, the earlier steps and the
  image (at most 1280 px on the long edge), and gets back one JSON action:
  click, double_click, right_click, type, key, scroll, launch_app, wait,
  done or fail. Coordinates are 0-1000 of the image for the Qwen3-VL family
  (`ModelSpec.coords`), and Relay converts them to screenshot pixels.
- Actions run through the same `Computer` that Claude drives, so the kill
  switch, the approval prompt (D-025, D-064) and the audit log all apply.
  An owner denial, the kill switch or a prompt that can't open stops the run.
  Other errors go back to the model as the step's result.
- The model never signs in or types passwords. A sign-in page ends the run
  with "sign-in needed", as the rule for Claude already says.
- The run gives up after 25 steps by default, after three unusable replies,
  or when the same action changes nothing three times in a row.
- Model choice: `--model`, then settings.json `local_ai.agent`, then
  `local_ai.describe` or `local_ai.grounding` (each `{model_id}`), then the
  first downloaded of Qwen3-VL-8B-Instruct, Holo2-8B, Holo2-4B and UI-Venus.
  Qwen3-VL-8B-Instruct is the only one with a download source today (D-052),
  so it is the local operator's default.
- The model loads when the run starts and unloads when the run ends, because
  the CLI process ends with the task.

This changes D-001's split for the no-Claude case only. With Claude
available, the local model is still hands and eyes (D-008 layering, D-036).
