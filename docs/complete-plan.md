# Relay: plan for the complete version (2026-10-16)

The owner asked for one complete, working Relay rather than pieces. This plan
covers the whole app. It builds on the redesign
(`/mnt/project-files/relay/redesign/ui-automation-redesign.md`) and on
D-084/D-085. Each item lands on `dev`, and `main` and the installer follow once
everything is in.

## What the owner gets

- **Talk to Relay** in the chat or the floating pill (Ctrl+Shift+Space). The
  pill shows each step live and has Stop and a box for corrections.
- **Screen tasks go to the local AI at once.** Examples: open or switch to a
  program, go to a site, search, fill a form, move through an app. Cloud AI is
  not consulted first.
- **Cloud AI only for thinking.** Examples: what to write in a cover letter,
  which of several results fits, a plan for a long job. The local AI asks
  Cloud AI mid-task when it needs that, and gets text back.
- **No permissions, no setup.**
  - No extension, token, policy or PowerShell windows.
  - A card appears only before a step that can't be undone and that the owner
    didn't ask for.
  - Everything downloads in the installer.

## Local AI (at most three models, all for the screen)

| Role | Model | VRAM | Why |
|---|---|---|---|
| Operator brain: plans batches over the element list, reads screenshots when needed | **Holo3.1-9B** (H Company, Apache-2.0, Qwen3.5 base, Jun 2026), Q5_K_M + f16 mmproj | ~8 GB with KV | Built for fast local computer use with structured JSON actions; 9B is H Company's speed/accuracy balance; fits 16 GB with room for a second model |
| Windows UI Automation | OS | 0 | Names, roles and actions of every control; presses without moving the mouse |
| Windows OCR | OS | 0 | Text in windows UIA can't read |

That is one AI model, kept loaded while Relay runs, plus two OS engines.
Holo3.1-4B can be picked in Settings for speed. Qwen3-VL-8B stays in the
catalog as a fallback. A second resident model (a grounding specialist such
as GUI-Owl-1.5-8B) is added only if the benchmark shows Holo3.1-9B misses
targets.

## Pieces and owners

| # | Piece | Owner |
|---|---|---|
| 1 | Tiered operator: instant moves, batches, text first (D-084) | plan thread, **done** |
| 2 | Warm shared model server (D-085) | plan thread, **done** |
| 3 | Holo3.1-9B in the catalog and Setup; pixel coordinates on a 32-px aligned image (D-087) | plan thread, **done** |
| 4 | **Operator service** (`relay-screen operator`, D-086): JSON lines over stdio for the app, local socket for `ui_task`; task/say/stop/route/state/quit | plan thread, **done** |
| 5 | Background awareness in the service: foreground history, open windows, app catalog, browser address | plan thread, **done** |
| 6 | Cached UI Automation (one CacheRequest per read, uncached fallback); web pages listed from the document element (D-089) | plan thread, **done** |
| 7 | App catalog from the Shell's AppsFolder (COM), PowerShell only as a fallback (D-090) | plan thread, **done** |
| 8 | Ask-Cloud AI action from the operator (`cloud-cli -p`, no tools, text answer) | plan thread, **done** |
| 9 | Router: operator or Cloud AI, in the service | plan thread, **done** |
| 10 | Pill overlay, chat on service events, start/stop the service, route messages | app thread |
| 11 | Drop the browser lane, the extension policy (removed on update) and the token step | app thread |
| 12 | One installer: runtime + Holo3.1-9B download, no prompts | app thread |
| 13 | Operator moved into relay-core (Rust) behind the same interface (D-088) | plan thread |
| 14 | The owner's words count as approval (D-091) | plan thread, **done** |

## Order

3 → 4/5 → 8/9 (done) → 6/7 → 13 → docs and checks → main. The app thread builds 10–12
against the event shape in D-086 at the same time. The final installer comes
from `main`.

## Not doing now

- Recorded skills.
- Agent Workspace (a separate desktop).

These come after the owner has used the complete version.
