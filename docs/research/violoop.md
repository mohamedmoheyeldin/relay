# Violoop reference notes (2026-10-14)

For design understanding only. No Violoop code, assets or extracted files are copied into Relay.

## Product (violoop.ai, release notes)
- External box: HDMI 2.0 capture (4K30), USB-HID keyboard/mouse via an STM32 security MCU, RK3576 + 26 TOPS NPU, 13 GB RAM, Qwen 8B Q4 on device (~53 tok/s), physical confirmation key, AMOLED touchscreen, dual-monitor support.
- Modes: Instant screen-aware suggestions (Tab to accept), long-running cross-app tasks, Away mode (lock screen, phone pings when done or blocked).
- Action routing with fallback: script/API 10 ms (99%), keyboard shortcut 20 ms (95%), accessibility handle 50 ms (90%), GUI click 200 ms (70%). "Three vision models read the screen".
- Local memory of how you work, filtered text summary (not raw screen) sent to cloud for big tasks.
- 0.7.36 release notes: memory graph, Agenda/schedules, browser waits and download management, custom API models with reasoning levels and context compaction, Plugins and Agents settings.

## Desktop app (installed 0.7.36, read-only inspection of names/versions)
- Electron 40.9.3 (Chromium 144), electron-updater, NSIS installer, ~703 MB.
- package `violoop-pc`, Vite + TypeScript, Vue 3 + vue-router + vue-i18n, ant-design-vue, echarts, markdown-it, highlight.js, pdfjs.
- Automation deps by name: @modelcontextprotocol/sdk, playwright-core, nut-js (input), node-screenshots, get-windows, koffi (FFI), node-pty; helper binaries for desktop control, overlay and a UIA activator.
- Bundles its own runtimes in `%APPDATA%\Violoop\pc\runtime` (bun, CPython 3.12 + venv): same "self-contained" approach Relay takes (D-004, D-010).
- UI: warm off-white theme, teal accent, Inter + JetBrains Mono, "EXECUTE" mode badge, top bar (Agenda, search, device chip, connection status, theme, downloads), tray icon.

## Takeaways for Relay
- Keyboard shortcuts and accessibility before pixels (D-008).
- A DOM/browser lane is worth evaluating (D-016).
- Phone approval card pattern: Run / Always allow / Cancel (M3).
- Things a software operator cannot match: secure desktop and locked-screen operation (architecture §9).
