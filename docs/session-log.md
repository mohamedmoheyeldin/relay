# Session log

Short entries so later sessions can pick up where earlier ones stopped. Newest first.

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
