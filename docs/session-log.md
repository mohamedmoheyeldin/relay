# Session log

Short entries so later sessions can pick up where earlier ones stopped. Newest first.

## 2026-10-14 · M1-07 UI Automation tools
- `uia_tree`, `uia_find`, `uia_invoke`, `uia_set_value` (`relay_screen/uia.py` pure, `uia_windows.py` on the `uiautomation` package, Apache-2.0). Element ids, one COM thread, output caps: D-026. Screen-reader flag from D-017 is set while the server runs.
- On-PC check: `uv run python scripts/check_uia.py` (Notepad File > Save without vision, Edge page links with bounds, tree size).
- Live check on the owner's PC (3 monitors): ALL PASS after fixes. The check now targets Notepad by title (it opens behind the foreground window), needs a real "Save as" Window, and finds Edge under `EdgeCore\<version>` (no `Edge\Application\msedge.exe` on this PC; Brave Origin needs a licence in a fresh profile). Listings name the display of off-screen elements and skip layout noise: D-027. `claude -p` with `uia_tree` works end to end.

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
- Read the project brief and the owner's screenshots (PC specs, drives, repo, RefBox site, installer, release notes, app).
- Read current docs: `computer_toolset_20260801` (platform.claude.com) and Claude Code headless/MCP/permissions/hooks (code.claude.com). Notes in [research/claude-integration.md](research/claude-integration.md).
- Read-only scan of the owner's PC (with permission): RefBox is Electron 40 + Vue 3 + ant-design-vue; machine facts in [setup.md](setup.md#owners-machine-2026-10-14).
- Researched local GUI-grounding models and runtimes: [research/local-models.md](research/local-models.md).
- Wrote architecture, decisions D-001..D-016, roadmap, M1 plan; created `dev` from `main`.
- Next: M1-01 scaffolding (in progress on branch `claude/relay-m1-plan-6zr3a1` -> PR into `dev`).
