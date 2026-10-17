# Relay design system

Status: implemented in `ui/` (October 2026). Source of truth for values: `ui/src/styles/tokens.css`.
Live reference: the component gallery at `#/gallery` in `pnpm dev` (development builds only).
Research behind it: `docs/research/design-language.md` (palettes, contrast, motion) and
`docs/research/native-and-testing.md` (window materials, title bars, testing).

## 1. Principles

- **Calm and warm.** Warm paper and charcoal neutrals, a serif for headings, one restrained accent.
  Separation comes from value steps first (sidebar, canvas, cards are different tones; there is no
  line between the sidebar and the content) and soft 1px hairlines only inside cards, never from
  gradients or heavy shadows.
- **Proposal, ran, verified.** The interface always shows the difference between what an AI
  proposed, what Relay ran, and what Relay checked afterwards. Only "Verified" means checked.
- **Fast to drive.** Every action is reachable from the keyboard (palette, shortcuts, roving lists),
  menus and dialogs follow platform conventions, details are progressively disclosed.
- **Own identity.** Relay's mark, palette and voice are original. No third-party logos, fonts or
  product names appear in the UI. "Claude Code" and "Codex" appear only as plain-text names of the
  two provider integrations.

## 2. Tokens

All tokens are CSS custom properties on `:root`, exposed to Tailwind v4 through `@theme inline`
in `ui/src/styles/index.css` (the default Tailwind palette is removed so only Relay tokens exist).
Themes are selected with attributes on `<html>`: `data-mode` (light/dark), `data-theme`
(relay/graphite/midnight), `data-accent` (terracotta/sage/indigo/plum/ink), `data-material`
(none/mica/vibrancy) and `data-titlebar` (native/overlay). `public/boot.js` sets the first three
before the first paint (no flash); the appearance store keeps them current.

### 2.1 Colour (Relay theme)

| Token | Tailwind | Light | Dark | Use |
|---|---|---|---|---|
| `--relay-bg` | `bg-bg` | #FAF7F2 | #1C1A17 | canvas |
| `--relay-sidebar` | `bg-sidebar` | #F1EDE4 | #151311 | sidebar, settings nav |
| `--relay-surface` | `bg-surface` | #FFFEFB | #262320 | cards, composer, inputs |
| `--relay-surface-raised` | `bg-raised` | #FFFEFB | #2C2925 | menus, dialogs, popovers |
| `--relay-surface-sunk` | `bg-sunk` | #EFEAE0 | #121110 | code, segmented tracks, user bubble |
| `--relay-hover` / `--relay-active` | `bg-hover` / `bg-active` | ink 5% / 9% | cream 6% / 10% | row states |
| `--relay-border` | `border-line` | #E4DED2 | #34302A | decorative hairline |
| `--relay-border-strong` | `border-line-strong` | #8F887A | #7A7365 | control boundaries (>= 3:1) |
| `--relay-text` | `text-fg` | #1F1D1A | #F2EDE4 | primary text |
| `--relay-text-2` | `text-fg-2` | #4A463F | #C9C2B5 | secondary text |
| `--relay-text-muted` | `text-fg-muted` | #6B665B | #9A9385 | captions, meta |
| `--relay-accent` | `bg-brand` | #B4532F | #E58A67 | the one primary action, focus ring |
| `--relay-accent-hover` | `bg-brand-hover` | #9C4526 | #EE9C7C | hover of brand fills |
| `--relay-accent-text` | `text-brand-text` | #9C4526 | #E58A67 | accent-coloured text |
| `--relay-on-accent` | `text-on-brand` | #FFFFFF | #1C1A17 | label on brand fill |
| `--relay-accent-soft` | `bg-brand-soft` | #F4E3DA | #3A2A23 | selected cards, avatar |
| `--relay-danger` / `-soft` | `text-danger` / `bg-danger-soft` | #A63A32 / #F6E4E1 | #F0877D / #3B2220 | failures, destructive |
| `--relay-success` / `-soft` | | #3A6A27 / #E5EEDC | #86C060 / #23301B | verified |
| `--relay-warning` / `-soft` | | #7A5A12 / #F3EAD3 | #D9A93F / #352B14 | approvals, sensitive |
| `--relay-info` / `-soft` | | #2F5F9A / #E1EAF5 | #8AB4E8 / #1F2A38 | cloud, shared context, Cloud AI chips |
| `--relay-violet` / `-soft` | `text-violet` / `bg-violet-soft` | #6A4AA6 / #ECE6F6 | #B9A4EE / #2A2438 | MCP chips (e.g. Playwright browser) |
| `--relay-code-bg` | | = surface | = surface-sunk | code blocks (keeps syntax colours >= 4.5:1) |

The accent is a deeper terracotta than the reference product's clay (#D97757/#CC785C fail AA as a
fill with a light label); it is Relay's own value. Alternative accents (sage, indigo, plum, ink)
and themes (Graphite: cool greys; Midnight: true black/paper white) are defined in `tokens.css`
and pass the same contrast checks. Users may also pick their own background and text colour;
secondary and muted text are then derived so they keep >= 7:1 and >= 4.8:1 (`customVars` in
`lib/theme.ts`).

**Window materials.** With `data-material="mica"` (Windows 11) or `"vibrancy"` (macOS) the page
background is transparent and only the canvas and sidebar use translucent tokens
(`--relay-bg-translucent`, `--relay-sidebar-translucent`, 72-80% and 50-55% opacity). Cards, menus
and inputs stay opaque so text contrast never depends on the wallpaper. `prefers-reduced-transparency`
restores solid colours. With `"none"` everything is solid.

### 2.2 Typography

| Role | Token / utility | Size / line | Face | Notes |
|---|---|---|---|---|
| Display | `text-display` + `font-display` | 36/42, -0.015em | Newsreader (opsz 48) | greeting (30/42 on narrow) |
| Title | `text-title` | 26/32, -0.01em | Newsreader | page titles, settings sections |
| Heading | `text-heading` | 17/24, 600 | Inter | section headings |
| Message | `text-message` | 15.5/26 | Inter (or Newsreader 16.5/27 when "Answer font: Serif") | assistant answers |
| Body | `text-body` | 15/24 | Inter | composer, user messages |
| UI | `text-ui` | 14/20 | Inter | rows, buttons, labels |
| Caption | `text-caption` | 12.5/16 | Inter | meta, descriptions |
| Micro | `text-micro` | 11.5/14 | Inter | section labels, badges |
| Code | `text-code` | 13/20 | Geist Mono | code blocks, tool names, paths |

Serif is used for headings, the greeting, the plan objective and dialog titles only; chrome and
tool output stay sans for density.

### 2.3 Space, radii, lines, layout

- Space: 4px base, 8px rhythm (`--relay-space-1` 4 … `--relay-space-16` 64). Rows are 32px
  (`h-8`) in lists and menus, controls 32-36px, primary buttons 40px.
- Radii: xs 4, sm 6, md 8 (controls), lg 12 (cards, menus), xl 16 (timeline card, dialogs),
  composer 22, full for pills.
- Lines: 1px hairlines in `--relay-border` (decorative, ~1.5:1); control boundaries use
  `--relay-border-strong` (>= 3:1, WCAG 1.4.11).
- Layout: sidebar 280px (rail 56px), transcript column 768px, header 48px. Narrow below 768px:
  the sidebar becomes an overlay drawer.
- Title bar: `--relay-titlebar-inset` (76px) and `--relay-titlebar-top` (28px) are non-zero only
  for the macOS overlay title bar, reserving the traffic lights; top bars carry
  `data-tauri-drag-region`. On Windows the window is frameless (`data-titlebar="custom"`): the
  app draws minimise/maximise/close (`components/shell/WindowControls.tsx`) and top bars reserve
  `--relay-controls-w` (138px) on the right.
- Hairlines: inside cards they use `--relay-border` at 60-70% (`border-line/60`, `/70`); the
  sidebar, settings navigation and code-block header separate by tone only.

### 2.4 Elevation

| Token | Use |
|---|---|
| `--relay-shadow-sm` | segmented thumb, secondary buttons |
| `--relay-shadow-card` | composer, task timeline card, provider cards |
| `--relay-shadow-pop` | menus, popovers, tooltips, toasts |
| `--relay-shadow-dialog` | dialogs, command palette |

Shadows are soft and short; cards rely on value contrast plus a hairline first.

### 2.5 Motion

| Token | Value | Use |
|---|---|---|
| `--relay-dur-fast` | 100ms | hover, press, toggles |
| `--relay-dur` | 160ms | menus, disclosures, fades |
| `--relay-dur-panel` | 240ms | sidebar slide, drawer |
| `--relay-ease` | cubic-bezier(0.2, 0, 0, 1) | default |

Only `transform` and `opacity` animate. The sidebar collapse slides the panel with a transform
(its right 56px remains as the rail) and the main column follows with a FLIP transform, so no
frame re-lays out the transcript. A running step shows a slow light sweep; skeletons pulse
opacity. Under `prefers-reduced-motion: reduce` all animations and transitions drop to ~0ms, the
shimmer and sweep are removed and the pulse dot is static.

**Motion for React was not added.** Every interaction here is a CSS transition or keyframe
(tw-animate-css for Radix enter/exit). No interaction needed physics, layout animation or
gesture handling, so the dependency would only add weight.

### 2.6 Z-index and focus

Z-index scale: sticky 10, sidebar 30, overlay 40, drawer 45, popover 60, dialog 80, palette 85,
toast 100, tooltip 110. Focus: a 2px outline in the accent colour with 2px offset on
`:focus-visible` everywhere (inputs add a soft ring); in forced-colours mode the system
highlight colour is used.

## 3. Components

Primitives are shadcn/ui (initialised with the shadcn CLI, Radix based) and then rewritten to
Relay tokens; they live in `ui/src/components/ui/`.

| Component | File | Notes |
|---|---|---|
| Button | `button.tsx` | primary (brand), secondary (surface + hairline), outline, ghost, danger, danger-ghost, link; sm/md/lg + icon sizes |
| IconButton | `icon-button.tsx` | always has an accessible name and a tooltip with its shortcut |
| Badge | `badge.tsx` | neutral/outline/brand/success/danger/warning/info; always carries a word |
| Tooltip | `tooltip.tsx` | `Tip` wrapper; inverted colours; shows the platform shortcut |
| DropdownMenu, ContextMenu | `dropdown-menu.tsx`, `context-menu.tsx` | shared row styling; destructive variant |
| Dialog, AlertDialog | `dialog.tsx`, `alert-dialog.tsx` | serif title, hairline, dialog shadow; Esc closes |
| Popover, Select | `popover.tsx`, `select.tsx` | raised surface |
| Switch | `switch.tsx` | brand track when on; 3:1 boundary when off |
| Segmented | `toggle-group.tsx` | one tab stop, arrow keys, exposed as a radio group |
| RadioGroup, RadioCard | `radio-group.tsx` | cards for privacy modes and themes |
| Input, Textarea, Label | `input.tsx`, `textarea.tsx`, `label.tsx` | 3:1 boundary, accent focus |
| Progress, ProgressRow | `progress.tsx` | scaleX bar; indeterminate sweep |
| Skeleton | `skeleton.tsx` | opacity pulse placeholders (no spinners for loading) |
| Kbd, Shortcut | `kbd.tsx` | Ctrl on Windows, Command symbol on macOS from one helper (`lib/platform.ts`) |

Product components:

- **Task timeline** (`components/task/TaskTimeline.tsx`): route badge (On this PC / Relay Local
  AI / Cloud / Unavailable), the route reason and the "what did the work" chips (`UsedChips.tsx`:
  Local AI green, Cloud AI blue, Native tools neutral, MCP servers such as "Playwright browser"
  violet; icon + words, never colour only); privacy ledger row "Shared with <provider> · N
  categories · N items · size" (expandable table) or "Nothing was shared"; the plan labelled
  "Proposed by <source> · Relay checks each step as it goes"; numbered steps with their `via` chip
  and only two risk badges: "Changes or deletes data" (destructive) and "Sensitive info" (only when
  the step has `sensitive_data`; `risk: "sensitive"` alone shows nothing); live states Up next,
  Working (sweep), Checked (filled success check), "Done, not checked" (neutral outlined tick),
  "Didn't work" / "Blocked" (red, opens with the reason), Skipped, Stopped; lists longer than 6
  steps collapse to the latest step plus failures ("Show all N steps") for web tasks and finished
  runs; inline approval line with countdown; verdict footer in plain words ("Done · all 3 steps
  checked", "Done · 2 of 3 steps checked", "Done, but not checked", "Step 2 didn't work",
  "Blocked at step 2", "Stopped", "Not done: you said no", "Waiting for you") with a one-line hint;
  Retry task; raw log behind "Details".
- **Chat answers** (`components/chat/Messages.tsx`): a question (`route.intent === "chat"`) is a
  plain assistant message with its chips and, for cloud answers, a compact privacy ledger; no task
  card. A message that asks the user to act (web tasks) is a "Your turn" callout.
- **Code blocks** (`components/chat/CodeBlock.tsx`): language label, Copy button ("Copied" state,
  `aria-label`, keyboard reachable), shiki highlighting loaded on demand and applied once when the
  fence closes (no re-highlight per streamed token), `white-space: pre` with horizontal scroll,
  Geist Mono without ligatures, `--relay-code-bg`. Inline code is a small tinted chip.
- **Permission dialog** (`PermissionDialog.tsx`): blocking alert dialog, risk wording, detail in
  mono, 2-minute countdown with a bar, Decline focused first for destructive actions, Esc declines.
- **Agent group** (`ToolGroup.tsx`): "Worked for 12s · 7 steps", collapsible, tool names inside.
- **Composer and context controls** (`components/chat/`): centred card with attach, project chip,
  round send/stop; under it the privacy-mode segmented control and the provider picker (status
  per provider, unavailable ones stay listed with the reason and the command to fix it).
- **Sidebar** (`components/shell/Sidebar.tsx`), **search palette**, **task history** page,
  **settings** page, **installer** screens.

## 4. Accessibility

- Contrast is verified automatically: `ui/src/styles/tokens.test.ts` parses `tokens.css`, resolves
  every theme x mode x accent (3 x 2 x 5 = 30 combinations) and asserts AA for every text pair
  (4.5:1; 7:1 for primary text) and 3:1 for control boundaries and the focus ring. Playwright runs
  axe-core with colour contrast on Home, Task history and AI & Privacy in both modes.
- Measured ratios, default theme (from the token test):

| Pair | Light | Dark |
|---|---|---|
| text on bg | 15.73 | 14.89 |
| text on surface | 16.67 | 13.40 |
| text-2 on bg | 8.78 | 9.81 |
| text-muted on bg | 5.34 | 5.69 |
| text-muted on sidebar | 4.89 | 6.08 |
| text-muted on surface-raised (menus) | 5.66 | 4.75 |
| text-muted on surface-sunk | 4.76 | 6.18 |
| on-accent on accent (primary button) | 4.98 | 6.75 |
| accent-text on bg | 5.97 | 6.75 |
| accent-text on accent-soft | 5.12 | 5.32 |
| danger / success / warning / info on their soft fills | 5.23 / 5.38 / 5.31 / 5.36 | 5.90 / 6.44 / 6.43 / 6.76 |
| violet on violet-soft / on surface | 5.48 / 6.62 | 6.82 / 7.15 |
| on-danger on danger (destructive button) | 6.42 | 7.00 |
| border-strong on bg (control boundary) | 3.29 | 3.69 |
| focus ring on bg | 4.66 | 6.75 |

- Keyboard: global shortcuts (below); roving tabindex (one tab stop, arrows, Home/End) in the
  sidebar list, plan steps, settings navigation and segmented controls; the task history list is a
  listbox with `aria-activedescendant`; the palette is a combobox; F2 renames and Delete asks to
  delete the focused chat; Esc closes menus, dialogs, the drawer and settings; Esc in the composer
  stops a running task.
- Screen readers: polite live region announces task start, plan, approval requests, failures and
  the result; streaming answers are `aria-busy`; status is never colour-only (every dot or chip has
  a word); icons are `aria-hidden` with labelled buttons.
- Reduced motion and reduced transparency are honoured; forced colours keep focus visible.

### Keyboard shortcuts

| Action | Windows | macOS |
|---|---|---|
| Search and commands | Ctrl+K | ⌘K |
| New chat | Ctrl+N (also Ctrl+Shift+O) | ⌘N |
| Show/hide sidebar | Ctrl+B (also Ctrl+Shift+S) | ⌘B |
| Task history | Ctrl+Shift+H | ⌘⇧H |
| Settings | Ctrl+, | ⌘, |
| Keyboard shortcuts | Ctrl+/ | ⌘/ |
| Focus the message box | Shift+Esc | ⇧Esc |

## 5. Fonts and licences

All fonts are bundled from npm (no CDN); each package's `LICENSE` was read before use and a copy
ships in `ui/public/licenses/`.

| Font | Package | Version | Licence | Copyright line in LICENSE |
|---|---|---|---|---|
| Inter (variable, wght) | `@fontsource-variable/inter` | 5.3.0 | SIL OFL 1.1 | 2016 The Inter Project Authors |
| Newsreader (variable, opsz+wght) | `@fontsource-variable/newsreader` | 5.3.0 | SIL OFL 1.1 | 2020 The Newsreader Project Authors (Production Type) |
| Geist Mono (variable, wght) | `@fontsource-variable/geist-mono` | 5.3.0 | SIL OFL 1.1 | 2024 The Geist Project Authors |

Fallbacks: Segoe UI Variable / system-ui, Georgia, Cascadia Mono / Consolas. Fontsource splits
each font by unicode range, so only the subsets a page uses are loaded.

## 6. Decisions and trade-offs

- **Density control: not added.** Rows are already 32px and the transcript is reading-oriented;
  a second density would double the visual-regression surface for little gain.
- **TanStack Table: not used.** Task history is a single-column virtualised listbox with a
  filter; it has no sortable columns, so a table abstraction would add weight without benefit.
- **Transcript virtualisation** switches on above 40 items (TanStack Virtual, loaded lazily);
  shorter chats render directly with `content-visibility: auto`.
- **Confirmation dialog vs inline card:** the contract makes confirmations blocking, so the
  modal dialog is the decision point; the timeline shows the same request inline with its state.
- **Two approval settings:** `Hybrid.confirm_level` (plan steps) lives in AI & Privacy;
  `Settings.approval_mode` (agent mode) stays in General with an explanation. See the backend
  requests in the UI report if these should merge.
