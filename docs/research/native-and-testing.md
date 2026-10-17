# Native-quality Windows 11 + macOS behaviour and full-stack testing (Tauri 2)

Research date: 2026-10-16. Read-only research; nothing was installed or built. Versions come from the npm registry (`registry.npmjs.org`) and crates.io API on the research date. Behaviour claims come from the cited official pages. Anything not confirmed is listed in section 8.

## 1. Verified versions

"Date" is the publish date of the version in the registry (npm `time`, crates.io `updated_at` for the crate, which can be later than the stable version's own date).

### 1.1 Tauri core and plugins

| Package / crate | Stable | Date | Source |
|---|---|---|---|
| `tauri` (crate) | 2.12.4 (3.0.0-alpha.5 exists; do not use) | 2026-10-14 | https://crates.io/api/v1/crates/tauri |
| `tauri-build` (crate) | 2.7.3 | 2026-10-14 | https://crates.io/api/v1/crates/tauri-build |
| `@tauri-apps/api` | 2.12.4 | 2026-10-14 | https://registry.npmjs.org/@tauri-apps/api |
| `@tauri-apps/cli` / `tauri-cli` | 2.12.3 | 2026-10-14 | https://registry.npmjs.org/@tauri-apps/cli |
| `tauri-driver` (crate) | 2.1.0 | 2026-09-26 | https://crates.io/api/v1/crates/tauri-driver |
| `window-vibrancy` (crate, used internally by Tauri) | 0.8.1 | 2026-09-22 | https://crates.io/api/v1/crates/window-vibrancy |
| notification: `tauri-plugin-notification` / `@tauri-apps/plugin-notification` | 2.7.0 / 2.7.0 | 2026-10-14 | crates.io, npm |
| dialog: `tauri-plugin-dialog` / `@tauri-apps/plugin-dialog` | 2.8.3 / 2.8.3 | 2026-10-14 | crates.io, npm |
| global-shortcut: `tauri-plugin-global-shortcut` / `@tauri-apps/plugin-global-shortcut` | 2.4.1 / 2.4.1 | 2026-10-10 (npm) | crates.io, npm |
| single-instance: `tauri-plugin-single-instance` (Rust only, no npm package; npm registry returns 404) | 2.5.4 | 2026-10-14 | https://crates.io/api/v1/crates/tauri-plugin-single-instance |
| opener: `tauri-plugin-opener` / `@tauri-apps/plugin-opener` | 2.7.2 / 2.7.2 | 2026-10-14 | crates.io, npm |
| autostart: `tauri-plugin-autostart` / `@tauri-apps/plugin-autostart` | 2.7.0 / 2.7.0 | 2026-10-01 (npm) | crates.io, npm |
| updater: `tauri-plugin-updater` / `@tauri-apps/plugin-updater` | 2.14.0 / 2.14.0 | 2026-10-14 | crates.io, npm |
| window-state: `tauri-plugin-window-state` / `@tauri-apps/plugin-window-state` | 2.5.0 / 2.5.0 | 2026-09-26 (npm) | crates.io, npm |
| (also useful) `tauri-plugin-deep-link` 2.6.2, `tauri-plugin-log` 2.10.1, `tauri-plugin-store` 2.5.0 | | 2026-10-14 | crates.io |

All the official plugin docs checked (notification, single-instance, updater, window-state, autostart, global-shortcut) state **Rust 1.90 or later** is required (https://v2.tauri.app/plugin/notification/ and siblings). GitHub's Windows Server 2025 image has Rust 1.99.0 and macOS 26 has Rust 1.98.1, so CI satisfies this.

### 1.2 Frontend

| Package | Stable | Date | Notes / source |
|---|---|---|---|
| `react`, `react-dom` | 19.3.1 | 2026-10-14 | npm |
| `vite` | 8.3.4 | 2026-10-08 | Node `^20.19.0 \|\| >=22.12.0` (npm `engines`) |
| `@vitejs/plugin-react` | 6.1.2 | 2026-10-05 | peer `vite ^8`; v6 uses oxc/rolldown (npm peerDependencies) |
| `typescript` | 7.0.2 | 2026-07-08 | npm `latest` tag. Confirm shadcn/vitest/typescript-eslint support before adopting 7; not verified here |
| `tailwindcss` + `@tailwindcss/vite` | 4.3.3 | 2026-07-16 | `@tailwindcss/vite` peer `vite ^5.2 \|\| ^6 \|\| ^7 \|\| ^8` (npm); setup https://tailwindcss.com/docs/installation/using-vite |
| `shadcn` (CLI package) | 4.22.0 | 2026-10-16 | Node `>=20.18.1` (npm); docs https://ui.shadcn.com/docs/installation/vite |
| `@tanstack/react-query` | 5.104.3 | 2026-10-15 | npm |
| `@tanstack/react-table` | 9.3.1 (v9 is now `latest`; v8 is the previous major) | 2026-10-12 | npm. v9 has API changes from v8; check the v9 docs before copying v8 examples |
| `@tanstack/react-virtual` | 3.14.21 | 2026-10-16 | npm |
| `react-router` | 8.4.0 | 2026-09-15 | Node `>=22.22.0`, peer `react >=19.2.7` (npm). Package is `react-router` (not `react-router-dom`). Use declarative/data mode in a Tauri SPA (no SSR needed) |
| `zod` | 4.6.7 | 2026-10-12 | npm |
| `zustand` | 5.0.15 | 2026-08-13 | npm |
| `motion` | 14.3.0 | 2026-10-15 | peer react 18 or 19 (npm). Import from `motion/react` |

### 1.3 Testing

| Package | Stable | Date | Notes / source |
|---|---|---|---|
| `vitest` | 5.0.5 | 2026-10-15 | Node `^22.12.0 \|\| ^24.0.0 \|\| >=26.0.0`; peer vite `^6.4 \|\| ^7 \|\| ^8` (npm) |
| `@testing-library/react` | 16.3.3 | 2026-08-27 | npm |
| `@testing-library/user-event` | 14.6.8 | 2026-10-13 | npm |
| `@playwright/test` | 1.64.1 | 2026-10-14 | Node `>=20` (npm) |
| `@axe-core/playwright` | 4.14.0 | 2026-10-15 | npm |
| `vitest-axe` | 0.1.0 (last published 2022-10-21; effectively unmaintained; a `1.0.0-pre.5` tag exists) | 2022 | npm. Prefer `axe-core` called directly in Vitest or `@axe-core/playwright` in E2E. `jest-axe` 11.0.0 (2026-07-26) is maintained but Jest-oriented |
| `webdriverio` / `@wdio/cli` | 10.0.2 is `latest`; 9.32.0 is the newest 9.x | 2026-10-08 / 2026-09-20 | See compatibility trap below |
| `@wdio/tauri-service` | 1.5.0 (`next` = 1.3.0-next.1) | 2026-10-08 | Peer `webdriverio ^9.0.0`, depends on exact `webdriverio 9.31.9`, Node `>=18.20.0` (npm). https://webdriver.io/docs/desktop-testing/tauri/ |
| `@wdio/tauri-plugin` (frontend half of the wdio plugin) | 1.5.0 | 2026-10-08 | npm |
| `tauri-plugin-wdio`, `tauri-plugin-wdio-webdriver` (crates) | 1.5.0 each | 2026-10-08 | crates.io |
| `edgedriver` (npm helper; not needed if the service auto-downloads) | 8.0.0 | 2026-10-09 | npm. The old `msedgedriver` npm package is dead (91.0.0, 2021) |
| `pnpm` | 12.14.0 | 2026-10-15 | npm. The user's global notes list pnpm for new JS projects; pin via `packageManager` |

**Compatibility trap (verified from npm metadata):** `@wdio/tauri-service@1.5.0` peers on `webdriverio ^9.0.0` and pins 9.31.9, but `latest` is 10.0.2. Install `webdriverio@9`, `@wdio/cli@9`, `@wdio/local-runner@9`, `@wdio/mocha-framework@9` (or jasmine/cucumber) and `@wdio/spec-reporter@9` together. Do not let pnpm resolve WDIO packages to 10.x until the service declares support. Keep the E2E package in its own workspace folder (for example `e2e/`) so its pins do not constrain the app.

**Runtime constraints to align (from npm `engines`):** Vite 8 needs Node 20.19+/22.12+, Vitest 5 needs Node 22.12+/24+/26+, react-router 8 needs Node 22.22+. Node 24 (the user's fnm default for other projects, and GitHub's macOS image ships 24.20.0) satisfies all. Pin it in `.node-version` and `engines`.

## 2. Native windowing on Windows 11 and macOS

Primary sources: https://v2.tauri.app/reference/config/ (WindowConfig, WindowEffectsConfig, TitleBarStyle), https://v2.tauri.app/learn/window-customization/, https://v2.tauri.app/learn/window-menu/, https://v2.tauri.app/learn/system-tray/.

### 2.1 Window effects (Mica / Acrylic / Tabbed / vibrancy)

Config (`tauri.conf.json`, `app.windows[]`). The `windowEffects` shape is `{ effects: [...], state?, radius?, color? }`:

```json
{
  "label": "main",
  "transparent": true,
  "decorations": true,
  "windowEffects": { "effects": ["mica"] }
}
```

Per-platform effect lists (from the config reference):

- Windows 11 only: `mica`, `micaDark`, `micaLight`, `tabbed`, `tabbedDark`, `tabbedLight`. `mica` and `tabbed` follow the system dark preference; the `Dark`/`Light` variants force it.
- Windows 10/11: `acrylic` (documented bad resize/drag performance on Win10 1903+ and Win11 build 22000). `blur` is Win7/10/11-22H1 only and has bad resize/drag performance on Win11 build 22621. Avoid both as the primary look.
- macOS 10.14+: `sidebar` (10.11+), `headerView`, `sheet`, `windowBackground`, `hudWindow`, `fullScreenUI`, `tooltip`, `contentBackground`, `underWindowBackground`, `underPageBackground`, `menu`, `popover`, `titlebar`, `selection`. macOS 26+: `liquidGlassRegular`, `liquidGlassClear`.
- `windowEffects.state` and `radius` are macOS only. `color` affects only Blur/Acrylic on Win10 1903+ (nothing on Win11) and Liquid Glass on macOS. `interactive` needs macOS 27+.
- Runtime equivalent: `WebviewWindow.setEffects()` / `clearEffects()` in `@tauri-apps/api/window` and `Window::set_effects` in Rust (documented under the JS window namespace https://v2.tauri.app/reference/javascript/api/namespacewindow/; not opened in detail here, verify signatures).
- `app.macOSPrivateApi` is a no-op from Tauri 2.12.1 (transparency APIs are always enabled). On older Tauri it was required for macOS transparency. We are on 2.12.4, so do not add it; note this affects App Store review only historically.

Recommended setup:

1. Make both the window and the webview transparent: `"transparent": true` in config, and in CSS `html, body, #root { background: transparent; }`. Any opaque ancestor (a Tailwind `bg-background` on `body`, a shadcn `Card` filling the page) hides the effect. Give only intentional content panes an opaque or semi-opaque colour.
2. Pick the effect per OS. Use a platform-keyed window config (a `tauri.windows.conf.json` and `tauri.macos.conf.json` merge file, https://v2.tauri.app/develop/configuration-files/; not opened here) or apply in Rust `setup` behind `#[cfg(target_os)]`. Suggested: Windows `mica` (fall back to a solid colour if the OS build is not Win11), macOS `sidebar` or `underWindowBackground`.
3. Fall back gracefully. Win10 has no Mica; detect OS build and use a solid theme background. Also respect the Windows "Transparency effects" setting off and "reduce transparency" on macOS: the window then shows the opaque fallback, so the CSS needs a sensible `--background` anyway.
4. Set `backgroundColor` on the window (config reference: sets window and webview colour; on Windows the alpha is ignored for the window layer and, on Win8+, a non-zero alpha is ignored for the webview layer) to the theme colour matching the first paint to limit white flash.
5. Create windows hidden (`"visible": false`) and show them after the first frame/`window-state` restore to avoid the flash (recommended by the window-state docs for restore flicker, https://v2.tauri.app/plugin/window-state/).
6. For transparent windows on Windows the config reference says `noRedirectionBitmap` can help avoid a white flash at creation; the exact config key name was not confirmed here (see section 8).

Pitfalls:

- Mica needs a transparent webview; the effect is drawn by DWM behind the window, so any opaque pixel in the web content hides it. Tauri issue #1564 (white webview background flash) is long-standing and was still reported on Win11 per a 2024 comment. I found no confirmed v2-specific fix for `windowEffects` flashes.
- `acrylic` / `blur` drag performance: avoid unless the user accepts it.
- Windows `shadow`: undecorated + `shadow: true` gives Win11 rounded corners with a 1px border (config reference). `shadow: false` has no effect on decorated windows.
- Mica on Windows only applies if the user has "Transparency effects" on and, for the title-bar-integrated look, the window uses the native frame or extends into it. Tabbed is meant for tab-style title bars.
- Dark mode: effects `mica`/`tabbed` follow system theme, so set the window `theme` to match the app theme setting if the app has its own toggle (config `theme`, Windows and macOS 10.14+; also `setTheme` at runtime).

### 2.2 Title bar, traffic lights, drag regions, Snap layouts

macOS (config reference, TitleBarStyle):

- `titleBarStyle: "Overlay"` with `decorations: true` keeps the native traffic lights and shows web content under a transparent title bar. `hiddenTitle: true` hides the title text. `trafficLightPosition: { x, y }` repositions the buttons and requires `Overlay` plus `decorations: true`.
- Overlay caveats from the docs: title-bar height differs by OS version, title colour depends on system theme, and dragging only works when the window has focus, so add a drag region (below) and pad your top bar around the buttons (about 70-80 px on the left, verify visually per OS version).
- `Transparent` style (window background colour shows through) can be set in Rust via `WebviewWindowBuilder` and `objc2-app-kit` `NSWindow.setBackgroundColor` (https://v2.tauri.app/learn/window-customization/).
- A fully custom title bar on macOS loses some system window behaviours (move/align per the docs). Prefer Overlay.

Windows:

- Option A (recommended for native quality): keep native decorations (`decorations: true`). You keep Snap layouts, system caption buttons, Aero Snap, and correct Win11 rounded corners. With Mica you get the native look for free. Put app toolbar content inside the client area.
- Option B: `decorations: false` and a custom title bar. Required capability permissions: `core:window:default`, `core:window:allow-close`, `core:window:allow-minimize`, `core:window:allow-toggle-maximize`, `core:window:allow-start-dragging` (official page, section "Custom titlebar").
- `data-tauri-drag-region` applies only to the element carrying it, not its children; each child that should drag needs its own attribute, while buttons/inputs remain clickable. Alternative: `mousedown` handler calling `startDragging()` and `toggleMaximize()` when `e.detail === 2`. For touch/pen on Windows, the docs mention CSS `app-region: drag`.
- **Snap layouts with a custom title bar:** the official docs do not mention Snap layouts at all. A third-party write-up (dev.to, July 2026, single author, unverified) says the Win11 Snap flyout only appears when the top-level window answers `WM_NCHITTEST` with `HTMLAYOUT`-style maximize hit-test (`HTMAXBUTTON`), and WebView2's child window covers the client area so JS cannot trigger it; CSS hover still fires which looks like it should work. The community plugin `tauri-plugin-frame` (https://github.com/clarifei/tauri-plugin-frame) claims Snap-layout support for `decorations: false` on Win11. Treat as unverified; test on a real Win11 machine before relying on it. Practical conclusion: custom caption buttons lose Snap layouts unless you add native hit-testing, so default to native decorations on Windows.
- Per-monitor DPI: WebView2 and Tauri are per-monitor DPI aware by default; use `devicePixelRatio` and logical sizes (`LogicalSize`) in window APIs; test with 100%/150%/200% and moving between monitors. The window-state plugin docs do not discuss DPI (not verified).

### 2.3 Native menu bar (macOS) and Windows menu

Source: https://v2.tauri.app/learn/window-menu/.

- Build with Rust `MenuBuilder`/`SubmenuBuilder`/`PredefinedMenuItem` and `app.set_menu(menu)`, or JS `Menu.new({items}).then(m => m.setAsAppMenu())`. Handle clicks via `app.on_menu_event(|app, e| match e.id().0.as_str() {...})` or per-item JS `action`.
- macOS: menus appear in the global menu bar; all top-level items must be submenus and the first submenu becomes the app menu (About/Quit). Include predefined items (`copy`, `paste`, `undo`, `select_all`, `quit`, `about`, services, hide) so standard shortcuts work. Without an Edit menu, Cmd+C/V may not work in the webview on macOS.
- Windows/Linux: a menu set with `set_menu` renders inside the window frame; for a Mica/modern look many apps skip it on Windows and use in-app toolbar commands instead. Decide per platform with `#[cfg(target_os = "macos")]`.
- JS menu objects hold native handles: call `close()` or use `await using` (Tauri 2.12+).
- Submenu icons require Rust features `image-png` / `image-ico`.

### 2.4 Tray

Source: https://v2.tauri.app/learn/system-tray/.

- Enable Cargo feature `tray-icon` on `tauri`. Build with `TrayIconBuilder::new().icon(app.default_window_icon().unwrap().clone()).menu(&menu).build(app)?`.
- By default the menu shows on both left and right click; call `.show_menu_on_left_click(false)` to handle left click as "show window" (Windows convention).
- Tray click events do not fire on Linux (irrelevant here). macOS template icons (`icon_as_template`) were not covered by the guide; check the Rust API docs (not verified).
- Provide a monochrome template PNG for macOS and a coloured `.ico`/PNG for Windows (per platform conventions; not verified from Tauri docs).

### 2.5 Notifications and Windows app identity (AUMID)

- Plugin docs (https://v2.tauri.app/plugin/notification/): on Windows it "only works for installed apps" and shows the PowerShell name and icon in development. Desktop action buttons are not available (Actions API is mobile only). Permission flow: `isPermissionGranted()` then `requestPermission()` then `sendNotification()`; add `notification:default` to capabilities.
- AUMID: Tauri's docs do not discuss it. The `bundle.identifier` in `tauri.conf.json` is the reverse-DNS app identity and is what installed builds use. For correct toast identity in dev you would need to register an AUMID (registry key under `HKCU:\Software\Classes\AppUserModelId\<id>` plus a Start Menu shortcut carrying the AUMID), which general Windows guidance describes but I could not confirm as a Tauri-supported flow. Plan: test toasts only from the installed NSIS build; use the installed app for any "notification appears with our name and icon" check.
- Pick the `identifier` once and never change it (changing it changes the data folder, WebView2 profile, AUMID and updater lineage). Do not use an identifier ending in `.app` (Tauri warns on macOS for bundle extension clash; commonly cited, not verified here).
- `useHttpsScheme` (config reference): flipping it between releases changes the IndexedDB/cookies/localStorage location on Windows and orphans old data. Decide on day one.

### 2.6 Other plugins (setup notes)

- single-instance (https://v2.tauri.app/plugin/single-instance/): must be the first plugin registered; callback gets `(app, args, cwd)`; focus the existing window in it; `#[cfg(desktop)]`. No JS API and no capability needed.
- global-shortcut: no permissions by default; enable `global-shortcut:allow-register`, `allow-unregister`, `allow-is-registered`. Handler sees `Pressed`/`Released`. Registration can fail if another app owns the shortcut; handle the error (conflict behaviour is not covered by the docs).
- autostart: `MacosLauncher::LaunchAgent`; permissions `autostart:allow-enable/disable/is-enabled`; optional args list.
- window-state: add early; create windows `visible: false`; permission `window-state:default`. Test interaction with `windowEffects` and custom decorations (not documented).
- dialog / opener: add `dialog:default` and `opener:default` and scope paths/URLs narrowly.
- updater (https://v2.tauri.app/plugin/updater/): needs its own minisign key pair (`tauri signer generate`), `pubkey` in config, `TAURI_SIGNING_PRIVATE_KEY[_PASSWORD]` at build time (not read from `.env`), `bundle.createUpdaterArtifacts: true`, endpoints must be TLS, Windows `installMode` `passive|basicUi|quiet`. This signature is independent of OS code signing.

## 3. Desktop E2E testing

Official guidance (https://v2.tauri.app/develop/tests/webdriver/, fetched 2026-10-16): the recommended path is **WebdriverIO with `@wdio/tauri-service`**, supporting Windows, Linux and macOS. The same page says driving `tauri-driver` directly works only on Windows and Linux because macOS has no WKWebView driver tool; on macOS use the service's embedded server.

### 3.1 Driver providers (`driverProvider`)

From https://webdriver.io/docs/desktop-testing/tauri/configuration and `.../platform-support`:

| Provider | Needs | Platforms | Notes |
|---|---|---|---|
| `official` | `tauri-driver` (cargo) + `msedgedriver` (Windows) or `WebKitWebDriver` (Linux) | Windows, Linux | Default on Windows/Linux when nothing else is set... but the configuration page says the service throws an error on Windows/Linux if neither `embedded` signal nor an explicit provider is set. Set it explicitly |
| `embedded` | `tauri-plugin-wdio-webdriver` compiled into the app | Windows, Linux, macOS | Auto-selected on macOS or when `TAURI_WEBDRIVER_PORT` is set. Default port 4445 (`embeddedPort`, `WDIO_EMBEDDED_PORT`). `startTimeout` default 60 s |
| `crabnebula` | `@crabnebula/tauri-driver`, paid `CN_API_KEY` for macOS, `tauri-plugin-automation`, `@crabnebula/test-runner-backend` | all | Paid on macOS; skip |

Recommendation: `embedded` on macOS (the only practical option), and either `official` or `embedded` on Windows. Using `embedded` on both gives one code path; use `official` on Windows as a cross-check because it exercises the real WebView2 over Edge WebDriver.

### 3.2 Windows with `official` (tauri-driver + msedgedriver)

1. `cargo install tauri-driver --locked` (latest 2.1.0), or set `autoInstallTauriDriver: true` (requires cargo; first install takes minutes).
2. msedgedriver must match the installed **WebView2 runtime** version, not the Edge browser version. The service handles this by default: `autoDownloadEdgeDriver: true` (Windows only) detects the WebView2 version and downloads a matching driver. Mismatch error text: "This version of Microsoft Edge WebDriver only supports Microsoft Edge version X." Manual route: read the File version of the WebView2 `msedgewebview2.exe` (Properties > Details), download the same-version driver, set `autoDownloadEdgeDriver: false` (platform-support page). Tauri's CI example uses `msedgedriver-tool` from cargo to fetch a driver matching the installed Edge (https://v2.tauri.app/develop/tests/webdriver/ci/).
3. `tauri-driver` listens on port 4444 by default (`tauriDriverPort`; each worker adds its index).
4. `wdio.conf.ts` minimal:

```ts
export const config: WebdriverIO.Config = {
  runner: 'local',
  specs: ['./specs/**/*.e2e.ts'],
  capabilities: [{ browserName: 'tauri', 'tauri:options': { application: '<absolute path to exe>' } }],
  services: [['tauri', { driverProvider: 'official', autoDownloadEdgeDriver: true }]],
  framework: 'mocha',
};
```

### 3.3 Embedded provider (works on macOS and Windows)

From https://webdriver.io/docs/desktop-testing/tauri/plugin-setup:

1. `src-tauri/Cargo.toml`: add `tauri-plugin-wdio-webdriver = "1"` (and optionally `tauri-plugin-wdio = "1"` for `browser.tauri.execute()`, IPC mocking and log capture), as **optional** dependencies tied to a feature.
2. Register `tauri_plugin_wdio_webdriver::init()` (and `tauri_plugin_wdio::init()`) in the builder.
3. Capabilities: `wdio-webdriver:default` (grants no IPC commands, loads the manifest) and `wdio:default` (or `wdio:allow-execute`).
4. For `tauri-plugin-wdio`: set `app.withGlobalTauri: true` and import `@wdio/tauri-plugin` in the frontend entry.
5. WDIO config: `services: [['tauri', { driverProvider: 'embedded', appBinaryPath: '...' }]]`.

macOS (https://webdriver.io/docs/desktop-testing/tauri/platform-support): the embedded provider is auto-detected, needs the plugin registered and `wdio-webdriver:default`. The docs state no macOS limits for the embedded driver and I found no independent verification that it has WKWebView parity with a real WebDriver (see section 8). Apple's `safaridriver` cannot drive an arbitrary WKWebView, which is why the in-app server exists.

### 3.4 Keeping test hooks out of production, and testing an installed build

The plugin docs offer two gates: `#[cfg(debug_assertions)]` or an optional Cargo feature (for example `wdio`). `debug_assertions` is **not** suitable for testing an installed production build, because release profiles disable it. Use a feature:

```toml
# src-tauri/Cargo.toml
[features]
e2e = ["dep:tauri-plugin-wdio", "dep:tauri-plugin-wdio-webdriver"]
[dependencies]
tauri-plugin-wdio = { version = "1", optional = true }
tauri-plugin-wdio-webdriver = { version = "1", optional = true }
```

```rust
#[cfg(feature = "e2e")]
let builder = builder.plugin(tauri_plugin_wdio::init()).plugin(tauri_plugin_wdio_webdriver::init());
```

- The capability file referencing `wdio:default` is only valid when the plugin exists. Use a **separate capability file** (`capabilities/e2e.json`) and enable it only for e2e builds via an extra config file: `tauri build --features e2e --config src-tauri/tauri.e2e.conf.json`, where the e2e config lists the capability or sets `withGlobalTauri: true`. Production builds omit the feature, the config overlay and the frontend import of `@wdio/tauri-plugin` (gate the import behind `import.meta.env.VITE_E2E` so Vite tree-shakes it). A CI check should assert the release binary/capabilities contain no `wdio` strings.
- Official guidance for testing an **installed** build was not found. The service takes an `appBinaryPath` (absolute or relative to config dir); the docs mention only freshly built binaries. Practical approach (inferred, not documented): build the installer with `--features e2e`, install it per-user via NSIS silent flags to a throwaway location (`/S /D=<dir>`), then set `appBinaryPath` to the installed exe. This tests packaging, resources, WebView2 bootstrap and per-user data paths with the e2e feature compiled in. A truly feature-free production binary cannot be driven by WDIO; test it by smoke checks only (launch, process alive, UI Automation, or CDP below). State this limitation plainly in the test plan.
- Use an isolated user-data directory per run so tests do not touch real user data: set the app's own data-dir override via env var, and for WebView2 set `WEBVIEW2_USER_DATA_FOLDER` (Playwright WebView2 page).

### 3.5 Browser mode and in-process tests (fast layers)

- WDIO "browser mode" (docs landing page): runs the frontend in Chrome against the Vite dev server with `invoke()` intercepted and mockable; no Tauri binary needed. Good for UI flow tests in CI cheaply.
- Vitest layer: `mockIPC`, `mockWindows`, `clearMocks` from `@tauri-apps/api/mocks` (https://v2.tauri.app/develop/tests/mocking/). `shouldMockEvents: true` (since 2.7.0) mocks `listen/emit` partially. jsdom lacks WebCrypto: polyfill `window.crypto.getRandomValues` with Node `randomFillSync`.
- Rust layer: Tauri has a mock runtime (`tauri::test`) for command-level tests; the test index page links to it but does not name APIs (use docs.rs to confirm).

### 3.6 Playwright over CDP against WebView2 (Windows only alternative)

Source: https://playwright.dev/docs/webview2.

1. Launch the app with env `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` and a unique `WEBVIEW2_USER_DATA_FOLDER`.
2. `const browser = await chromium.connectOverCDP('http://localhost:9222'); const page = browser.contexts()[0].pages()[0];`
3. Parallel runs need a distinct port and user-data folder per worker. Kill the app tree after each test.
4. Limits: Windows/WebView2 only (WKWebView has no CDP), controls only web content (no native menus, tray, dialogs, window chrome), and requires the app to pass through those env vars. It works against any build without compiling test plugins, including an installed production build, because the env var is read by the WebView2 runtime. That makes it the best option for production-build smoke tests with axe (`@axe-core/playwright` accepts a `page`) on Windows. Not officially listed by Tauri as a supported approach; this is an inferred combination (Tauri uses WebView2, Playwright documents WebView2 attach).
5. Caveat: Playwright's Windows WebView2 docs are written for WinForms hosts; with Tauri it is the same runtime, but verify on the first run (not tested).

### 3.7 Accessibility testing

- In Vitest + Testing Library: run `axe-core` directly (`axe.run(container)`) in a small helper and assert `violations` is empty; `vitest-axe` is stale (0.1.0, 2022), `jest-axe` 11.0.0 targets Jest.
- In Playwright (browser mode against the Vite dev server, or CDP against WebView2): `@axe-core/playwright` 4.14.0 (`new AxeBuilder({ page }).analyze()`).
- jsdom cannot evaluate colour contrast; contrast checks need a real browser (Playwright).

## 4. Recommended test pyramid for this app

1. Rust unit and mock-runtime tests (`cargo test`) for commands.
2. Vitest + Testing Library + `mockIPC` for React components and hooks; axe checks in jsdom.
3. WDIO browser mode or Playwright against `vite preview` for UI flows and contrast/axe.
4. WDIO `@wdio/tauri-service` with `embedded` provider against an `e2e`-feature build on Windows and macOS runners (native window, IPC, menus via IPC mocks, single-instance, tray not scriptable via WebDriver).
5. Installed-build smoke test on Windows: silent-install the NSIS package, launch, confirm via Playwright-over-CDP or WDIO (if built with the e2e feature) that the main window renders and a notification toast is raised (toast visibility itself cannot be asserted via WebDriver; check manually).
6. Manual visual checks for Mica, Snap layouts, DPI, traffic lights (no reliable automation; document a checklist).

## 5. GitHub Actions

Runner images (https://github.com/actions/runner-images, README fetched 2026-10-16):

- `windows-latest` = `windows-2025` (Server 2025, x64); `windows-2022`; `windows-11-arm` (arm64).
- `macos-latest` = macOS 26 arm64 (`macos-26`); Intel is `macos-26-intel` / `macos-latest-large`; `macos-15` (arm64) and `macos-15-intel`; macOS 14 images are deprecated. `-latest` migrations roll out over 1-2 months, so pin (for example `windows-2025`, `macos-26`) for reproducibility.
- Windows 2025 image (version 20261013.288.2): Microsoft Edge 155.0.4283.45 and Edge Driver 155.0.4283.45, Rust 1.99.0, Node 22.23.3, VS Enterprise 2022 17.14, WiX Toolset 3.14.1.8722. The image readme does **not** list the WebView2 runtime or NSIS. Tauri's own CI builds NSIS bundles on `windows-latest` (tauri-action downloads NSIS/WiX via the bundler), and the WDIO and Tauri CI examples run `windows-latest`, so WebView2 is expected to be present, but I could not verify it in the image manifest. Add a CI step that reads the WebView2 version from the registry (`HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}` `pv`) and fails fast with a clear message, and the service's auto driver download handles the matching.
- macOS 26 arm64 image: Node 24.20.0, Rust 1.98.1, Xcode CLT 26.6, Safari 26.6.2 and SafariDriver (not usable for WKWebView apps), Xcode 27 preview exists.
- Tauri's official workflow (https://v2.tauri.app/distribute/pipelines/github/): `actions/checkout`, `actions/setup-node`, `dtolnay/rust-toolchain@stable` (add macOS targets), `swatinem/rust-cache@v2` with `workspaces: './src-tauri -> target'`, then `tauri-apps/tauri-action@v1` with `tagName`, `releaseName`, `releaseDraft: true`, `args: --target ...` and `permissions: contents: write`. The page lists action versions `checkout@v7` and `setup-node@v6`; check the current major before copying. The page's example uses npm; use `pnpm/action-setup` (reads `packageManager`) then `actions/setup-node` with `cache: pnpm`.
- Caching: pnpm via `setup-node` `cache: pnpm` (requires pnpm to be installed first); cargo via `swatinem/rust-cache@v2` (also caches `~/.cargo/registry`). Cache the downloaded edgedriver is unnecessary (it is fast).
- Signing secrets pattern (names from Tauri docs; supply your own values, never commit):
  - macOS (https://v2.tauri.app/distribute/sign/macos/): `APPLE_CERTIFICATE` (base64 `.p12`), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `KEYCHAIN_PASSWORD` (temp keychain), plus notarization either `APPLE_API_ISSUER` + `APPLE_API_KEY` + `APPLE_API_KEY_PATH`, or `APPLE_ID` + `APPLE_PASSWORD` (app-specific) + `APPLE_TEAM_ID`. Notarization is required for Developer ID certs; free accounts cannot notarize. Ad-hoc signing (`signingIdentity: "-"`) still triggers the Privacy & Security approval.
  - Windows (https://v2.tauri.app/distribute/sign/windows/): OV certificate as base64 `.pfx` + password secrets imported to the runner store, then `certificateThumbprint`, `digestAlgorithm`, `timestampUrl` in `bundle.windows`; or `bundle.windows.signCommand` with Azure Key Vault (`relic`) / Azure Artifact Signing (formerly Trusted Signing) using `AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, `AZURE_CLIENT_SECRET`. Since 2024 EV no longer grants instant SmartScreen reputation; reputation accrues per certificate.
  - Updater: `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- E2E in CI: the Tauri CI sample runs `cargo install tauri-driver --locked`, the msedgedriver-tool step on Windows, and Node 24; for the service's `embedded` provider none of those are needed. On macOS the app launches a real GUI session on the hosted runner (hosted macOS runners have a logged-in session; not verified for WKWebView window creation, see section 8).
- Repo note: `C:\Projects\relay\CLAUDE.md` for the Python Relay says "No GitHub Actions: tests and checks run locally only (D-050)". Check `C:\Projects\relay-rust\CLAUDE.md` and decisions before adding workflows; this document does not override that rule. If Actions are disallowed, the macOS verification needs a Mac the user controls.

## 6. Installers

Source: https://v2.tauri.app/distribute/windows-installer/.

- NSIS (`-setup.exe`): can be built on Windows (and cross-compiled with `cargo-xwin`, "last resort"); `installMode`: `currentUser` (default, no admin, `%LOCALAPPDATA%`), `perMachine` (admin, Program Files), `both` (prompts, needs admin). Supports multiple languages in one installer, `installerHooks` (pre/post install/uninstall), custom `.nsi` template, `displayLanguageSelector`. Header/sidebar image theming exists in the NSIS config (`headerImage`, `sidebarImage`, `installerIcon`) but was not on the fetched page (not verified); expect to be limited to images, language strings and hooks unless you replace the template.
- WiX/MSI: Windows host only, WiX v3 (GitHub image has 3.14.1.8722), needs the VBSCRIPT Windows feature, one installer per language, enterprise/GPO friendly. Per-user vs per-machine for MSI is not described on the page.
- Recommendation: NSIS per-user (`currentUser`) as the primary installer: no UAC, works with the updater's `passive` mode and `quiet` mode. Offer MSI later only if enterprise deployment is required.
- WebView2 bootstrap options (`webviewInstallMode`): `downloadBootstrapper` (default, needs internet, 0 MB), `embedBootstrapper` (~1.8 MB), `offlineInstaller` (~127 MB), `fixedRuntime` (~180 MB, you patch it), `skip`. Windows 10 (April 2018+) and 11 ship WebView2. `minimumWebview2Version` triggers the bootstrapper when too old.
- ARM64: install VS "C++ ARM64 build tools" and use `aarch64-pc-windows-msvc`; GitHub offers `windows-11-arm` runners.
- macOS DMG (https://v2.tauri.app/distribute/dmg/): `bundle.macOS.dmg` supports `background`, `windowSize` (default 660x400), `windowPosition`, `appPosition`, `applicationFolderPosition`; build on a Mac with `tauri build --bundles dmg`. Known issue: icon sizes and positions are not applied when DMGs are created on CI (tauri-apps/tauri#1731). Notarize and staple (`--skip-stapling` available), universal builds need both `aarch64-apple-darwin` and `x86_64-apple-darwin` targets or `--target universal-apple-darwin` (the latter not verified on the fetched page).

## 7. Step-by-step recommended setup (order of work)

1. Pin toolchains: Node 24 via fnm, pnpm via `packageManager`, Rust 1.90+ (stable), Tauri 2.12.x crates and `@tauri-apps/*` 2.12.x JS. Keep crate and npm versions of the same plugin on matching minor.
2. Frontend: Vite 8 + `@vitejs/plugin-react` 6 + Tailwind 4.3 via `@tailwindcss/vite` and `@import "tailwindcss"`; run `pnpm dlx shadcn@latest init -t vite` (or configure the `@/*` alias manually). Add TanStack Query/Table (v9 API)/Virtual, react-router 8 (declarative mode), zod 4, zustand 5, motion 14.
3. Native shell: `transparent: true` window with `windowEffects` per OS, CSS-transparent root, native decorations on Windows, `Overlay` + `hiddenTitle` + `trafficLightPosition` on macOS, native app menu on macOS with predefined items, tray with `show_menu_on_left_click(false)`, single-instance first, window-state with hidden-until-restored windows.
4. Capabilities: minimum per window; one capability file per concern; `e2e.json` separate and only enabled in e2e builds.
5. Test layers per section 4 with webdriverio pinned to 9.x.
6. Packaging: NSIS `currentUser`; DMG; updater keys; signing secrets in GitHub encrypted secrets only if Actions are permitted.

## 8. Could not be verified (explicit list)

- Exact name of the Windows `noRedirectionBitmap` window config key and whether it is a Tauri config field vs. a builder method (only mentioned inside the `transparent` description).
- Whether WebView2 Runtime is installed on `windows-2025` runners (image readme lists Edge 155.0.4283.45 only) and which WebView2 version it has.
- Whether the WDIO embedded provider on macOS has any functional gaps versus WKWebView automation (docs list none; no independent test).
- Whether hosted macOS runners can create and screenshot a Tauri WKWebView window reliably for e2e (not tested; WDIO CI page returned 404).
- Any official instructions for running WDIO against an installed (signed, installer-deployed) production build; the NSIS silent-install approach in 3.4 is my inference. WDIO usage-examples and configuration pages do not cover it.
- Snap layouts with `decorations: false`: only third-party claims (a dev.to post and `tauri-plugin-frame`); no official Tauri statement and no hardware test.
- AUMID registration steps for unpackaged Tauri apps and toast activation identity; Tauri docs are silent; web results were generic.
- White-flash behaviour and Mica correctness for Tauri 2.12.4 specifically (only an old issue #1564 and generic advice found).
- `Window::set_effects` / `setEffects` exact signatures and platform tauri.*.conf.json merge behaviour (pages not opened).
- TypeScript 7.0.2 compatibility with the shadcn CLI, Vitest 5 and the ESLint stack; React Table v9 migration details; Tailwind Node/browser support matrix.
- macOS tray template-icon API, autostart `MacosLauncher` variants, global-shortcut conflict behaviour, window-state + DPI/multi-monitor behaviour (docs do not cover them).
- `--target universal-apple-darwin` support in tauri-action 1.x and current action major versions (docs sample showed checkout@v7, setup-node@v6; not independently checked).
- NSIS `headerImage`/`sidebarImage` config key names and MSI per-user options (not on the fetched page).
- `tauri-driver` README (404) and Tauri 2 macOS minimum OS requirement.

## 9. Source URLs

- https://v2.tauri.app/develop/tests/webdriver/ ; /ci/ ; /example/webdriverio/ ; https://v2.tauri.app/develop/tests/ ; https://v2.tauri.app/develop/tests/mocking/
- https://webdriver.io/docs/desktop-testing/tauri/ ; /configuration ; /plugin-setup ; /platform-support ; /usage-examples
- https://playwright.dev/docs/webview2
- https://v2.tauri.app/reference/config/ ; /learn/window-customization/ ; /learn/window-menu/ ; /learn/system-tray/
- https://v2.tauri.app/plugin/{notification,single-instance,updater,window-state,autostart,global-shortcut}/
- https://v2.tauri.app/distribute/{windows-installer,dmg,pipelines/github}/ ; /distribute/sign/{macos,windows}/
- https://github.com/actions/runner-images (README, Windows2025-Readme.md, macos-26-arm64-Readme.md)
- https://ui.shadcn.com/docs/installation/vite ; https://tailwindcss.com/docs/installation/using-vite
- https://registry.npmjs.org/<package> and https://crates.io/api/v1/crates/<crate> for all versions
- Third party (unverified): https://dev.to/zbrooklyn/windows-11-snap-layouts-in-a-frameless-tauri-app-the-part-every-guide-gets-wrong-2fjh ; https://github.com/clarifei/tauri-plugin-frame ; https://github.com/tauri-apps/tauri/issues/1564
