# M1 plan: relay-screen MCP server working with Claude Code

Weeks 1-2. Pure vision + UIA, approval on every state change.

**Demos that close M1**
1. "Open Notepad, type text, save the file" driven by Claude Code through relay-screen.
2. The canonical form flow on one test site (a local test form shipped in
   `tests/e2e/site/`), up to and including the approval before Submit.

Each item below is a GitHub issue labelled `M1`. Order is the build order;
items 2-4 can run in parallel after 1.

---

### M1-01 Monorepo scaffolding, tooling and CI
- uv workspace at the root with `relay-screen` as the first member; Python 3.12 pinned.
- `relay-screen` package skeleton with `relay-screen` console entry point and a stdio MCP server that starts and lists one `ping` tool.
- ruff (lint + format), pyright (basic), pytest configured; `uv run pytest` passes.
- ~~GitHub Actions on `windows-latest`~~ removed by D-050: `scripts/check.py` runs lint, type check and tests locally.
- `.mcp.json` registers relay-screen for Claude Code in this repo; `.claude/settings.json` extended (dev commands + read-only relay-screen tools allowed).
- `CLAUDE.md` with project conventions and pointers to docs.

**Acceptance:** fresh clone -> `uv sync` -> `uv run pytest` green locally and in CI; `claude mcp list` (in the repo) shows `relay-screen` connected; calling `ping` from Claude Code returns `pong`.

### M1-02 Display geometry, DPI awareness and coordinate scaling
- Per-monitor-v2 DPI awareness set before any capture/UI import.
- Enumerate displays (id, name, physical rect in virtual-desktop coords, scale, primary).
- Pure scaling functions: screenshot px <-> physical px <-> virtual desktop, for any long-edge budget; rounding and clamping rules.
- `list_displays`, `select_display` tools; default active display = foreground window's monitor.

**Acceptance:** unit tests cover 3440x1440, 2560x720, 1920x1080 at 100/125/150% scale and a display placed below/left of primary (negative coords); round trip error <= 1 physical px; on the owner's PC `list_displays` reports all three monitors correctly.

### M1-03 Screen capture: screenshot and zoom
- `screenshot`: capture active display with mss, resize to the budget (default long edge 1568, max 2576 / 3.75 MP), return PNG image content.
- `zoom(region)`: region in screenshot px -> capture physical region at full resolution -> fit to the same budget, aspect preserved.
- Records the scale of the last screenshot per display for later coordinate mapping.

**Acceptance:** screenshot of a 3440x1440 display returns 1568x656; zoom of a 200x100 screenshot-px region returns a sharp image; result stays under Claude Code's MCP output cap; capture of the iGPU-driven display works.

### M1-04 Input layer: SendInput mouse and keyboard
- ctypes SendInput: absolute move (virtual desk normalization), button down/up, clicks (single/double/triple), wheel (vertical/horizontal), drag.
- Key-name parser for toolset syntax (`Return`, `ctrl+s`, `alt+Tab`, `super`), `repeat`, `hold_key(duration)`; modifier `text` on clicks/scroll.
- Unicode `type` via `KEYEVENTF_UNICODE`, chunked, with small inter-chunk delay.
- Detects and reports UIPI failures (foreground window elevated) as tool errors.

**Acceptance:** unit tests for key parsing and event sequences (SendInput mocked); manual check script types `Hello, Relay ✓` into Notepad and clicks precise targets on every monitor.

### M1-05 Computer toolset MCP tools and `batch`
- All 17 members with toolset names/parameters, coordinates in screenshot px of the active display.
- `batch(actions)`: in-order, stop at first failure, exact halt text `Not executed: an earlier computer action in this turn failed.` for the rest.
- Input validation errors are clean tool errors, never crashes.

**Acceptance:** in-process MCP client tests for every member and for batch (success, failure in the middle, validation error); Claude Code can open Notepad via `key super` + `type notepad` + `key Return` in one batch.

### M1-06 Safety: kill switch, approval gate, audit log
- Kill switch: top-left corner watcher + Ctrl+Alt+Pause; aborts current batch, disarms until re-armed (`rearm` tool or tray later).
- Action classification read/act; M1 policy = every act needs approval.
- `approve` tool implementing the `--permission-prompt-tool` contract (`{"behavior":"allow","updatedInput":...}` / `{"behavior":"deny","message":...}`) with a native Windows prompt.
- `.claude/settings.json`: read tools allowed, act tools left to prompt.
- Audit log: `runs/<run-id>/actions.jsonl` plus before/after thumbnails.

**Acceptance:** moving the mouse to the corner during a 10-step batch stops it within one action and returns the halt text for the rest; headless `claude -p` run with `--permission-prompt-tool` asks before each act; every action appears in the log.

### M1-07 UI Automation tools
- `uia_tree(window?, depth, filter)`: compact tree (role, name, automation id, enabled, bounds in screenshot px).
- `uia_find(query)`: by name/role/automation id; returns center coordinate in screenshot px.
- `uia_invoke`, `uia_set_value` (Invoke/Value patterns) as act tools.
- Output trimmed to stay well below the MCP token cap.

**Acceptance:** finds and invokes Notepad's File > Save without vision; returns usable bounds for Edge page controls; tree of a large window stays under 8k tokens.

### M1-08 OCR: `find_text`
- Windows.Media.Ocr over the active display or a region; returns words/lines with boxes in screenshot px; fuzzy match option.

**Acceptance:** finds button labels on the test form and in Notepad's Save dialog; median latency on a 3440x1440 display reported in the PR.

### M1-09 Local model runtime manager
- Download pinned llama.cpp CUDA build + model files to `%LOCALAPPDATA%\Relay\` with resume and SHA-256 verification; progress reported.
- Start on demand on a private port with full GPU offload; stop after 60 s idle; NVML free-VRAM check before load; never spill to system RAM.
- `relay-screen doctor` CLI: driver/CUDA, VRAM, runtime, models, Claude Code version and login state.
- Moves into the Rust relay-core in M2 (D-037).

**Acceptance:** cold start -> first answer measured and logged; after 60 s idle `nvidia-smi` shows the VRAM released; doctor output readable and correct on the owner's PC.

**Built (D-052):** `relay_screen/runtime/`: resumable SHA-256-checked downloads, llama.cpp and Hugging Face sources, `installed.json`, a supervisor (one llama-server per model, NVML check, task holds per D-042, 60 s idle unload) and the CLI (`relay-screen doctor`, `runtime install`, `runtime try MODEL`, `models list/download/remove`). `runtime try` covers the acceptance run: it times the cold start and first answer, waits for the idle unload and checks the VRAM came back. Still to do on the owner's PC, when they ask: `runtime install`, download Qwen3-VL-8B, `runtime try`, `doctor`.

### M1-10 Local VLM tools: `locate`, `describe`, `verify`
- Per-model adapters (prompt format and coordinate convention: absolute vs 0-1000) behind one interface.
- `locate(description)` -> coordinate in screenshot px + confidence; uses the model's preferred input resolution.
- `describe(region?)` and `verify(expectation)` -> `{ok, reason}`.

**Acceptance:** on the test form, `locate` hits every input/button; `verify("Save as dialog is open")` correct on positive and negative cases.

### M1-11 Grounding benchmark and model decision
- Small eval set: ~60 labelled targets from the owner's own apps (3440x1440) + the test site + a ScreenSpot-Pro subset.
- Compare UI-Venus-1.5-8B, Holo2-8B, Holo2-4B, Qwen3-VL-8B at Q4_K_M and Q8_0: accuracy, latency, VRAM peak, swap time.
- Also measure both models resident together at Q4/Q5 (D-036).
- Update D-005/D-036 with the result and set defaults in config.
- Run `relay-screen agent-bench` per model: the local operator's task success rate against the owner's 95% target (D-068).

**Acceptance:** results table committed under `docs/research/`; D-005 marked accepted with the chosen default(s).

### M1-12 Playbooks v0
- Format (markdown + YAML front matter) per architecture §7; `playbook_list/load/save` tools; example playbook for Notepad under `playbooks/examples/`.

**Acceptance:** after the Notepad demo, Claude saves a playbook; a second run reads and follows it.

### M1-13 Demo 1: Notepad end to end
- Script `scripts/demo-notepad.ps1` runs `claude -p` with relay-screen, approval prompts on, and saves the run log.

**Acceptance:** file saved with the requested text; run log + final screenshot attached to the PR.

### M1-14 Demo 2: test form flow
- Local test site (`tests/e2e/site/`, served by `python -m http.server`): fake login page, fact fields, one judgment textarea, Submit, confirmation page.
- `config/profile.example.yaml`; Claude fills fact fields from it, writes the judgment field, pauses for approval before Submit, saves proof screenshot and playbook.

**Acceptance:** recorded run reaches the confirmation page with correct values; login step waits for the owner; Submit only after approval.

### M1-15 Setup guide and promotion to main
- `docs/setup.md` complete for a clean Windows 11 PC; README quick start verified.
- Merge `dev` -> `main` and tag `v0.1.0`.

**Acceptance:** setup followed from scratch on the owner's PC works; tag pushed.
