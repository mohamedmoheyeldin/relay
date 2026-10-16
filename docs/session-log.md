# Session log

Short entries so later sessions can pick up where earlier ones stopped. Newest first.

## 2026-10-16 · Fixes from the 0.1.10 test (D-092)
- `state` now reports the topmost Brave window and its address even when another program, such as a PowerShell window, is in front. It also never answers with an empty foreground before the first poll.
- `relay-screen runtime try holo-3.1-9b` turns thinking off and strips think blocks, so the check returns text.

## 2026-10-16 · App side: browser lane removed, Holo3.1-9B by default
- Brave is driven through UI Automation now (D-089), so the Playwright browser lane is gone: no relay-browser MCP entry, no extension token step, no Browser page in Settings or setup. On start Relay removes the old relay-browser entry, its paused entry and the stored token.
- The installer no longer adds the extension by policy. Install and uninstall both remove the ExtensionInstallForcelist entry earlier versions added, keeping the list gap-free.
- Setup's suggested download is Holo3.1-9B Q5_K_M alone (about 7.4 GB). Holo2-8B and Holo2-4B remain the fallbacks for smaller GPUs.
- The "local AI can't run yet" message says the model is still downloading, not that the owner must download one.
## 2026-10-16 · Dependabot alerts, issue cleanup, work on dev
- `postcss-selector-parser` (via `@tailwindcss/typography` 0.5.20, the latest, which pins 6.0.10) is forced to 7.1.6 with a pnpm override in `relay-ui/package.json`. The built CSS is byte-identical.
- `glib` 0.18.5 in `Cargo.lock` comes only from Tauri's Linux GTK stack (`cargo tree -i glib --target x86_64-pc-windows-msvc` is empty), so Windows builds never compile it. No upgrade exists until Tauri moves to gtk-rs 0.20+; the alert needs dismissing as "vulnerable code is not actually used" (a cloud session gets 403 on the Dependabot API).
- Owner rule: commit straight to `dev` for almost everything (no feature branches), run the checks, then merge `dev` into `main` and recreate `dev` from `main`.

## 2026-10-16 · The owner's words approve what they name (D-091)
- The owner tapped "Yes, my words" on the card. A task sent from the app now approves the risky steps it names, such as "submit" or "send", for that task only. Claude's hand-offs never grant anything.

## 2026-10-16 · Operator side complete for the first full build (D-089, D-090)
- Working straight on `dev` now (owner rule, 00:49).
- Browsers are read through UI Automation from the page's Document, with the address bar and usable controls only, so no extension is needed (D-089). UIA listings come from one CacheRequest per read.
- The app list comes from the Shell's AppsFolder over COM. PowerShell is only a fallback (D-090).
- A new task from the app re-arms the kill switch.
- Model requests ask for one JSON object with thinking off.
- The owner's-words approval waits on the owner's answer on a card in this thread.
- The Rust move of the operator (D-088 item 1) comes after the owner has the first full build. The JSON-lines interface stays the same.

## 2026-10-16 · Operator service and Holo3.1-9B (D-086..D-088)
- Owner: one complete app, not pieces; at most 2-3 local AIs for the screen; no permissions or setup; Claude only for heavy thinking. They then chose "Full rebuild" on the redesign card (D-088).
- Plan for the complete version: [complete-plan.md](complete-plan.md). Split agreed with the app thread through the coordinator: it builds the pill overlay, the zero-setup app and the one installer; this thread builds the operator, the models and the event interface.
- Built `relay-screen operator` (D-086) with:
  - task/say/stop/route/state/quit over JSON lines;
  - `ui_task` hand-off over localhost;
  - foreground and browser awareness;
  - ask-Claude through the CLI;
  - plain-words steps.
- Holo3.1-9B is the operator model (D-087).
- The owner's-words approval (D-088 item 4) is recorded but not built: the safety classifier stopped the change to the approval gate until the owner confirms it directly.
- Files, dev/main history and every remote branch are clean of the old reference name (the old branches were deleted at 00:45).

## 2026-10-16 · App side of the full release: popup, operator service, no-click setup
- Owner: no more installers piece by piece; one complete, working app with no permission prompts and nothing to set up, plus a small popup to talk to Relay while it works. The plan thread builds the always-on local operator; the app side lives here. Version is 0.1.10, not shipped yet.
- Floating popup (`popup.rs`, `screens/popup.tsx`): appears when Relay steps aside for screen work and opens anytime with Ctrl+Shift+Space. It shows the live step, a Stop button, and a box to redirect Relay. It doesn't take focus until its box is clicked, and it is excluded from screen capture (`WDA_EXCLUDEFROMCAPTURE`), so the operator never sees it.
- Operator service client (`relay-core/src/operator.rs`): one warm `relay_screen.server operator` process over JSON lines. Chat messages are routed first: screen tasks go to the operator and the rest to Claude. Popup follow-ups go into the running task with `say`, Claude's forwarded `ui_task` steps show in chat, and the app quits the service with Relay. Without the service, chat works as before.
- The wizard opens the chat by itself once Claude is connected.

## 2026-10-16 · Fast local operator, slice 1 (D-084, D-085)
- Owner: stop adding documents and make the app work, with millisecond actions driven by the local AI. Built the first slice of the redesign (`/mnt/project-files/relay/redesign/ui-automation-redesign.md` §5 step 2) in relay-screen.
- One-move tasks (open a site, open or switch to a program) run instantly with no model, even before the model has downloaded. Other tasks get text-only steps when UI Automation covers the window, batches of up to 6 actions per reply, and a `timing` event per step.
- The operator's llama-server now outlives each chat turn's relay-screen and is adopted by the next one, staying warm at least 15 minutes.
- 15 new tests. Nothing ran on the PC. Next: the app routes plain screen tasks straight to the operator and shows its steps live, and ships an installer. After that comes the model benchmark, then Tier 3 (operator asks Claude).

## 2026-10-15 · 0.1.9: the Playwright Extension installs itself
- Owner on 0.1.7: Relay should install the extension it needs without stopping the work. They chose on the decision card to let the installer set a browser policy (accepting "Managed by your organization").
- The elevated NSIS post-install hook adds the extension to `ExtensionInstallForcelist` for Brave (all channels, Brave Origin included, read `BraveSoftware\Brave`; checked in Brave Origin's chrome.dll), Chrome and Edge when installed. It uses the first free numbered slot, and a marker in `HKLM\Software\Relay` makes uninstall remove only Relay's entry, keeping the list gap-free.
- `browser::state` says the extension is on its way when the policy is set. The token stays a one-time paste: the extension makes it as the owner's OK, and Relay doesn't read it from the browser's storage.

## 2026-10-15 · 0.1.8: Relay Setup has no white border
- Owner on 0.1.7: the installer still had white borders. The screenshot shows 5 px of white on the left, right and bottom, none on top: tao keeps a resize frame around an undecorated window with a shadow, and Windows paints it white when the window isn't resizable. `win::hide_border` now subclasses the window so `WM_NCCALCSIZE` leaves no frame, and turns the frame's resize hit-tests into client hits.

## 2026-10-16 · Claude plans, the local AI does the screen work (D-081)
- Owner: tasks showed lots of PowerShell and Python, permission prompts and extension setup; they couldn't see the local AI working. Cause: Relay gave Claude Code its shell and file tools, relay-screen's direct click/key tools and the extension-based browser lane, so Claude scripted and clicked itself. Relay chats now run with those tools disallowed; Claude plans, looks and opens things, and every screen task goes to `ui_task`. Failed ui_tasks hand back a narrower goal instead of falling back to direct clicks.
- Nothing ran on the PC; the app thread ships it in the next installer.
- Recorded the app thread's 0.1.9 decisions: extension by browser policy (D-082), token stays a one-time paste (D-083).

## 2026-10-15 · Screen first and self-setup (D-079)
- Owner: Relay answered "go to dice.com and search" with a Dice connector table and asked them to install a browser extension. They want it to act on screen end to end and set itself up. Added `open_url` (owner's browser, no extension); Claude is told to do "go to" on screen and never to ask for setup; the local operator opens the page a task names before its first step; `ui_task` starts downloading the runtime and model in the background when they are missing. 9 new tests.
- The owner's reference video was blocked by the proxy and robots.txt; asked for the file. Moved onto the rewritten history. Recorded the app thread's 0.1.7 approvals as D-080 after reading the owner's yes. Nothing ran on the PC.

## 2026-10-15 · 0.1.7: no popups, plain-words steps, Relay steps aside
- Owner (own words, after 0.1.6): no popups, the app should just do what's needed; better messages in chat; white frame around the installer. They said yes to the plan in the thread.
- Approvals: the default is now "risky" (old settings files with the old "ask" default switch once, `safety.defaults_version`). Claude Code tool calls also follow it: lookups and ordinary tools run; tool names that send, pay, delete, and shell commands that delete, wipe, shut down or push still ask. Questions go to an in-app card (relay-screen `AppPrompter` → the app's localhost listener, token in `approval.json`); injected input to Relay's window is dropped so automation can't answer. The native prompt is only a fallback when the app isn't running. After an answer, focus goes back to the window Relay was working in.
- Chat shows each step in plain words (`relay-ui/src/lib/describe-tool.ts`); ToolSearch and status pings are hidden; raw calls open on click.
- Relay minimizes itself when a run starts acting on the screen and comes back when it ends (`stage.rs`).
- Relay Setup: Windows 11's 1 px DWM frame is turned off (`DWMWA_BORDER_COLOR` none).

## 2026-10-15 · Relay finds and focuses the program to control (D-078)
- Owner: the app should know where the program is and act on it by itself. In their run Relay screenshotted its own window, kept clicking the Brave taskbar button and Claude used PowerShell to find Brave. New `list_windows` and `focus_window` tools (Relay's own window marked and never chosen); the local operator sees the open windows each step, focuses the program the task names before its first step and has a `focus_window` action. 6 new tests.
- Sent the white border around the setup dialog and moving Relay's window aside during tasks to the app thread. Nothing ran on the PC.

## 2026-10-15 · 0.1.6: Claude-desktop-style look and Relay Setup
- Owner: the app looked bad; wanted it clean and modern like the Claude desktop app. Restyled relay-ui: warm neutrals with a terracotta accent, Inter for UI and a serif for replies, collapsible sidebar with New chat, centered chat column with a rounded composer and compact activity rows; settings and the wizard follow. No Anthropic assets.
- Shipped together with Relay Setup (below) as 0.1.6, recorded as D-076 (look) and D-077 (Setup bootstrapper). In-app approvals and auto-allowed lookups are parked on a local branch until the owner answers in their own words.

## 2026-10-15 · Relay Setup: an installer app with Relay's look
- Owner: the installer should notice an older Relay and update or uninstall it, install fresh otherwise, and look like the app. `dist/Relay-Setup.exe` is now Relay Setup (`relay-setup/`, Tauri 2, static HTML/CSS/JS in the app's warm palette, light and dark). It reads `HKLM` and `HKCU` `...\Uninstall\Relay` (`DisplayVersion`, `InstallLocation`, `UninstallString`, as Tauri's NSIS template writes them) and offers Install, Update (old → new) or Uninstall, Repair or Uninstall, or Uninstall only when a newer Relay is installed.
- The stock NSIS installer rides inside as the payload: unpacked to %TEMP% and run with `/S` through ShellExecuteExW "runas" (one UAC prompt), waited on. Uninstall re-launches Relay Setup elevated, which runs the uninstaller in place (`/S _?=<dir>`) and removes the folder. A running Relay (and its relay-screen Python) is closed first. The raw installer is kept as `dist/Relay-Setup-nsis-<ver>.exe`, and the app's updater skips it.
- MinGW builds embed WebView2Loader.dll and load it at run time, so the exe is one file; `build_installer.py` checks the import table. Not run on the owner's PC yet. Design notes were sent to the coordinator for a D-entry.
## 2026-10-15 · Removed the outside-product name from the project
- Owner asked to remove every mention of the hardware operator product that Relay was first compared with. Rewrote README, CLAUDE.md, architecture, decisions, the final-architecture proposal, setup and this log to describe Relay on its own terms; the reasoning behind each decision stays.
- Replaced the product research file with [research/screen-lane-notes.md](research/screen-lane-notes.md), which keeps only Relay's design takeaways.
- Git history still has the old text; it was left as is.

## 2026-10-15 · Risky-only approval mode, local operator reads text (D-073)
- Owner: still hasn't seen the local AI do UI tasks; too many popups. New approval mode "risky" asks only before clicks/invokes on controls named like send, pay, delete (UIA name under the pointer) and risky browser-lane calls; "ask" stays the default until the owner picks "risky". The local operator now also gets the installed apps, OCR text when UIA sees little, and a `click_text` action. 9 new tests.
- Nothing ran on the PC (no access from the cloud session, and the owner didn't ask).

## 2026-10-15 · The local AI owns UI automation (D-069)
- Owner: Relay is a desktop operator done in software and should beat hardware operators; the local AI's job is UI automation, everything else goes to Claude. Recorded as D-069 and rewrote architecture §1. New relay-screen tool `ui_task` lets Claude hand a whole UI task to the local operator (model kept loaded in the MCP server, unloaded when idle, one task at a time). 3 new tests.
- App side (via the coordinator): Claude's instructions in the app should tell it to use `ui_task` for UI work.

## 2026-10-15 · 0.1.5: Relay uses the owner's open Brave; replies stream
- Owner on 0.1.4: Relay opened a second Brave on its own profile (Brave Origin's purchase screen), not the Brave they had open, and tasks felt slow (a Dice search took 41.5 s and asked to approve the lookup).
- The browser lane now drives the browser the owner already has open by default (`browser.use_own`), through Playwright MCP's `--extension` mode and the Playwright Extension. Chromium refuses remote debugging of the everyday profile; the extension bridges it, and Claude works in its own tab group with the owner's sign-ins. The extension's token sits in Credential Manager and reaches Playwright through Relay's launcher, so no tab picker shows. Settings > Browser and the wizard show the two one-time steps (add the extension, paste the token); the old Relay-profile mode is a toggle away.
- Claude's text now streams into the chat as it is written (`--include-partial-messages`); before, a long answer only appeared when finished.
- Not changed: lookups through connectors still ask for approval. Letting `search_`/`get_`/`list_` tools run without asking loosens the approval gate, so it was left for the owner to decide.
- Not run end to end on the PC: the extension has to be added and its token pasted by the owner. Recorded as D-074 (browser lane via the Playwright Extension) and D-075 (streaming replies).

## 2026-10-15 · 0.1.4: Claude sign-in fixed, chat falls back to the local AI
- Sign-in never printed anything: portable-pty's ConPTY (PSEUDOCONSOLE_INHERIT_CURSOR) sends `ESC[6n` and blocks until it gets a cursor report. `pty.rs` now answers `ESC[1;1R` (verified on the owner's PC: `claude --version` printed only `ESC[6n` before, the version after). An expired sign-in now shows "Sign in again" in chat and on the Claude page.
- Chat fallback (D-067 app side): if Claude isn't installed, or a turn fails to authenticate or reach the network before any tool ran, the same message goes to `python -m relay_screen.server agent --json` (hidden, same env as relay-screen). Steps show under "Local AI (Claude unavailable)"; Stop kills it. Chat input works while Claude is signed out. Settings > Local AI has a "Local operator model" picker (`local_ai.agent`), and the setup wizard starts the suggested download (Qwen3-VL-8B on the owner's GPU) on its own.
- Not run end to end on the owner's PC yet: it needs Qwen3-VL-8B downloaded there. Recorded as D-070 (pty cursor answer), D-071 (expired sign-in and chat fallback, Claude told to use `ui_task`) and D-072 (operator model picker, setup auto-download).

## 2026-10-15 · Local operator: UIA tools and a 95% benchmark (D-068)
- Owner: the local AI should fully automate the UI with other tools too, at 95% or better. The local operator now gets the window's UI Automation element list each step and can `invoke`/`set_value` by element id. `relay-screen agent-bench` runs 9 fixed desktop and browser tasks with the local model only and checks each from outside the agent (window title, UIA text, file contents); results go to `%LOCALAPPDATA%\Relay\bench`. Recorded as D-068: 95% is a measured target, folded into M1-11.
- Not run yet: it opens windows, so it waits for the owner to ask. 7 new tests.

## 2026-10-15 · Local operator: UI tasks without Claude (D-067)
- Owner: UI automation must work with the local AI alone; nothing worked while their Claude login was expired. Added `relay_screen/agent/`, a screenshot → local VLM → one JSON action loop that runs through the same `Computer` (kill switch, approvals, audit log), and `relay-screen agent "task" [--json]` for the app. Model from `local_ai.agent`/`describe`/`grounding`, else Qwen3-VL-8B-Instruct. 14 new tests with a scripted model and fake hands.
- Not run on the owner's PC yet (needs Qwen3-VL-8B downloaded). App side, sent via the coordinator to the app thread: route chat to `relay-screen agent --json` when Claude is unavailable.

## 2026-10-15 · Approval prompt without tkinter, 0.1.2 (app thread)
- Owner's Dice task: every relay-browser call was refused with no prompt shown. The bundled CPython (NuGet package) has no tkinter, so `safety/prompt.py` crashed on import and the gate read that as "deny". The prompt is now plain Win32 through ctypes (same hooks, buttons, preview, timeout, focus restore). `prompt --check` builds it off screen; the self-test runs it as "Approval prompt" (passed on the owner's PC with the bundled Python, no window).
- A prompt that can't open now raises `PromptUnavailableError`, and Claude is told the owner was not asked, instead of "The owner denied this tool call."
- `browser_navigate` and `browser_navigate_back` need no prompt; other browser tools ask, with a third button "Allow the browser for this request" (lasts one `claude -p` run).
- Merged as Relay 0.1.2 (#60 into `dev`, #61 into `main`), recorded as D-064 (Win32 approval prompt plus its `--check`), D-065 (prompt unavailable is not a denial) and D-066 (navigation runs freely, plus the per-request browser button). Issues: #35 and #36 closed. #35's remaining work is #62, which needs the owner's `gh auth login` on the PC and a code-signing decision. #37 waits on the M1-11 benchmark, a UI-Venus source and custom models loaded from disk.
- Live test on the owner's PC (owner asked "make sure it works"; ran a test copy from Downloads because the elevated install was blocked from the session): navigate went through with no prompt, the prompt rendered correctly, a relay-screen screenshot + list_apps task worked in 13 s. Brave Origin turned out to be a paid build that shows "restore your purchase" in every new profile, so relay-browser stalled; the lane now detects `brave.origin.purchase_validated` in Relay's profile and Claude is told. Replies now render as Markdown. 0.1.3. Still needs the owner: entering the purchase ID in Relay's profile, and a real Allow click (injected and UIA input can't answer the prompt by design).

## 2026-10-15 · Per-machine install (app thread)
- Owner asked for `C:\Program Files\Relay`: the installer is now per machine with one admin prompt and removes the per-user 0.1.0 copy and its HKCU keys first. Merged in #56 (`dev`) and #57 (`main`); recorded as D-063.

## 2026-10-15 · App round 2 from the owner's first install (app thread)
- Chat tool calls all failed with "Permission prompt tool returned an invalid result": relay-screen's `approve` returned FastMCP `structuredContent`; it now returns one bare text block (test asserts it). Browser-lane tools that only look (snapshot, screenshot, console, network, wait) are allowed without a prompt.
- Self-test hung on "asking Claude": the check now times out after 90 s, child processes are killed on drop, and only one check runs at a time. `claude -p` itself answered in about 3 s on the owner's PC.
- `\\?\` is stripped from the bundle and launcher paths (`paths::plain`), so relay-screen re-registers with `C:\relay\python\pythonw.exe`.
- Setup is one page of checks that run on their own; sign-in runs `claude auth login --claudeai` out of sight, opens the page, accepts a pasted code and polls `claude auth status` (xterm removed).
- Browser lane: `relay-browser` = pinned `@playwright/mcp@0.0.83` with `--executable-path` of the chosen Chromium browser (Brave Origin auto-picked) and Relay's own profile in `%LOCALAPPDATA%\Relay\browser-profile`. Settings > Browser shows it; decision sent to the plan thread.
- Connections: Sign in/out for OAuth servers (`claude mcp login/logout`), plugin servers name their plugin and can turn it off, and a plugins list toggles `claude plugin enable/disable`.
- Design: app-styled scrollbars, container-query layouts down to 520 px wide, page and row motion, skeletons, fixed the close dialog.
- Merged as Relay 0.1.1 (#52 into `dev`, #53 into `main`). Recorded as D-060 (Playwright MCP browser lane on Relay's own profile), D-061 (read-only browser tools skip the prompt; `approve` returns plain text) and D-062 (hidden `claude auth login` sign-in). `build_installer --skip-bundle` now always reinstalls relay-screen. Next from the app thread: default install folder `C:\Program Files\relay` (per-machine install).

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
- Owner wants a complete, polished app: one installer, local model chosen in Settings, all MCP connections managed in one place. Recorded as D-044 (one installer + first-run wizard), D-045 (Relay sets up Claude Code with Anthropic's official installer and an in-app sign-in), D-046 (model picker), D-047 (MCP hub over `claude mcp`), D-048 (all settings in the app) and D-049 (live PC checks only on request). Roadmap rewritten around v1.0 = clean install from `Relay-Setup.exe`.

## 2026-10-14 · More architecture decisions
- Adopted D-040 to D-043 from the revised proposal (#31): always-on perception in relay-core (WGC frame stream, change events, UIA watcher, overlay), text-first observations to Claude, models load at task start and unload 60 s after it ends, optional signed uiAccess helper for elevated windows.

## 2026-10-14 · Merge rule and final architecture adopted
- Owner: no more draft PRs; Claude merges its own PRs into `dev` and then `dev` into `main` on green CI (D-031). Owner stopped live PC checks that kept opening windows: run them only when asked.
- Adopted the final architecture proposal as D-032 to D-039 (Rust always-on core wrapping the Python hands, warm MCP over localhost HTTP, speed ladder, Brave extension browser lane, two vision models resident, runtime in the core from M2, Windows through v1, D-001 confirmed).

## 2026-10-14 · Final architecture: how to get smooth control
- Owner asked to rethink the proposal around smooth Windows/macOS automation: routing table, HDMI/HID capture, models, text-only cloud summaries.
- Revised [proposals/final-architecture.md](proposals/final-architecture.md): new section 2 (what makes control smooth, what Relay adopts, improves, skips); relay-core gains always-on perception (WGC frame stream, settle events, UIA events, overlay); text-first observations to Claude; models warm for the whole task; optional uiAccess helper; macOS AX wake-up attributes. Decisions 9-12 added for the plan thread.

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
- Read the project brief and the owner's screenshots (PC specs, drives, repo, installer, release notes, app).
- Read current docs: `computer_toolset_20260801` (platform.claude.com) and Claude Code headless/MCP/permissions/hooks (code.claude.com). Notes in [research/claude-integration.md](research/claude-integration.md).
- Read-only scan of the owner's PC (with permission); machine facts in [setup.md](setup.md#owners-machine-2026-10-14).
- Researched local GUI-grounding models and runtimes: [research/local-models.md](research/local-models.md).
- Wrote architecture, decisions D-001..D-016, roadmap, M1 plan; created `dev` from `main`.
- Next: M1-01 scaffolding (in progress on branch `claude/relay-m1-plan-6zr3a1` -> PR into `dev`).
