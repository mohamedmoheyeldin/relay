# Relay

Relay is a Windows tray app with one chat window. Describe a task and an AI agent carries it out
on your desktop: it looks at the screen and uses the mouse and keyboard. Relay asks before anything
that can't be undone, and `Ctrl+Alt+Shift+Esc` stops the agent immediately.

## How it works
- The agent is [Claude Code](https://claude.com/claude-code) running headless as a child process
  (`claude -p --output-format stream-json`), so Relay uses your existing sign-in. No API keys.
- Relay serves desktop tools (screenshot, click, type, keys, scroll, windows, ...) to it over a
  loopback MCP server.
- Your own services (Gmail, GitHub, ...) connect the same way: add MCP servers to `mcp.json`
  (Settings shows where it is).

## Run it
Prerequisites: Rust, `trunk`, `cargo-tauri` v2, the `wasm32-unknown-unknown` target, and Claude Code
signed in (`claude auth login`).

```powershell
cd src-tauri
cargo tauri dev      # development
cargo tauri build    # installer
```

## Develop
```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p relay-ui --target wasm32-unknown-unknown -- -D warnings
cargo test --workspace
```

Architecture, conventions and decisions are in [CLAUDE.md](CLAUDE.md).
