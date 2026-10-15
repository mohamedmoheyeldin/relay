# Session log

Short entries so later sessions can pick up where earlier ones stopped. Newest first.

## 2026-10-15 · App round 2 from the owner's first install (app thread)
- Chat tool calls all failed with "Permission prompt tool returned an invalid result": relay-screen's `approve` returned FastMCP `structuredContent`; it now returns one bare text block (test asserts it). Browser-lane tools that only look (snapshot, screenshot, console, network, wait) are allowed without a prompt.
- Self-test hung on "asking Claude": the check now times out after 90 s, child processes are killed on drop, and only one check runs at a time. `claude -p` itself answered in about 3 s on the owner's PC.
- `\\?\` is stripped from the bundle and launcher paths (`paths::plain`), so relay-screen re-registers with `C:\relay\python\pythonw.exe`.
- Setup is one page of checks that run on their own; sign-in runs `claude auth login --claudeai` out of sight, opens the page, accepts a pasted code and polls `claude auth status` (xterm removed).
- Browser lane: `relay-browser` = pinned `@playwright/mcp@0.0.83` with `--executable-path` of the chosen Chromium browser (Brave Origin auto-picked) and Relay's own profile in `%LOCALAPPDATA%\Relay\browser-profile`. Settings > Browser shows it; decision sent to the plan thread.
- Connections: Sign in/out for OAuth servers (`claude mcp login/logout`), plugin servers name their plugin and can turn it off, and a plugins list toggles `claude plugin enable/disable`.
- Design: app-styled scrollbars, container-query layouts down to 520 px wide, page and row motion, skeletons, fixed the close dialog.

## 2026-10-14 · App and installer (app thread)
- PR #46 merged into `dev` and #47 into `main`: the Tauri app and Relay-Setup.exe (about 26 MB). Recorded as D-053 (relay-core is an in-process library for now), D-054 (embedded CPython replaces PyInstaller), D-055 (runtime and models download during setup), D-056 (Linux cross-build, MSVC on Windows; check.py runs cargo and tsc when present), D-057 (unsigned; updates download the new installer into Downloads), D-058 (Local AI drives relay-screen's CLI, `local_ai.models_dir`) and D-059 (llama.cpp: newest release with a Windows CUDA build; b11624 / CUDA 13.4 on the owner's PC).
- Still to wire into relay-screen in M1-10/11: `local_ai.grounding`/`describe`, `local_ai.idle_unload_secs`, `safety.always_ask`, `phone_approvals`.

## 2026-10-14 · M1-09 local model runtime
- Built `relay_screen/runtime/` and the `relay-screen doctor | runtime | models` commands (D-052). Pins come from the publisher (GitHub asset digests, Hugging Face LFS SHA-256) and are recorded in `installed.json`; the GUI models get a source in M1-11, Qwen3-VL-8B uses Qwen's own repo. 23 unit tests with fake downloads, processes, clock and GPU.
- Not yet run on the owner's PC (it downloads about 7 GB and loads the GPU); waiting for them to ask.
- Next: M1-10 local VLM tools (`locate`, `describe`, `verify`) on top of the supervisor.

## 2026-10-14 · Auto-merge when ready
- `scripts/check.py --report` posts a `local-checks` commit status via the GitHub CLI; a `dev and main` ruleset requiring it makes "Auto-merge when ready" available (D-051). The ruleset is set up in the repo settings by the owner; Claude's PRs merge directly under the admin bypass.

## 2026-10-14 · Local checks only, pnpm
- Owner: no GitHub Actions for tests; run every check locally. Removed `.github/workflows/ci.yml`, added `scripts/check.py` (ruff, format, pyright, pytest) as the gate before push and merge. The app will use pnpm. D-050.

## 2026-10-14 · Product target: one download, one app
- Owner wants a complete Violoop-like app: one installer, local model chosen in Settings, all MCP connections managed in one place. Recorded as D-044 (one installer + first-run wizard), D-045 (Relay sets up Claude Code with Anthropic's official installer and an in-app sign-in), D-046 (model picker), D-047 (MCP hub over `claude mcp`), D-048 (all settings in the app) and D-049 (live PC checks only on request). Roadmap rewritten around v1.0 = clean install from `Relay-Setup.exe`.

## 2026-10-14 · More architecture decisions
- Adopted D-040 to D-043 from the revised proposal (#31): always-on perception in relay-core (WGC frame stream, change events, UIA watcher, overlay), text-first observations to Claude, models load at task start and unload 60 s after it ends, optional signed uiAccess helper for elevated windows.

## 2026-10-14 · Merge rule and final architecture adopted
- Owner: no more draft PRs; Claude merges its own PRs into `dev` and then `dev` into `main` on green CI (D-031). Owner stopped live PC checks that kept opening windows: run them only when asked.
- Adopted the final architecture proposal as D-032 to D-039 (Rust always-on core wrapping the Python hands, warm MCP over localhost HTTP, speed ladder, Brave extension browser lane, two vision models resident, runtime in the core from M2, Windows through v1, D-001 confirmed).

## 2026-10-14 · Final architecture: how Violoop gets smooth control
- Owner asked to rethink the proposal against how Violoop achieves smooth Windows/macOS automation. Read violoop.ai and its Q&A (routing table, HDMI/HID, models, text-only cloud summaries) alongside research/violoop.md.
- Revised [proposals/final-architecture.md](proposals/final-architecture.md): new section 2 (what Violoop does, what Relay copies, beats, skips); relay-core gains always-on perception (WGC frame stream, settle events, UIA events, overlay); text-first observations to Claude; models warm for the whole task; optional uiAccess helper; macOS AX wake-up attributes. Decisions 9-12 added for the plan thread.

## 2026-10-14 · App discovery
- `list_apps` and `launch_app` (`relay_screen/apps.py` pure, `apps_windows.py` for Start menu, App Paths, registered browsers and window tracking). `"browser"` picks the owner's browser even though the https handler still names EdgeCore. Design: D-030.
- On-PC check: `uv run python scripts/check_apps.py` (discovery, Notepad by name in front, browser with a URL). `check_uia.py` finds Brave Origin through the catalog.

## 2026-10-14 · Final architecture proposal
- Owner sent a revised brief (Rust always-on core, speed ladder, local model table, cross-platform). Wrote [proposals/final-architecture.md](proposals/final-architecture.md): Rust relay-core daemon wraps the Python relay-screen, warm MCP over localhost HTTP, extended speed ladder, Brave extension for the browser lane, two 8B vision models resident at Q4/Q5, Windows through v1.
- decisions.md and architecture.md untouched; section 9 lists the D-entries for the plan thread to adopt.

## 2026-10-14 · M1-08 OCR and the next step, app discovery
- `find_text` (`relay_screen/ocr.py` pure, `ocr_windows.py` on the WinRT OCR projections): tiles wide displays, upscales small regions, fuzzy phrase matching, centers in screenshot px (D-029). On-PC check: `uv run python scripts/check_ocr.py` (Save dialog buttons, median latency).
- Owner: Brave Origin is their only browser (D-028). Owner asked that Relay find installed tools by itself instead of hard-coded paths: next is app discovery (`list_apps`, `launch_app`, default browser).

## 2026-10-14 · M1-07 UI Automation tools
- `uia_tree`, `uia_find`, `uia_invoke`, `uia_set_value` (`relay_screen/uia.py` pure, `uia_windows.py` on the `uiautomation` package, Apache-2.0). Element ids, one COM thread, output caps: D-026. Screen-reader flag from D-017 is set while the server runs.
- On-PC check: `uv run python scripts/check_uia.py` (Notepad File > Save without vision, Edge page links with bounds, tree size).
- Live check on the owner's PC (3 monitors): ALL PASS after fixes. The check now targets Notepad by title (it opens behind the foreground window), needs a real "Save as" Window, and finds Edge under `EdgeCore\<version>` (no `Edge\Application\msedge.exe` on this PC; Brave Origin needs a licence in a fresh profile). Listings name the display of off-screen elements and skip layout noise: D-027. `claude -p` with `uia_tree` works end to end.
- Owner: Brave Origin is their only browser (D-028). The check now opens a new Brave Origin window in their own profile and closes just that window. ALL PASS (link with bounds, tree ~2.6k tokens). Throwaway Edge/Chrome remains the fallback elsewhere.

## 2026-10-14 · M1-06 safety layer
- Owner asked for more autonomy: verify on the PC, fix, push and merge into `dev` without waiting on them (memory: relay-autonomy). PR #22 (verdict polling, D-024) from the PC session was merged that way.
- Built `relay_screen/safety/`: kill switch (corner watcher, Ctrl+Alt+Pause, guarded input backend), approval gate with Relay's own prompt window (ignores injected input, D-019), `approve` tool for headless runs, `rearm` and `safety_status` tools, audit log in `runs/`. Design in D-025, which amends D-011: Relay's prompt is the gate, and act tools are allowlisted in Claude Code.
- Next: live check on the PC (prompt appears, Deny stops, corner stops a batch, log written), then M1-07 UI Automation.

## 2026-10-14 · Change verdict fix found by a live check
- Running `scripts/check_batch.py` on the owner's PC: Notepad opened, but Start, typing "notepad" and Return were all reported as "no visible change". A probe showed capture was fine (Start covers ~19% of the primary display within 0.25 s); the single 0.15 s look was too early for shell animations, and Escape closing Start was missed the same way.
- Fix (D-024): the verdict polls every 0.1 s up to 0.75 s and returns on the first change. Two unit tests added; ruff, format, pyright and 124 tests pass; live re-run reports "screen changed" for every step.

## 2026-10-14 · M1-05 computer toolset tools and batch
- PR #21: all 17 `computer_toolset_20260801` members are relay-screen tools; `batch` stops at the first failure and answers the rest with the exact halt text; acts report "target changed", "screen changed" or "no visible change" (D-018). `Computer.before_act` is the hook for M1-06's approval gate and kill switch.
- Verified on the owner's PC with `uv run python scripts/check_batch.py`: Start menu, type "notepad", Return opened Notepad (Windows 11 Notepad restores unsaved tabs, so it showed the earlier typing test's text).
- Next: M1-06 safety (kill switch, approve tool, audit log).

## 2026-10-14 · M1-02..04 verified on the owner's PC
- PR #19 merged (displays, screenshots, input). All three monitors capture correctly; cursor moves land with 0 px error.
- Live typing found two bugs fixed in the follow-up PR: Windows 11 Notepad garbles Unicode packets for letters (now real key presses, D-023) and emoji crashed `VkKeyScanW`. `scripts/check_input.py` now types into its own Tk box and compares read-back.
- Next: M1-05 (computer toolset member tools + batch, with D-018 change verdicts), then M1-06 safety.

## 2026-10-14 · M1-01 done, M1-02..04 built
- PR #16 (plan + scaffolding) merged into `dev`; repo cloned to `C:\Projects\relay` on the owner's PC; headless `claude -p` called `mcp__relay-screen__ping` successfully. Owner still needs to accept the folder trust prompt in `C:\Projects\relay` once.
- Built display geometry/scaling, screenshot + zoom + list/select display tools, and the SendInput input layer (not yet exposed as tools; that is M1-05).
- Manual desktop check: `uv run python scripts/check_input.py` (moves the mouse, types into Notepad).

## 2026-10-14 · Planning and M1 kickoff
- Read the project brief and the owner's screenshots (PC specs, drives, repo, Violoop site, installer, release notes, app).
- Read current docs: `computer_toolset_20260801` (platform.claude.com) and Claude Code headless/MCP/permissions/hooks (code.claude.com). Notes in [research/claude-integration.md](research/claude-integration.md).
- Read-only scan of the owner's PC (with permission): Violoop is Electron 40 + Vue 3 + ant-design-vue; machine facts in [setup.md](setup.md#owners-machine-2026-10-14).
- Researched local GUI-grounding models and runtimes: [research/local-models.md](research/local-models.md).
- Wrote architecture, decisions D-001..D-016, roadmap, M1 plan; created `dev` from `main`.
- Next: M1-01 scaffolding (in progress on branch `claude/relay-m1-plan-6zr3a1` -> PR into `dev`).
