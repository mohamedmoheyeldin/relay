# Performance

Measured 2026-10-16 on the author's PC (Windows 11, RTX 5080, release build). One machine, a few runs each: baselines, not
guarantees. There is **no automated budget gate** yet.

| measure | value | how |
|---|---|---|
| App executable | 8.9 MB (`relay-app.exe`, includes installer, UI and MCP server) | file size |
| Stand-alone MCP server | 1.9 MB (`relay-mcp.exe`) | file size |
| UI assets | 4.0 MB in `ui/dist` (fonts included) | folder size |
| UI JS loaded at startup | 604 KB raw / 192 KB gzip (before the redesign: 436 / 132) | `vite build`, measured by the UI agent |
| Window ready (debug port answering) | 0.50, 0.51, 0.53 s | launch to DevTools target list, 3 runs |
| Memory after 8 s idle | `relay-app.exe` 39 MB; whole process tree incl. WebView2 about 400 MB working set | PowerShell, sums double-count shared pages |
| MCP server start to `initialize` reply | about 8 ms warm, 59 ms first run | 5 runs |
| Native task (`open notepad`) | about 1 s from send to result in the UI | real app run |
| Cloud-planned task (Claude Code, one list step) | about 9 s routed + planned | real app run |

## What was designed for speed
- Trivial requests skip every AI call. Provider status is cached for 30 s. Long chats are virtualised, heavy UI
  (settings, history, Markdown, syntax highlighting) is split into lazy chunks.
- Motion uses transform and opacity only and respects reduced-motion (checked in the design system tests, not profiled).

## Not measured yet
Frame rate under load, input latency, long-chat scroll cost, local-model latency (no model installed during these runs),
cold start with antivirus scanning, battery. A budget check in CI is the next step.
