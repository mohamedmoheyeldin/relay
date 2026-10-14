# Roadmap

| Milestone | Scope | Demo | Exit |
|---|---|---|---|
| **M1** (weeks 1-2) | relay-screen MCP server: computer toolset members + batch, UIA, OCR, local VLM, runtime manager, safety, playbooks v0 | Notepad save; test form up to approved Submit | [m1-plan.md](m1-plan.md) all done, `dev` -> `main`, tag v0.1.0 |
| **M2** (weeks 3-4) | relay-core router + lanes (service / screen / judgment), headless `claude -p` stream-json driver, `profile.yaml`, judgment-field escalation, browser lane decision (D-016) | Reply to an email with data from a sheet + a form submission in one task | v0.2.0 |
| **M3** (month 2) | relay-ui: Tauri tray + chat, approvals, login prompts, activity log, playbook memory, auto-unload surfaced, ntfy.sh phone approvals, single installer with sidecars, first-run model download. Tray UX: ask once on first close (keep running in tray recommended), idle/busy/needs-approval icon states, quick-input hotkey, floating progress chip, briefing of unattended runs, usage meter | Whole canonical flow from the chat window | v0.3.0 |
| **M4** (later, optional) | Session recording, newer GUI models, QLoRA on own data, separate-session "away mode" research | Measurable grounding gain on own eval set | n/a |

## Non-goals (v1)
- Multi-user, distribution to others (subscription rule), cross-platform.
- Proactive "watch me and suggest automations".
- Storing credentials in the repo.
