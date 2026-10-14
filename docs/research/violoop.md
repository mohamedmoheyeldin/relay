# Violoop research (2026-10-14)

Purpose: learn how Violoop's desktop app and computer control work so Relay is
better. Design understanding only.

**Boundary.** Public repositories were read freely. The installed Windows app
(0.7.36, `C:\Program Files\Violoop`) was inspected statically and read-only:
file and folder names, version info, bundled license notices, dependency
manifests (`package.json`, `requirements.txt`), the Playwright upstream
manifest, the user-facing release notes, and crate names embedded in the
helper binaries. No Violoop source file (`.js`, `.vue`, `.ts`, internal
READMEs) was opened, and nothing from Violoop is copied into Relay.
Where a statement below is drawn from a file *name* rather than its contents,
it is marked *(inferred from names)*.

## 1. Answer to "is the app source public?"

No. None of the 18 public repositories in
[github.com/BvioTech](https://github.com/orgs/BvioTech/repositories) contains
the desktop app or the agent. The agent itself does not even run on the PC:
the desktop app's own manifest describes it as

> "Violoop PC thin shell — Electron + Vue3 frontend, business logic served by violoop-edge over WebRTC"

(`resources/app.asar` → `package.json`, `name: violoop-pc`, `private: true`).
"violoop-edge" is the RK3576 box; its software is not public. The PC app is a
UI plus a tool host that the box calls into.

Note: the shipped `app.asar` contains the client's readable JavaScript and Vue
sources (not minified, plus `tests/`, `vite.config.ts`, internal READMEs).
That is *packaged client code*, not a published repository, and carries no
license file (the `package.json` `license: "MIT"` field on a `private`
package is not a grant we rely on). Treat it as proprietary; it was not read.

## 2. Classification of everything found

| Item | Class | Evidence | License | Useful to Relay? |
|---|---|---|---|---|
| Violoop desktop app 0.7.36 | Compiled installer (packaged client code inside) | [release.violoop.ai/en](https://release.violoop.ai/en) → `…/179188449731782-Violoop-Setup-0.7.36.exe` (Win), `…-0.7.36-arm64.dmg`, `…-0.7.36-universal.dmg`, `…-0.7.36.AppImage`, Android `violoop-mobile-1.1.29-34.apk`, all on `dqk7irzt7hqfw.cloudfront.net/public/release/` | Proprietary | Architecture and UX only |
| Device ("violoop-edge") software | Not public | Named only in the PC app's `package.json` | n/a | No |
| [violoop-builder](https://github.com/BvioTech/violoop-builder) | Build infrastructure | Archived 2026-06-19. `CLAUDE.md`: packages an aarch64 cross toolchain (`arm-gnu-toolchain-12.3.rel1`, `aarch64-none-linux-gnu`), a sysroot copied from the device, `toolchain.cmake`; Docker images `builder:base` → `builder:violoop-pro`; `publish.yml` releases a tarball on `v*` tags. "Contains no application code." | None shown | No (device-side C/C++ toolchain) |
| [homebrew-violoop](https://github.com/BvioTech/homebrew-violoop) | Distribution metadata | `Casks/violoop.rb` pins 0.7.27 (stale), two CloudFront DMG URLs + SHA-256, `zap` of `~/Library/Application Support/Violoop` and `ai.violoop.app.plist`. README (Chinese) calls it a private tap installed by `violoop-ansible` | None shown | No |
| violoop-ansible | Not public | Mentioned in the Homebrew README only | n/a | No |
| [litellm](https://github.com/BvioTech/litellm) | Upstream fork ([BerriAI/litellm](https://github.com/BerriAI/litellm)) | Viewed branch `litellm_internal_staging` | Upstream MIT (enterprise dir separate) | Use upstream directly if ever needed |
| [hailort](https://github.com/BvioTech/hailort), [hailort-drivers](https://github.com/BvioTech/hailort-drivers) | Upstream forks (hailo-ai) | `violoop` branch, 23 commits | MIT / LGPL-2.1 (GStreamer) / GPL-2.0 (driver) | No (Hailo NPU on device) |
| [rkwifibt](https://github.com/BvioTech/rkwifibt) | Vendored BSP (not a fork) | `violoop` branch, 2 commits; `debian/`, `meson.build`, `wifibt-init.service` | BSD-3-Clause (Rockchip) | No |
| [rknn-llm](https://github.com/BvioTech/rknn-llm), [rknn-toolkit2](https://github.com/BvioTech/rknn-toolkit2), [rknn3-toolkit](https://github.com/BvioTech/rknn3-toolkit), [rknn3-model-zoo](https://github.com/BvioTech/rknn3-model-zoo) | Upstream forks (airockchip) | Fork banner | Rockchip "Other" / Apache-2.0 (model zoo) | No (Rockchip NPU) |
| [kernel](https://github.com/BvioTech/kernel), [u-boot](https://github.com/BvioTech/u-boot), [rkbin](https://github.com/BvioTech/rkbin), [mpp](https://github.com/BvioTech/mpp), [librga](https://github.com/BvioTech/librga), [JeffyCN-mirrors](https://github.com/BvioTech/JeffyCN-mirrors) | Board support / upstream forks | Org listing | GPL-2.0 / mixed / Apache-2.0 / LGPL-2.1 | No |
| [mbedtls](https://github.com/BvioTech/mbedtls), [tiny-regex-c](https://github.com/BvioTech/tiny-regex-c) | Upstream forks (libraries) | Org listing | Apache-2.0 / Unlicense | No |
| [huggingface.co/Violoop](https://huggingface.co/Violoop) | Model assets (none public now) | 0 models, 0 datasets, 4 members, links bvio.ai. Indexed `Violoop/OmniParser-v2.0` and `Violoop/PP-OCRv4_mobile_det` no longer load | n/a | The upstream models are public (see §5) |

Fork diffs against upstream were not audited; every fork is device-side
(Rockchip/Hailo board, NPU, Wi-Fi), none touches the desktop app. Their only
value to Relay is confirming the box runs Linux on RK3576 with a Hailo or
Rockchip NPU.

## 3. How the desktop app is built

- **Shell:** Electron 40.9.3 / Chromium 144.0.7559.236 (strings in
  `Violoop.exe`), `electron-updater` with a generic feed
  (`resources/app-update.yml` → `https://api.violoop.ai/release/violoop/feed`),
  NSIS per-machine install with `elevate.exe` for updates.
- **UI:** Vite + TypeScript, Vue 3, vue-router, vue-i18n (many locales),
  ant-design-vue 4 + `@ant-design/icons-vue`, echarts, markdown-it,
  highlight.js, a file viewer (office/pdf previews). Own design tokens
  (`electron/vio-tokens.css`). Icons from Tabler (MIT) and Lucide (ISC),
  loaders from LDRS and Loading UI (MIT) per `THIRD_PARTY_NOTICES.md`.
- **Office add-in:** a separate React 19 + Vite + `office-js` task pane
  (`office-addin/package.json`, "Violoop AI Agent for Microsoft Office"),
  side-loaded with `office-addin-dev-certs`.
- **Native helpers (Rust):** `resources/bin/desktopctl.exe` (8 MB) and
  `violoop-overlay.exe` (2.9 MB). Embedded crate paths show `windows 0.58`,
  `tokio`, `interprocess` (local IPC), `serde_json`, `image`/`png`/`zune-jpeg`,
  and `resvg`/`tiny-skia`/`rustybuzz` (vector + text rendering for the
  overlay). `desktopctl` also contains UI Automation symbols.
- **Node native modules** (`app.asar.unpacked`): `@nut-tree-fork/nut-js`
  (mouse/keyboard), `node-screenshots` (capture), `get-windows` (active
  window), `koffi` (FFI to Win32), `@lydell/node-pty` (terminals),
  `clipboardy`, `serialport` (USB link to the box), `playwright-core`.
- **Other deps of note:** `@modelcontextprotocol/sdk` 1.29.0, `zod` +
  `zod-to-json-schema` (tool schemas), `gpt-tokenizer`, `partial-json`
  (streaming tool-call parsing), `node-machine-id`, proxy agents.
- **Managed runtimes:** Bun and CPython (`electron/runtime/{bun,python}`,
  installed under `%APPDATA%\Violoop\pc\runtime`). The Python
  `requirements.txt` is `numpy pandas pillow opencv-python pyautogui pynput
  requests pypdf`: a scratch environment for scripts the agent writes.
- **User extension points:** `%USERPROFILE%\.violoop\mcp.json` (MCP servers)
  and `%USERPROFILE%\.violoop\skills\<name>\SKILL.md` (Agent Skills format;
  bundled: `find-skills`, `skill-creator`, `manage-mcp`,
  `install-browser-connector`, `office-js`, `scanned-pdf-cleanup`).
- The team builds with Claude Code (`CLAUDE.md` files in violoop-builder and
  the add-in).

## 4. How computer control works *(inferred from names unless stated)*

**Topology.** The agent loop and models live on the box. The PC app connects
to it over WebRTC (`transports/edge_peer_*`), USB serial
(`edge_serial_port`, `serial_engine_*`), or TCP, and serves RPC calls
(`ipc/rpc-bridge`, `system_rpc/`). Screen frames also stream from the PC
(`screen_stream/`) in addition to HDMI capture. Physical HID input comes from
the box; the PC app adds software paths the HDMI/HID route cannot reach.

**Tool families the PC app hosts** (`electron/tools/`):

| Family | Files | What it gives the agent |
|---|---|---|
| Accessibility | `desktop_ax.js`, `desktop_target_registry.js`, `data/ax/lark.txt` | UIA tree reading and element targets, with per-app hints |
| Input | `computer_input.js`, `util/cursor_native.js`, nut-js, koffi | Mouse/keyboard from the PC side |
| Capture | `capture_screen.js`, `util/display_topology.js`, node-screenshots | Screenshots across monitors with a display map |
| Verify after acting | `post_click_observation.js`, `preview_post_action_observation.js`, `preview_action_state.js` | Re-observe after each action and judge success (release notes 0.7.35: "verification after desktop actions") |
| Built-in browser | `tools/browser.js`, `web_preview.js` (138 KB), `preview_cdp.js`, `preview_aria_snapshot.js`, `vendor/playwright-aria`, `stealth.min.js`, `captcha_*` | An embedded Chromium the agent drives over CDP and reads as Playwright ARIA snapshots; waits, dialogs, uploads, downloads, multiple tabs, cookie sync with the user's browser |
| Shell and files | `unified_exec.js`, `pty/`, `apply_patch.js`, `glob.js`, `grep.js`, `io.js`, `path_sandbox.js` | Coding-agent style tools, sandboxed to allowed paths |
| Apps | `installed_apps.js`, `aumid_cache.js`, `active_app_native.js` | Launch apps by AppUserModelID, know the foreground app |
| Office | `office_bridge/` + add-in | Drive Word/Excel/PowerPoint through office-js instead of pixels |
| Extensibility | `mcp/`, `acp/`, `bundled-skills/` | MCP client, Agent Client Protocol host (external coding agents), skills |

**Lighting up accessibility on Windows.** `bin/uia-activator/launch.vbs`
(comment read, it is a launcher script) states the problem plainly: many apps
(Qt apps such as WeChat 4.x, also Chromium/Electron) keep their UIA/IA2 tree
dormant until an assistive technology is present. Violoop registers an
at-logon scheduled task that runs NVDA's `hook_host` to make new processes
expose their trees. (`hook_host.exe` itself was not present in this install.)

**Playwright ARIA snapshots (Apache-2.0, public).** Violoop vendors Playwright
revision `18205280b6112a4a08238195942c4fa30c199a62`
(`ariaSnapshot.ts`, `ariaSnapshotDistiller.ts`, `roleUtils.ts`, `yaml.ts`, …)
and adds a redaction hook for field values. This is the cheapest high-quality
"what is on this web page" representation for an LLM.

**Proactive layer.** `proactive/commitment/` has per-app adapters (WeChat,
Slack, Lark, Discord, WhatsApp, Messages, generic, learned), a Windows and
macOS platform driver, and services named `screen_reader`, `ax_text_tree`,
`focus_chain`, `scene_resolver`, `notif_rhythm`, `completion_pipeline`. It
watches chat apps for promises the user makes ("I'll send it tomorrow") and
offers to do them.

## 5. Models

Violoop's site: Qwen 8B Q4 on the device (~53 tok/s) and "three vision models
read the screen"; cloud models via the user's own API keys. The previously
public Hugging Face repos point at what two of those likely are (inference):

- **OmniParser v2** ([microsoft/OmniParser-v2.0](https://huggingface.co/microsoft/OmniParser-v2.0)):
  YOLO-based interactable-element detector (AGPL-3.0) + Florence-2 icon
  captioner (MIT).
- **PP-OCRv4 mobile det** (PaddleOCR, Apache-2.0): fast text detection.

Release notes add: reasoning-level settings per model, custom OpenAI-style
providers with editable context size, automatic context compaction (100K to
900K for 1M-context models), memory graph, "subscription model fallbacks".

## 6. UX inventory (from component and window names, release notes, screenshot)

- **Windows:** main window, `floating-bar` (always-on-top agent bar),
  `quickinput` (global-hotkey prompt), `plan-chip` (small plan/progress chip),
  `notification` (own toast stack that stays while hovered), `mascot`
  (optional desktop pet with skins), `debug-overlay`, Rust click-through
  overlay, `screen-capture`.
- **Tray:** three icon states, `tray-idle.png`, `tray-busy.png`,
  `tray-active.png`.
- **Close behaviour dialog** (`CloseBehaviorModal.vue`; screenshot in the
  thread): asked once on first close. Title "What should the close button do?",
  explains the app must keep running to keep working, two radio cards
  ("Keep running in the background", marked Recommended, vs "Quit the app"),
  one primary button "Save and tuck away", changeable later in Settings.
- **Safety and trust:** `KillSwitch.vue`, `DeviceButtonPrompt.vue` (press the
  physical key to approve), `BudgetMeter.vue` (spend/context budget),
  `AwayBriefing.vue` (what happened while you were away), `SecretsView`
  (stored credentials), `self-check` diagnostics, permission guidance windows.
- **Views:** Agenda (schedules with status filters and run history), Agents,
  Command, Drive, Files, Memory (graph), Workspace/Projects, Onboarding,
  Settings, Secrets.

## 7. Takeaways for Relay

Aimed at what the plan (D-001…D-016) does not already cover.

1. **UI framework: confirmed.** Electron 40.9.3 + Vue 3 + ant-design-vue
   (D-007 is correct). Nothing found argues against Tauri 2 + React + Untitled
   UI. One addition: Violoop ships its click-through overlay as a separate
   small Rust process; in Tauri that can be a second transparent,
   click-through, always-on-top window, no extra binary needed.
2. **Turn on screen-reader mode before reading UIA (new, cheap).** The
   dormant-tree problem is real. Relay should call
   `SystemParametersInfo(SPI_SETSCREENREADER, TRUE)` while armed (and restore
   it on exit) and use `--force-renderer-accessibility` when it launches a
   Chromium browser. Do not bundle NVDA (GPL-2.0); try the flag first and
   measure on the M1 form demo.
3. **DOM lane via Playwright ARIA snapshots (feeds D-016).** Playwright is
   Apache-2.0 and the Python package exposes `locator.aria_snapshot()`. This
   is the representation Violoop chose for its browser; recommend it for the
   M2 browser lane, with field-value redaction before anything goes to the
   cloud.
4. **Verify after every action (add to architecture §4.3).** Violoop has
   dedicated post-action observation for desktop and browser. Relay should
   make "re-observe and compare" part of the action contract: after
   click/type, re-read the UIA element or a crop and return a short
   success/unclear verdict with the tool result.
5. **Playbooks as Agent Skills.** Violoop adopted the same `SKILL.md` folder
   format Claude Code uses, plus a user `mcp.json`. Relay's playbooks
   (architecture §7) should be Claude Code skills so the cloud brain loads
   them natively.
6. **Office through office-js, not pixels (new lane).** For Word/Excel/
   PowerPoint, an add-in or COM automation beats screen control. Add a
   "connector" note for Office under D-003/D-008; COM via `pywin32` is the
   lighter first step for Relay.
7. **Launch apps by AppUserModelID.** Cache installed apps with their AUMIDs
   and start them through the shell, instead of searching the Start menu by
   pixels.
8. **Models (D-005 stays).** Violoop pairs a small local LLM with detector +
   OCR + captioner. If UI-Venus grounding misses icons, OmniParser v2 is the
   known fallback, but its detector is AGPL-3.0: keep it optional and
   out-of-process. PP-OCRv4 (Apache-2.0) is a faster OCR alternative to
   Windows OCR for small text if needed.
9. **Tray and close UX (M3).** Copy the *pattern* (not wording or assets):
   first-close dialog with Recommended "keep running in tray" vs "quit",
   remembered and changeable in Settings; tray icon with idle / busy /
   needs-approval states; a quick-input hotkey window; a small floating
   plan/progress chip; toasts that stay while hovered; an "away briefing"
   summarising unattended runs; a visible budget/context meter.
10. **Approval UX.** Violoop's trust anchor is a physical key. Relay's
    software equivalent (D-011) should be just as hard to fake: approval
    prompts in Relay's own top-most window that the agent cannot click
    (Relay ignores synthetic input on its own approval window), plus the
    existing kill switch.
11. **Proactive suggestions are a later idea, not M1-M3.** The commitment
    detector over chat apps is a distinctive Violoop feature; park it in the
    roadmap as "post-M3".
