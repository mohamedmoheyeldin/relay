# relay-screen

Relay's MCP server for Claude Code. It mirrors the `computer_toolset_20260801`
members (screenshot, zoom, clicks, type, key, scroll ...) and adds UI Automation,
OCR, local vision (`locate`, `describe`, `verify`), playbooks and safety gates.
See `docs/architecture.md` §4 and `docs/m1-plan.md`.

```powershell
uv run relay-screen          # stdio MCP server (Claude Code starts it via .mcp.json)
uv run pytest
```
