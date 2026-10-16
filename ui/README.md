# Relay UI

The chat window, settings and setup/uninstall screens for Relay, built with React 19, TypeScript (strict),
Vite, Tailwind CSS v4 and Radix UI primitives. It talks to the Rust backend only through the commands and
the `relay-event` event in `../docs/ui-contract.md`.

## Commands (pnpm only)

```powershell
pnpm install     # once
pnpm dev         # http://127.0.0.1:1420 (Tauri devUrl); in a plain browser it uses the built-in mock backend
pnpm build       # tsc --noEmit && vite build -> ui/dist (relative asset paths for Tauri)
pnpm test        # vitest: chat reducer, date grouping, theme maths, MCP JSON validation
```

Mock backend: open `http://127.0.0.1:1420` in any browser. Add `?mode=install`, `?mode=install&update=1`,
`?mode=install&fail=1` or `?mode=uninstall` to see the setup screens. In the mock, a message containing
"error"/"login" shows the not-signed-in error, "not installed" shows the missing-CLI error, and setting
Approvals to "Destructive actions" makes the Downloads clean-up ask for approval.

## Layout

```
src/
  main.tsx              routes app vs installer (installer_info) before the first render
  App.tsx               shell, global shortcuts, lazy settings
  lib/                  pure logic (tested): chat.ts reducer, grouping.ts, theme.ts, mcp.ts,
                        backend.ts (Tauri transport + typed commands), mock.ts (dev backend),
                        highlight.ts (lazy shiki, JS regex engine)
  store/app.ts          zustand store: event queue flushed once per frame, run -> conversation routing,
                        lazy conversation creation, debounced conv_save
  store/appearance.ts   theme/mode/accent/custom colours -> CSS variables, saved in localStorage
  components/           sidebar, header, composer, messages, dialogs, search palette, project page
  settings/             settings page (code-split)
  installer/            setup and uninstall screens (code-split)
```

## Notes

- Chats are stored by the backend; `items` is owned by this UI (see `lib/chat.ts` for the shape).
- Markdown, syntax highlighting, settings and installer screens load on demand, so the first paint
  only needs the main chunk.
- `.shots/` holds local screenshots from visual checks and is ignored by Git.
