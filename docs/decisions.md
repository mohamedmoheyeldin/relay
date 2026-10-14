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
