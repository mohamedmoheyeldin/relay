# Final report (branch `new_app`, 2026-10-17)

## What exists
A Rust + Tauri + React desktop operator. Every message is first classified:
- **Questions and code requests** are answered in words (streamed; nothing runs). Coding questions go to the cloud AI in
  Hybrid mode; others to the local AI.
- **PC tasks** use plan -> validate -> permission -> execute -> verify, with native shortcuts for common requests
  (open an app, list a standard folder, what is using disk space), replans that never repeat finished effects, and an
  escalation from the local AI to the cloud when the local plan is unusable.
- **Website tasks** drive a real browser through the Playwright MCP server one observed step at a time, in the owner's own
  browser (Brave, Chrome or Edge with the Playwright extension) or a separate profile. Irreversible clicks ask first;
  passwords and card numbers are never typed.
Plus: tags showing what did the work, plain wording, "Sensitive" only for real sensitive data, appearance controls
(code themes with preview, fonts, text size, density, task view), resizable sidebar, System check with fix buttons,
frameless Windows window, a self-installing single executable, and an MCP server (`Relay.exe --mcp`).

## Verified on this PC (2026-10-17)
| layer | result |
|---|---|
| Rust unit + integration (incl. hybrid engine, web loop, MCP stdio, store) | 376 passed, 0 failed, 1 ignored (moves the real mouse) |
| UI unit/component (Vitest) | 216 passed |
| UI browser E2E (mock backend) | 35 passed |
| Visual regression | 48 baselines passed |
| Real installer (UAC accepted by the owner) | 6/6: files in `C:\Program Files\Relay`, uninstall entry, Start menu shortcut, version |
| Desktop E2E against the **installed** app | 6/6 (window, strict CSP, backend, settings round trip, validation, Local only) |
| Release build ignores the WebView2 debug hook | verified |

Real scenarios, run through the real app (Claude Code, local Qwen3 14B, real browser):
| message | path | result |
|---|---|---|
| "example of playwright code for testing amazon search?" | chat, Claude Code | code answer with code blocks, first text in 2.5 s, 10 s total, nothing opened |
| "what is a mutex?" | chat, local AI | answer streamed, ~8 s |
| "Find what is using my disk space" | native | drive totals + largest folders/files in 0.2 s |
| "list the files in my Downloads folder" / "open notepad" | native | instant |
| "go to https://www.dice.com/ and apply for fde jobs with easy apply" | web, Claude Code + Playwright | opened Dice, handled the cookie dialog, searched, opened an FDE job, reached **Apply Now** and asked for confirmation (declined by the test, nothing submitted), ~20 s, one tab |

Bugs found by these runs and fixed: file-based page snapshots (the AI saw an empty page), covered-element click timeouts,
links opening new tabs, long pages hiding the Apply button, a local plan opening an unrelated website, file listings and
disk questions routed to an AI, raw page code on confirmation cards, stale Ollama address, confirmation level ignored in
web tasks.

## Not verified (read this)
- **Your own Brave through the extension**: the connection works (Relay opened dice.com in your Brave earlier), but each
  new session needs you to click **Allow & select** on the Playwright connect tab, or to save the extension's token once in
  Settings > AI & Privacy > Browser control. I did not approve that prompt or copy the token for you: it grants access to
  your whole browser. The Dice regression run used a separate test Chromium.
- The final release build is in `Downloads\Relay Setup.exe` but is **not installed**: installing needs the Windows prompt.
  The installed copy right now is the previous test build.
- macOS, Codex CLI (not installed), screen readers, high-contrast mode, and Mica/vibrancy remain untested.
- The Dice account was not signed in; the sign-in hand-off ("sign in, then say continue") is covered by tests, not a live run.
