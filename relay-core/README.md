# relay-core

Rust library with everything the app does that is not UI. For now the app links it
in-process; splitting it into its own daemon is M2 work. See `docs/architecture.md` §5.

- `settings` the settings file (`%APPDATA%\Relay\settings.json`) and run pruning
- `claude` find, install, update, sign-in state and self-test of the `claude` CLI (D-002)
- `chat` one chat turn through `claude -p --output-format stream-json`
- `mcp`, `secrets`, `launcher` the MCP hub over Claude Code's own MCP list, with secrets
  in Windows Credential Manager
- `gpu`, `catalog`, `models`, `download`, `runtime` local model picker, fit check,
  resumable verified downloads and the llama.cpp runtime
- `system`, `updates`, `doctor`, `paths`, `proc` helpers

`cargo test -p relay-core` runs on any OS; `scripts/check.py` runs it with the rest.
