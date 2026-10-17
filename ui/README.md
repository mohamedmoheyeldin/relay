# Relay UI

The chat window, task timeline, task history, settings and setup/uninstall screens for Relay.
React 19 + TypeScript (strict) + Vite 8, Tailwind CSS v4 (CSS-first tokens), shadcn/ui on Radix,
Zustand (client state), TanStack Query v5 (all backend data), Zod (every IPC payload), React Router
(HashRouter), TanStack Virtual, Lucide. It talks to the Rust backend only through the commands and
the `relay-event` event in `../docs/ui-contract.md` (v2). Design system:
`../docs/Design-System.md`.

## Commands (Node 24 via fnm, pnpm only)

```powershell
pnpm install
pnpm dev               # http://127.0.0.1:1420 (Tauri devUrl); in a plain browser it uses the mock backend
pnpm lint              # ESLint 9 flat config + typescript-eslint + react-hooks
pnpm typecheck         # TypeScript 7 (tsc --noEmit)
pnpm test              # Vitest: reducers, schemas, mock flows, token contrast, components + axe (jsdom)
pnpm test:e2e-browser  # Playwright against pnpm dev + mock: flows, keyboard, installer, axe with contrast
pnpm test:visual       # Playwright visual regression (tests/visual/__screenshots__, light+dark, 1100 and 520 wide)
pnpm build             # typecheck + vite build -> dist (relative asset paths for Tauri)
```

First Playwright run: `pnpm exec playwright install chromium`. Playwright starts and stops the dev
server itself. After an intended visual change run `pnpm test:visual:update` and review the PNGs.
Review screenshots for design work: `pnpm exec playwright test --project=shots` (written to `.shots/`,
ignored by Git).

TypeScript 7 has no JavaScript compiler API, so `typescript` resolves to 6.0.3 (used by
typescript-eslint and editors) and the TypeScript 7 compiler is installed as `typescript7`
(used by `pnpm typecheck`).

## Mock backend

`pnpm dev` in a browser uses `src/lib/mock` (it follows the contract exactly; tests check every
event against the Zod schemas). Query switches, combinable:

- `?scenario=` flags: `all-ready`, `codex-missing`, `claude-signed-out`, `local-setup`,
  `confirm-off`, `local-only`, `cloud-preferred`, `empty`, `fast`, `slow`, `mac` (vibrancy +
  overlay title bar), `mica`.
- `?flow=` forces the run flow: `native`, `local`, `cloud`, `download` (asks approval),
  `verify-fail`, `refused`, `declined`, `cloud-fail` (local fallback), `agent`, `not-installed`,
  `not-signed-in`, `chat`. Without it, words in the message pick a flow (for example "verify",
  "system32", "decline", "offline", "email", "notepad", "downloads", "disk").
- `?mode=install`, `?mode=install&update=1`, `?mode=install&fail=1`, `?mode=uninstall` show the
  setup screens; `#/gallery` shows the component gallery (development builds only).

Example: `http://127.0.0.1:1420/?scenario=all-ready&flow=declined#/`.

## Layout

```
src/
  main.tsx               routes app vs installer (installer_info) before the first render
  app/                   App (providers, HashRouter routes, shell, FLIP sidebar), shortcuts
  styles/tokens.css      design tokens (colour per theme/mode/accent/material, type, space, motion…)
  styles/index.css       Tailwind v4 @theme mapping, base, markdown, motion primitives
  lib/schemas/           Zod schemas per area (events, conversations, hybrid, system)
  lib/backend.ts         Tauri/mock transport, validated typed commands, contract-violation reporting
  lib/chat.ts            pure reducer: events -> transcript items (task timeline + agent groups)
  lib/task.ts            verdicts, step state machine, labels;  lib/workflow.ts  task history mapping
  lib/platform.ts        get_platform handling and the single Ctrl/Cmd shortcut helper
  lib/mock/              mock backend + scenarios
  store/app.ts           zustand: live chats from events, approvals, toasts, UI chrome
  store/appearance.ts    theme/mode/accent/custom colours -> <html> attributes
  hooks/queries.ts       TanStack Query hooks and optimistic mutations
  components/ui/         shadcn primitives adapted to Relay tokens
  components/{shell,chat,task,projects}/   app components
  pages/                 TasksPage, Installer, Gallery (code-split)
  settings/              settings page and sections (code-split)
tests/                   Playwright: e2e/, visual/, shots/
```
