# Relay

**Local hands, cloud brain.**

Relay is a local-first AI desktop operator for Windows: a "second user" on your
PC that reads the screen and drives the keyboard and mouse to finish tasks for
you. Perception and execution run on your own GPU. Judgment (planning, writing,
recovering when stuck) comes from Claude through your own Claude Code
subscription login.

It is a software take on the [Violoop](https://violoop.ai) hardware operator,
built for one person's PC. It is not a product and is not meant to be shared
or run on anyone else's subscription.

> Status: **M1 in progress** (relay-screen MCP server). See [docs/roadmap.md](docs/roadmap.md).

## How it works

```
you ── chat ──▶ relay-ui ──▶ relay-core ──▶ claude -p (your subscription)
                                │                 │  MCP connectors: Gmail, Calendar, Drive, GitHub ...
                                │                 ▼
                                └──────────▶ relay-screen (MCP server)
                                              screenshot · UIA · OCR · local VLM
                                              click · type · keys · playbooks
                                              kill switch · approvals · audit log
                                                  │
                                       llama-server on your GPU (VRAM only, unloads when idle)
```

- **Services via MCP, pixels last.** Anything with an API goes through Claude Code connectors.
- **Cheapest target first:** API → Windows UI Automation → OCR → local vision model → Claude with a screenshot.
- **Playbooks, not training.** Every successful screen task is saved as a markdown playbook; the next run follows it without a cloud call.
- **You stay in charge.** You do every login, approve irreversible actions, and moving the mouse to the top-left corner stops Relay instantly.

Full design: [docs/architecture.md](docs/architecture.md). Why things are the way they are: [docs/decisions.md](docs/decisions.md).

## Repository layout

| Path | What |
|---|---|
| `relay-screen/` | MCP server: computer-use toolset, UI Automation, OCR, local VLM, safety (M1) |
| `relay-core/` | Orchestrator and router: service / screen / judgment lanes (M2) |
| `relay-ui/` | Tauri tray app and chat window (M3) |
| `playbooks/` | Your saved playbooks (gitignored except `examples/`) |
| `config/` | Example config (`profile.example.yaml`) |
| `tests/e2e/` | End-to-end demos and the local test form site |
| `scripts/` | Dev and demo scripts |
| `docs/` | Architecture, decisions log, roadmap, plans, research, session log |

## Quick start (developers, M1)

Requires Windows 11, an NVIDIA GPU, [Claude Code](https://code.claude.com/docs) logged in, and [uv](https://docs.astral.sh/uv/).

```powershell
cd C:\Projects
git clone https://github.com/mohamedmoheyeldin/relay
cd relay
git switch dev
uv sync
uv run pytest
claude          # approve the "relay-screen" project MCP server, then try: "take a screenshot"
```

Details: [docs/setup.md](docs/setup.md).

## Branches

`main` holds stable releases. All work lands on `dev` through feature-branch PRs and is promoted to `main` at milestone ends.

## Ground rules

- Claude is reached only through the official `claude` CLI with your own login. Relay never calls the Anthropic API and never touches your OAuth token.
- No credentials in the repo. Personal data (playbooks, `profile.yaml`, run logs, screenshots, `.env`) is gitignored.

## License

MIT. See [LICENSE](LICENSE).
