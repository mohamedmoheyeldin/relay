# Setup (developer, M1)

Relay is personal software for one Windows 11 PC. These steps get the M1
relay-screen server running against Cloud AI. The M3 installer will replace
most of this.

## Requirements
- Windows 11, NVIDIA GPU with a recent driver (RTX 50 series needs a CUDA 12.8+ capable driver).
- The Cloud AI CLI installed and logged in with your own subscription (`cloud-cli --version`, then run `cloud-cli` once to log in).
- [uv](https://docs.astral.sh/uv/) (`winget install astral-sh.uv`). uv installs the pinned Python 3.12 itself; your system Python is not used.
- git.

No Ollama, CUDA toolkit or system Python needed: Relay downloads its own model runtime and weights on first use (M1-09).

## Steps
```powershell
cd C:\Projects
git clone https://github.com/mohamedmoheyeldin/relay
cd relay
git switch dev
uv sync
uv run pytest
cloud-cli        # in the repo: approve the project MCP server "relay-screen" when asked
```
Inside Cloud AI, `/mcp` should list `relay-screen` as connected.

## Local model runtime (M1-09)
```powershell
uv run relay-screen doctor                              # GPU, CUDA, VRAM, runtime, models, Cloud AI
uv run relay-screen runtime install                     # llama.cpp CUDA build, about 0.5 GB
uv run relay-screen models                              # the catalog and what is downloaded
uv run relay-screen models download qwen3-vl-8b-instruct
uv run relay-screen runtime try qwen3-vl-8b-instruct    # cold start, first answer, 60 s idle unload
```
Everything goes under `%LOCALAPPDATA%\Relay` (models under `models\` there, or
the folder chosen in Settings), or under `RELAY_HOME` if that is set. No window
opens: llama-server runs hidden and logs to `logs\`. What was installed and
its SHA-256 is in `installed.json` (D-052).

## Personal files (never committed)
- `profile.yaml` (copy from `config/profile.example.yaml`)
- `playbooks/` (except `playbooks/examples/`)
- `runs/` (audit logs and screenshots)
- `.env`

## Owner's machine (2026-10-14)
| Item | Value |
|---|---|
| CPU / RAM | Ryzen 9 9950X3D, 32 GB DDR5-6000 |
| GPU | RTX 5080 16 GB (driver 617.42, CUDA 13.4), ~13.5 GB free at idle; AMD iGPU |
| Monitors | DISPLAY2 3440x1440 (primary), DISPLAY1 3440x1440 (below), DISPLAY5 2560x720 (on iGPU); 100% scaling |
| OS | Windows 11 Pro 26H2, build 26300 |
| Tools | cloud-cli 2.1.296, Python 3.14.7 (system), node 26.8.1, git 2.55, winget 1.29; Ollama not installed |
| Browsers | Brave Origin is the only browser the owner uses (corrected 2026-10-14, D-028). The Edge browser is gone; `EdgeCore\<version>\msedge.exe` remains as a Windows component and the https handler still points at `MSEdgeHTM`. Chrome not installed |
| Paths | projects in `C:\Projects` |
