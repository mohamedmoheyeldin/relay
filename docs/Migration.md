# Migration notes

## From the `rust` branch
`new_app` was branched from `rust`; `main` and `dev` are untouched. Kept because they are validated and tested:
`relay-os`, `relay-protocol`, `relay-mcp`, `relay-store`, the Ollama engine manager, the self-installer. Changed:
- New `relay-orchestrator` crate and `relay-agent::Hybrid` engine: plan, validate, execute, verify, recover.
- `relay-ai` gained a Codex adapter and plan-only/structured-output support for Claude.
- `relay-store` gained a `workflows` table (created automatically, no data loss for existing databases).
- Default confirmation level is now **Destructive**. The `rust` branch defaulted to *no prompts* at the owner's request
  on 2026-10-16; the later brief requires confirmation of destructive actions, which supersedes it. The setting is still
  user-changeable (Settings → Privacy & permissions).
- The older tool-using agent remains available as *agent mode* (off by default).
- UI rebuilt on shadcn/Radix with a Relay design system (see Design-System.md).

## Settings
`settings.json` gains a `hybrid` object; files from older builds load unchanged (missing fields take defaults). An old
`approval_mode` value is kept for agent mode and updated together with the confirmation level when changed from Settings.

## macOS
The Rust workspace compiles for macOS in CI; the Desktop adapter and menu/vibrancy behaviour have not been run on a Mac
by the author. Treat macOS as *implemented, unverified* until a person has run the checklist in Testing-Strategy.md.
