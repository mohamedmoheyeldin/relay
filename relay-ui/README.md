# relay-ui

The Relay desktop app: a Tauri 2 tray app with one chat window, the first-run wizard
and the settings pages (React + TypeScript + Untitled UI React, built with pnpm).
See `docs/architecture.md` §6.

## Layout

- `src/` React app. `screens/` holds the chat, wizard, settings, Local AI and
  Connections pages; `lib/api.ts` wraps every Tauri command; `components/base` etc.
  are Untitled UI React components (MIT, see `components/LICENSE-untitledui.txt`).
- `src-tauri/` the Rust shell: tray, close-to-tray, single instance, autostart, jobs
  with progress events, the in-app terminal used for `claude auth login`, and the
  `relay.exe mcp-launch` subcommand that hands Credential Manager secrets to MCP servers.
  The real work lives in the `relay-core` crate.
- `resources/python/` the bundled CPython 3.12 + relay-screen, filled by
  `scripts/build_installer.py` (git-ignored).

## Develop

```
pnpm install
pnpm tauri dev
```

## Build Relay-Setup.exe

From the repo root:

```
uv run python scripts/build_installer.py
```

It bundles Python + relay-screen, builds the NSIS installer and writes
`dist/Relay-Setup.exe`. On Windows it uses the MSVC target; on Linux it cross-builds
with `x86_64-pc-windows-gnu` (needs mingw-w64, nsis and libayatana-appindicator3-dev).
The installer is unsigned for now, so SmartScreen warns on first run.
