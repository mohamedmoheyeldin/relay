<div align="center">

<img src="relay-ui/src-tauri/icons/128x128.png" alt="Relay" width="96" height="96">

# Relay

**Local hands, cloud brain.**

A local-first AI desktop operator for Windows. Relay works your PC the way you
do, in any program or website, with a fast local AI doing the clicking and
Cloud AI doing the thinking.

![Platform](https://img.shields.io/badge/platform-Windows%2011-0078D4)
![Status](https://img.shields.io/badge/status-active%20rebuild-orange)
![Version](https://img.shields.io/badge/app-0.1.10-informational)
![License](https://img.shields.io/badge/license-MIT-green)

[How it works](#how-it-works) ·
[Status](#status) ·
[Getting started](#getting-started) ·
[Docs](docs/README.md) ·
[Decisions](docs/decisions.md)

</div>

---

## What Relay is

Relay is a second pair of hands for one Windows PC. You tell it what you want
in plain words ("open dice.com and search FDE jobs", "attach the PDF from
Downloads to a reply") and it does the work on your screen while you watch.

It runs on the PC itself, with full access to the hardware and software, and
needs no extra device:

- **An always-on local operator owns UI and OS automation.** It reads programs
  directly through Windows UI Automation and acts in milliseconds, using a
  local model on your own GPU only when it has to look at the screen.
- **Cloud AI handles only the heavy reasoning.** Planning a long task, writing
  the text that goes into a form, choosing between ambiguous results. Cloud AI is
  reached through the official Cloud AI CLI on your own subscription.
- **No permission prompts and no setup.** One installer sets everything up.
  Relay asks only before a step that can't be undone (send, pay, delete,
  submit), with a card in the chat.
- **A small floating popup** lets you hand Relay a task from anywhere and see
  each step as it happens.

Relay is built for one person on their own machine. It is not a hosted
service and is not meant to run on anyone else's subscription.

## How it works

```mermaid
flowchart LR
    you(["You<br/>chat or floating popup"]) --> app["<b>Relay app</b><br/>routes each request"]

    app -- "screen and OS work" --> op
    app -- "heavy reasoning" --> cloud

    subgraph op["<b>Local operator</b> · on your PC"]
        direction TB
        t0["<b>Tier 0</b><br/>instant moves, no model"]
        t1["<b>Tier 1</b><br/>plans over the UI Automation map"]
        t2["<b>Tier 2</b><br/>Holo3.1-9B vision on your GPU"]
        t0 --> t1 --> t2
    end

    subgraph cloud["<b>Cloud AI</b> · your subscription"]
        cloud["Cloud AI CLI<br/>+ MCP connectors"]
    end

    op --> pc(["Any Windows app<br/>or website"])
    op <-. "Tier 3: asks for judgment" .-> cloud
```

- **Reads the app, not the pixels.** Windows UI Automation gives exact names,
  roles, positions and actions for most programs, Chromium browsers included.
  Screenshots and the vision model are the fallback.
- **Several actions per model call.** Each is checked before it runs, and the
  batch stops at the first surprise.
- **Cloud AI thinks, Relay acts.** Cloud AI plans and writes; it never clicks.
- **Your browser, your logins.** Relay works where you are already signed in.
- **Stop any time.** Mouse to the top-left corner or Ctrl+Alt+Pause stops
  Relay instantly, and every action goes to a local audit log.

The full design is in [docs/architecture.md](docs/architecture.md) and every
non-obvious choice is recorded in [docs/decisions.md](docs/decisions.md).

## Status

Relay is being rebuilt around the local operator. The table is honest about
what runs today and what is still being built.

| Area | State |
|---|---|
| **relay-screen** MCP server: the computer-use toolset with batch, multi-monitor screenshots and zoom, UI Automation, Windows OCR, app and window discovery, `open_url`, kill switch, audit log | ✅ Built and checked on a real PC |
| **Local operator** (`ui_task`): instant Tier 0 moves, text-first Tier 1 over the element list, batches of up to 6 actions, per-step timing (D-084) | ✅ Built |
| Operator runs as a background service while Relay is open, model kept warm in VRAM, follow-ups and Stop while a task runs (D-085, D-086) | ✅ Built |
| **Holo3.1-9B** as the operator's model, for both text planning and screenshots (D-087) | ✅ Built, benchmark pending |
| Browsers driven through UI Automation with one cached read per page, no extension needed (D-089) | ✅ Built |
| App catalog read natively from the Shell, no PowerShell (D-090) | ✅ Built |
| **Floating popup**: live step, Stop and a box to redirect Relay, opened with Ctrl+Shift+Space | ✅ Built |
| **Desktop app** 0.1.10: chat with streamed replies, Cloud AI sign-in inside the app, local model picker and download, MCP hub, Settings, in-chat approval cards for irreversible steps | ✅ Built |
| Cloud AI limited to planning and reasoning, with no shell, scripts or direct clicks in chat (D-081) | ✅ Built |
| **Windows installer** (`Relay-Setup.exe`) that installs Cloud AI, the runtime and the model | 🔨 Final build waits for the rebuild |
| Removing the old browser-extension lane and its policy install | 🔨 In progress |
| Your own words count as approval ("send", "submit", "pay") (D-088) | 🔨 In progress |
| Operator moved into relay-core (Rust) with always-on live perception | 🔨 In progress |
| Skills recorded from successful runs and replayed at Tier 0 | 📋 Planned |
| Signed auto-updates | 📋 Planned |

The target is 95% task success on the operator benchmark
(`relay-screen agent-bench`, D-068), with typical tasks taking seconds.
See [docs/roadmap.md](docs/roadmap.md) for milestones.

## Getting started

### Requirements

- Windows 11
- An NVIDIA GPU (developed on an RTX 5080 with 16 GB of VRAM)
- A Cloud AI subscription; Relay's setup installs and signs in to Cloud AI
  for you

### Install

A one-click installer ships once the rebuild is complete. Until then, run
Relay from source.

### Run from source (developers)

You need [uv](https://docs.astral.sh/uv/) for Python 3.12, plus Rust and
[pnpm](https://pnpm.io/) for the app.

```powershell
git clone https://github.com/mohamedmoheyeldin/relay
cd relay
git switch dev
uv sync
uv run python scripts/check.py      # ruff, format, pyright, pytest (+ Rust and app checks when installed)

cd relay-ui
pnpm install
pnpm tauri dev                      # start the desktop app
```

To use relay-screen from Cloud AI directly, run `cloud-cli` in the repository
and approve the `relay-screen` project MCP server. More detail in
[docs/setup.md](docs/setup.md).

## Repository layout

| Path | What it holds |
|---|---|
| [`relay-screen/`](relay-screen/) | Python MCP server: computer-use tools, UI Automation, OCR, safety, and the local operator |
| [`relay-core/`](relay-core/) | Rust library: Cloud AI driver, MCP hub, model catalog, downloads and runtime, settings |
| [`relay-ui/`](relay-ui/) | Desktop app: Tauri 2 + React + TypeScript |
| [`relay-setup/`](relay-setup/) | The small setup app that wraps the installer |
| [`playbooks/`](playbooks/) | Saved task playbooks (only `examples/` is committed) |
| [`config/`](config/) | Example configuration |
| [`scripts/`](scripts/) | Local checks, desktop checks and the installer build |
| [`tests/e2e/`](tests/e2e/) | End-to-end demos and a local test form site |
| [`docs/`](docs/) | Architecture, decisions, roadmap, research and session log |

## Development

- Work happens on feature branches, merged into `dev` by pull request, then
  promoted from `dev` to `main` for stable versions.
- There are no GitHub Actions. `uv run python scripts/check.py` must pass
  before every push and merge (D-050).
- Windows-only code imports lazily so lint and tests run on any OS. Tests that
  need a real desktop are marked `@pytest.mark.desktop` and run only on request.
- New design decisions get a new entry in [docs/decisions.md](docs/decisions.md);
  old entries are never rewritten.

## Privacy and ground rules

- Cloud AI is reached only through the official Cloud AI CLI with your own
  login. Relay never calls the cloud AI's web API and never reads or stores your
  OAuth token.
- Screen work, perception and the local model stay on your PC.
- No credentials in the repository. Personal data (playbooks, `profile.yaml`,
  run logs, screenshots, `.env`) is gitignored.

## License

[MIT](LICENSE) © 2026 Mohamed Moheyeldin
