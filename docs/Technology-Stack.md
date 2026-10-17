# Technology stack

Versions are the ones pinned in the lockfiles on 2026-10-16. Choices and the alternatives that lost are in the
decision log in `CLAUDE.md`.

| layer | choice | why |
|---|---|---|
| Shell | Tauri 2.12 (Rust, WebView2 on Windows, WKWebView on macOS) | small binary, native webview, scoped capabilities, no bundled browser |
| Core language | Rust (stable, edition per workspace), tokio, axum, reqwest, rusqlite (bundled SQLite), serde, schemars | one memory-safe process owns every side effect |
| Windows OS layer | `windows` crate: UI Automation, SendInput, GDI | official bindings, no helper process |
| macOS OS layer | `core-graphics` / `core-foundation` adapter (in progress, unverified on hardware) | see Migration.md |
| Local AI | managed Ollama engine on 127.0.0.1:11435; model chosen from GPU memory (qwen3 4b/8b/14b) | local planning with schema-constrained JSON |
| Cloud AI | Claude Code CLI and Codex CLI behind `AiProvider` | driven as unmodified CLIs with the owner's login |
| UI | React 19, TypeScript, Vite 8, Tailwind 4, Radix + shadcn components | accessible primitives, owned component code |
| UI state | Zustand (UI-only state), TanStack Query (backend data), Zod (event and payload validation), React Router | each tool does one job; Zod guards the Rust/TS boundary |
| Fonts | Newsreader (headings), Inter (UI), Geist Mono (code), self-hosted via Fontsource | no network at runtime, CSP stays `'self'` |
| Tests | cargo test, Vitest + Testing Library, Playwright (+ axe-core), CDP-driven desktop run | see Testing-Strategy.md |
| CI | GitHub Actions (Windows + macOS for Rust, Ubuntu for UI) | `.github/workflows/ci.yml` |
| Package managers | cargo, pnpm (never npm/yarn) | repository rule |

## Not chosen
- Electron: ships a browser per app and grants Node to the renderer by default; Tauri's capability model fits the
  "UI cannot touch the OS" rule better.
- A shell-execution tool for the AI: the closed action set removes the largest injection risk.
- Direct web APIs for the Cloud AI: provider terms and credential handling; the CLI route keeps sign-in with the CLI.
