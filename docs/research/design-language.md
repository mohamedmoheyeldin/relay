# Design language research: Claude (primary) and ChatGPT (secondary)

Date: 2026-10-16. Purpose: ground an independent "Relay" design system. Nothing here
authorises copying logos, fonts or assets; only observed values and patterns.

## Method and confidence

- Tool limits: claude.ai, help.openai.com and support pages returned HTTP 403 or 429 to
  WebFetch; I did not log in. I could NOT read claude.ai's live computed CSS. So there are
  no verified values for the Claude web/desktop app's own sidebar, composer or message
  layout. Those rows are marked UNVERIFIED and carry my recollection or a third-party hint.
- Confidence tags: **[OFFICIAL]** Anthropic documentation; **[3P]** third-party extraction
  of anthropic.com (not the app); **[3P-SHORTCUT]** third-party shortcut list; **[UNVERIFIED]**
  recollection or inference, to be confirmed by inspecting the running app.
- Best primary source found: Anthropic's MCP Apps design guidelines, which publish the
  host style tokens Claude uses for light and dark mode (exact hexes below).

## Sources

- [OFFICIAL] https://claude.com/docs/connectors/building/mcp-apps/design-guidelines
- [OFFICIAL] https://claude.com/docs/connectors/building/mcp-apps/transparent-theming
- [OFFICIAL] https://support.claude.com/en/articles/12626668 (desktop quick entry, voice)
- [3P] https://kombai.com/examples/design-systems/claude-kombai (anthropic.com extraction)
- [3P] https://www.webdesignhot.com/design.md/anthropic/ (anthropic.com extraction, internally inconsistent)
- [3P] https://www.skills.sh/anthropics/skills/brand-guidelines (brand palette skill)
- [3P] https://github.com/motgenror/claude-css/blob/main/README.md and
  https://gist.github.com/maksim-volkmann/d798f161fb6613fa04895c3e7bd855dc (userstyles; variable names only)
- [3P-SHORTCUT] https://claudeimplementation.com/blog/claude-keyboard-shortcuts
- [3P-SHORTCUT] https://fastshortcuts.com/shortcuts/chatgpt/
- [3P-SHORTCUT] https://www.hongkiat.com/blog/chatgpt-desktop-app-keyboard-shortcuts/ (macOS)
- Search results only (not fetched): https://community.openai.com/t/shortcut-key-customization-and-enhanced-chat-toggle-functionality-for-chatgpt-windows-desktop-app/1022183

## 1. Colour

### 1.1 Claude host tokens [OFFICIAL] (MCP Apps design guidelines)

| Role | Light | Dark |
|---|---|---|
| background-primary (cards, inputs) | #FFFFFF | #30302E |
| background-secondary | #F5F4ED | #262624 |
| background-tertiary (page canvas) | #FAF9F5 | #141413 |
| background-inverse | #141413 | #FAF9F5 |
| text-primary | #141413 | #FAF9F5 |
| text-secondary | #3D3D3A | #C2C0B6 |
| text-tertiary (muted) | #73726C | #9C9A92 |
| border-primary / secondary / tertiary | #1F1E1D at 40% / 30% / 15% | #DEDCD1 at 40% / 30% / 15% |
| info bg / text / border | #D6E4F6 / #3266AD / #4682D5 | #253E5F / #80AADD / #4682D5 |
| danger bg / text / border | #F7ECEC / #7F2C28 / #A73D39 | #602A28 / #EE8884 / #CD5C58 |
| success bg / text / border | #E9F1DC / #265B19 / #437426 | #1B4614 / #7AB948 / #599130 |
| warning bg / text / border | #F6EEDF / #5A4815 / #805C1F | #483A0F / #D1A041 / #A87829 |
| ring primary | #141413 at 70% | #FAF9F5 at 70% |

Reading: Claude's dark mode is warm charcoal, not black: canvas #141413 is the deepest
layer and cards are lighter (#30302E). Hairline borders are 0.5px at 15% ink.
Caveat: these are tokens for embedded apps; they match the host theme but the app shell's
own sidebar/canvas hexes are UNVERIFIED (not published).

### 1.2 Brand and marketing palette [3P]
- Clay #D97757 (logomark/rare accent); emphasised clay #C6613F; web "brand" #CC785C
  (sources conflict on which is canonical; treat as UNVERIFIED which the app uses).
- Cream #FAF9F5, ink #141413, mid gray #B0AEA5, light gray #E8E6DC (skills.sh, consistent
  with the official tokens), panel #F0EEE6 / #F0EFE6, hairline #E7E6E1.
- Earth accents: kraft #D4A27F, manilla #EBDBBC, oat #E3DACC, coral #EBCECE, fig #C46686,
  heather #CBCADB, cactus #BCD1CA, olive #788C5D, sky #6A9BCC, plum #827DBD, mineral #629987.
- Accent is used sparingly: the primary button on anthropic.com is black/ink, not clay.
  Clay is for logomark, focus, small highlights. This restraint is a core part of the look.
- Selection colour: not found. [UNVERIFIED]
- Accent hover/danger/success for the app shell: not published beyond 1.1.

### 1.3 Contrast of Claude's own pairs (computed by me, WCAG 2.x)

| Pair | Ratio |
|---|---|
| #141413 on #FAF9F5 | 17.50 |
| #3D3D3A on #FAF9F5 | 10.34 |
| #73726C on #FAF9F5 (muted, light) | 4.58 (passes AA, barely) |
| #FAF9F5 on #30302E (dark primary on card) | 12.55 |
| #C2C0B6 on #30302E | 7.25 |
| #9C9A92 on #30302E (muted, dark card) | 4.69 |
| #9C9A92 on #262624 | 5.38 |
| #EE8884 on #602A28 (danger text dark) | 4.57 |
| #7F2C28 on #F7ECEC | 7.92 |
| #FAF9F5 on #CC785C (clay button label) | 3.11 (fails AA for normal text) |
| white on #D97757 | 3.12 (fails) |
| #D97757 on #FAF9F5 (clay as text) | 2.96 (fails) |

Finding: webdesignhot claims #FAF9F5 on #CC785C is 4.7:1; it is 3.11:1. Do not use a
mid-tone clay as a text-bearing button fill. Relay needs a deeper accent for fills.

## 2. Typography

- Anthropic Sans (UI) and Anthropic Serif (headings/display) are proprietary [OFFICIAL:
  fonts served from assets.claude.ai, brand.anthropic.com/typography]. Older/marketing
  fallbacks cited: StyreneA, Tiempos, Copernicus [3P, conflicting]. Do not use or bundle.
- Usage [3P kombai]: serif for display/section/card headings only (weight ~330 for large
  display, 400 at 36px); sans for body, UI and labels. Anthropic stacks: serif
  `"Anthropic Serif", Georgia, serif`; sans `"Anthropic Sans", system-ui, "Segoe UI", ...`.
- In the app, Claude's replies have used a serif face for message text and sans for chrome
  [UNVERIFIED for current build; userstyles reference `--font-claude-message` /
  `--font-claude-response` variables, which implies message font is separately themable].
- Official app scale for embedded UI [OFFICIAL]: text 12/14/16/20px; headings 12/14/16/20/24/28/36px;
  line heights 1.4 (text) , 1.25 (20-24px), 1.1 (28px), 1.0 (36px); weights 400/500/600/700;
  guidance: three sizes (heading, body, caption) and two weights.
- Marketing scale [3P]: 72/56/36 serif display; body 16/24, 17/25.5, label 15, caption 14.
  Heading tracking about -0.01 to -0.02em at large sizes.

### Free, openly licensed candidates (licences from my knowledge; verify before shipping each file)
| Font | Licence | Role fit |
|---|---|---|
| Newsreader (Production Type) | SIL OFL 1.1 | Closest in warmth to a Tiempos-style text serif; optical sizes; best for headings and optionally message prose |
| Source Serif 4 (Adobe) | SIL OFL 1.1 | Sturdy, readable serif with optical sizes; good fallback, wide weight range |
| Fraunces | SIL OFL 1.1 | "Wonky" soft serif; more personality than Claude; use only for display, can feel off-brand |
| Instrument Serif | SIL OFL 1.1 | Condensed display serif, single weight; headline-only, not body |
| Inter | SIL OFL 1.1 | Neutral UI sans; closest utilitarian match to Styrene-style grotesque; safe |
| Geist | SIL OFL 1.1 | Cleaner, more technical; good UI and mono pair (Geist Mono) |
| Segoe UI Variable (system) | Windows system font | Zero-cost native fallback on the Windows target |

Recommendation for Relay: headings and greeting in Newsreader (display opsz); body/UI in Inter
(or system Segoe UI Variable as fallback); code in Geist Mono or Cascadia Mono/ui-monospace.
Whether assistant message prose is serif or sans is a product choice: serif message body
echoes Claude strongly; sans body is more efficient for logs and tool output. Suggest sans
for chrome and tool output, serif only for greeting, page titles and long-form assistant prose
behind a setting.

### Recommended Relay scale (derived, not observed)
| Token | Size/line | Weight | Tracking | Face |
|---|---|---|---|---|
| display | 36/40 | 400 | -0.015em | serif |
| title | 24/30 | 450-500 | -0.01em | serif |
| heading | 18/26 | 600 | 0 | sans |
| body | 15/24 (message 16/26) | 400 | 0 | sans (or serif option) |
| ui | 14/20 | 450-500 | 0 | sans |
| caption | 12/16 | 400-500 | 0.01em | sans |
| code | 13/20 | 400 | 0 | mono |

## 3. Layout metrics

| Metric | Value | Confidence |
|---|---|---|
| Base spacing unit | 8px; scale 4,8,12,16,24,32,48,64 | [3P] |
| Radii | xs4 sm6 md8 lg10 xl12 full | [OFFICIAL] ; cards up to 16-24 on marketing [3P] |
| Border width | 0.5px regular hairline at 15% ink | [OFFICIAL] |
| Shadows | hairline 0 1 2 /.05; sm 0 1 3 /.1; md 0 4 6 -1 /.1; lg 0 10 15 -3 /.1 | [OFFICIAL] |
| Shadow usage | cards rely on value contrast and borders, shadows mostly on popovers/menus | [3P] |
| Touch target | 44px minimum | [OFFICIAL]; desktop rows can be 32-36 |
| Reading width | transcript column `max-w-3xl` = 48rem = 768px in claude.ai (userstyles override it) | partially verified via userstyle override; exact current value UNVERIFIED |
| Prose cap | 720px (~70ch) | [3P marketing] |
| Sidebar width | about 288px expanded, about 0-56px collapsed (rail) | [UNVERIFIED] |
| Header height | about 48-56px in app; 68px on marketing site | app UNVERIFIED, marketing [3P] |
| Composer | centered in the 768px column; large radius (about 20-24px), min height about 2 lines, controls row beneath text; sits on bg-primary card with hairline border and soft shadow | [UNVERIFIED] |
| Message spacing | user messages in tinted rounded bubbles right-aligned/left; assistant text unboxed full width; ~24px between turns | [UNVERIFIED] |
| Density | airy; generous whitespace, few borders | qualitative |

## 4. Component behaviours

Claude app (UNVERIFIED unless noted; I could not load the app):
- Mode tabs at top-left/sidebar of the desktop app: Chat, Cowork, Code (confirmed by a
  GitHub issue about the Cmd+1/2/3 mode switch, search result only).
- Sidebar order (recollection): new chat, search, Projects, Artifacts/Customize, then Starred,
  Recents list with per-item hover menu; account/plan footer with avatar and settings menu.
- Empty state: centred serif greeting with the clay asterisk mark, personalised ("Good
  evening, name"), composer below, small suggestion chips.
- Composer: "+" attach/tools menu, model picker bottom-right (dropdown with model list,
  extended thinking toggle), send button as filled clay/ink rounded square with up arrow,
  turning into a stop square during generation.
- Artifacts: right-hand side panel opening beside the conversation, with Preview/Code toggle,
  version selector, copy/download; conversation narrows to make room.
- Tool use: collapsed, expandable "steps" rows with icon and one-line summary; thinking is a
  collapsed "Thought process" disclosure. Permissions: inline card/dialog with Allow once /
  Always allow / Deny. Toasts: small bottom/center pills.
- Quick entry (macOS) [OFFICIAL]: double-tap Option (default), Option+Space, or custom; shows
  five recent chats, supports screen-region/app-window capture; Caps Lock dictation. Settings
  > General > Desktop app.
- MCP app design rules [OFFICIAL]: prefer visible controls (segmented buttons, toggle chips)
  over dropdowns; skeletons instead of spinners; at most 2 actions per inline card;
  both themes required; WCAG AA minimum; keyboard navigation.

ChatGPT desktop (usability patterns; mostly from memory plus shortcut lists, UNVERIFIED):
- Sidebar: collapsible, New chat at top, search chats, Library/GPTs/Projects sections,
  Projects list with nested chats, then chat history grouped by recency; account at bottom.
- Top bar: model picker dropdown at top-left of content area, share and overflow at right.
- Companion/launcher window: Alt+Space on Windows opens a compact floating chat [3P-SHORTCUT].
- Settings: modal with left tab list (General, Personalization, Connected apps, Data
  controls, Security, Account), right content pane.
- Slash menu "/" and "@" mentions in composer; composer "+" for files; Canvas side panel.

Efficient-navigation takeaways for Relay: persistent two-level left rail (primary nav then
recent list), global command palette with search, always-visible model/mode picker near the
composer, quick-launch hotkey, settings as a left-nav modal or page.

## 5. Motion

- Easing used on claude.com marketing: cubic-bezier(0, 0, 0.2, 1) for fades and accordions [3P kombai].
- Other 3P suggestions (single extraction, treat UNVERIFIED): 120ms fast, 220ms standard, 360ms
  slow, 480ms page; standard easing (0.4,0,0.2,1); reduced motion = opacity-only.
- App transitions known to exist (UNVERIFIED): sidebar collapse slide, side-panel open,
  disclosure expand, streaming text fade-in, hover background fades.
- Official guidance: inline cards expand to fullscreen with a smooth transition; skeleton
  loaders over spinners [OFFICIAL].

Proposed Relay motion: 100ms (hover/press), 160ms (menus, toggles), 240ms (panel/sidebar),
ease-out cubic-bezier(0.2, 0, 0, 1); honour prefers-reduced-motion (opacity only, 0-80ms).

## 6. Keyboard shortcuts

### Claude [3P-SHORTCUT, Anthropic publishes no official list; verify in-app]
claude.ai web: new conversation Ctrl+K (Mac Cmd+K); Enter submit; Shift+Enter newline; Esc
stop generating; toggle sidebar Ctrl+B; search Ctrl+F in sidebar.
Cowork desktop (third party): Ctrl+Shift+Space open from anywhere; Ctrl+N new task;
Ctrl+Shift+F search history; Ctrl+\ toggle focus mode (hide sidebar); Ctrl+1-9 switch
connectors; Ctrl+P plugin panel. A GitHub issue says Cmd+1/2/3 mode switching was buggy and
fixed ~2026-05-26. [OFFICIAL] only: quick entry double-tap Option / Option+Space (macOS),
Caps Lock dictation.

### ChatGPT [3P-SHORTCUT; Windows list unverified against help.openai.com]
Ctrl+Shift+O new chat; Ctrl+Shift+S toggle sidebar; Shift+Esc focus input; Ctrl+Shift+C
copy last response; Ctrl+Shift+; copy last code block; Ctrl+Shift+Backspace delete chat;
Ctrl+Shift+I custom instructions; Ctrl+/ list shortcuts; Ctrl+K search chats (one source);
Up arrow edit last message; Esc stop; "/" commands; "@" mentions; Alt+Space launcher
(configurable in settings). macOS: Cmd+N new, Cmd+Shift+N temporary chat, Cmd+F search,
Option+Space chat bar, Cmd+, settings, Cmd+O companion window.

Relay suggestion (Windows): Ctrl+K command palette/search, Ctrl+N new chat/task,
Ctrl+B toggle sidebar, Ctrl+, settings, Ctrl+/ shortcut sheet, Esc stop, Ctrl+Shift+Space
global launcher (configurable; Alt+Space conflicts with the Windows system menu in some setups),
Ctrl+1..9 switch workspaces, Up edit last message.

## 7. Accessibility notes

- Claude's own light muted text (#73726C) is 4.58:1 on cream and dark muted (#9C9A92) is
  4.69:1 on its card: both barely clear AA, so muted text must not go lighter.
- Clay (#CC785C / #D97757) fails 3:1 for text and fills with light labels (2.96-3.12:1); use
  as decoration only, or darken for interactive fills.
- Hairline borders at 15% opacity are decorative (around 1.5:1); interactive controls need a
  3:1 boundary (focus ring or stronger border) per WCAG 1.4.11.
- Focus: 2px accent ring with 2px offset; Claude official ring tokens are ink/cream at 70%.
- Target size >= 24px (WCAG 2.2) and 44px on touch; respect prefers-reduced-motion and
  forced-colors (Windows high contrast) by not relying on colour alone for state.

## 8. Proposed Relay tokens

Independent hexes: warmer-yellow neutrals than Claude's, accent is a deeper terracotta
(#B4532F light fill, #E58A67 dark) rather than #D97757/#CC785C/#C6613F. Serif headings,
sans UI. All ratios computed with the WCAG 2.x relative-luminance formula.

```css
:root, [data-theme="light"] {
  --relay-bg:            #FAF7F2; /* canvas */
  --relay-sidebar:       #F1EDE4;
  --relay-surface:       #FFFEFB; /* cards, composer, inputs */
  --relay-surface-sunk:  #EFEAE0; /* code blocks, segmented tracks */
  --relay-hover:         rgba(31, 29, 26, 0.05);
  --relay-active:        rgba(31, 29, 26, 0.09);
  --relay-border:        #E4DED2; /* hairline, decorative */
  --relay-border-strong: #8F887A; /* control boundary, >= 3:1 */
  --relay-text:          #1F1D1A;
  --relay-text-2:        #4A463F;
  --relay-text-muted:    #6B665B;
  --relay-accent:        #B4532F;
  --relay-accent-hover:  #9C4526;
  --relay-on-accent:     #FFFFFF;
  --relay-accent-soft:   #F4E3DA;
  --relay-danger:        #A63A32;
  --relay-success:       #3A6A27;
  --relay-warning:       #7A5A12;
  --relay-selection:     rgba(180, 83, 47, 0.22);
  --relay-focus:         #B4532F;
  --relay-shadow-pop:    0 10px 24px -8px rgba(31,29,26,.18), 0 2px 6px rgba(31,29,26,.06);
}
[data-theme="dark"] {
  --relay-bg:            #1C1A17;
  --relay-sidebar:       #151311;
  --relay-surface:       #262320;
  --relay-surface-sunk:  #121110;
  --relay-hover:         rgba(242, 237, 228, 0.06);
  --relay-active:        rgba(242, 237, 228, 0.10);
  --relay-border:        #34302A;
  --relay-border-strong: #7A7365;
  --relay-text:          #F2EDE4;
  --relay-text-2:        #C9C2B5;
  --relay-text-muted:    #9A9385;
  --relay-accent:        #E58A67;
  --relay-accent-hover:  #EE9C7C;
  --relay-on-accent:     #1C1A17;
  --relay-accent-soft:   #3A2A23;
  --relay-danger:        #F0877D;
  --relay-success:       #86C060;
  --relay-warning:       #D9A93F;
  --relay-selection:     rgba(229, 138, 103, 0.30);
  --relay-focus:         #E58A67;
  --relay-shadow-pop:    0 12px 28px -8px rgba(0,0,0,.55), 0 2px 6px rgba(0,0,0,.3);
}
:root {
  --relay-font-serif: "Newsreader", "Source Serif 4", Georgia, serif;
  --relay-font-sans:  "Inter", "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif;
  --relay-font-mono:  "Geist Mono", "Cascadia Mono", ui-monospace, Consolas, monospace;
  --relay-text-display: 36px/40px; --relay-text-title: 24px/30px;
  --relay-text-heading: 18px/26px; --relay-text-body: 15px/24px;
  --relay-text-ui: 14px/20px;      --relay-text-caption: 12px/16px;
  --relay-space-1: 4px; --relay-space-2: 8px; --relay-space-3: 12px; --relay-space-4: 16px;
  --relay-space-6: 24px; --relay-space-8: 32px; --relay-space-12: 48px;
  --relay-radius-sm: 6px; --relay-radius-md: 8px; --relay-radius-lg: 12px;
  --relay-radius-composer: 20px; --relay-radius-full: 9999px;
  --relay-border-width: 1px;           /* 0.5px on hi-dpi if desired */
  --relay-sidebar-w: 280px; --relay-sidebar-w-collapsed: 56px;
  --relay-content-max: 768px; --relay-header-h: 48px;
  --relay-ctl-h: 32px; --relay-row-h: 34px;
  --relay-dur-fast: 100ms; --relay-dur: 160ms; --relay-dur-panel: 240ms;
  --relay-ease: cubic-bezier(0.2, 0, 0, 1);
}
```

Layout and behaviour to implement (Relay decisions, mirrors efficiency of ChatGPT, tone of Claude):
two-level left rail (New / Search / Projects, then recents with hover menu), collapse to
56px icon rail with Ctrl+B, centred 768px transcript, composer with 20px radius and controls
row (attach, mode/model picker, send/stop), right side panel 360-520px resizable for
artifacts/run details, tool steps as collapsed one-line rows, permission prompts as inline
cards with Allow once / Always / Deny, toasts bottom-centre pills, skeletons over spinners,
settings as a left-nav page.

### Contrast results for proposed tokens (computed)

| Pair | Light | Dark |
|---|---|---|
| text on bg | 15.73 | 14.89 |
| text on sidebar | 14.39 | 15.89 |
| text on surface | 16.67 | 13.40 |
| text-2 on bg | 8.78 | 9.81 |
| text-muted on bg | 5.34 | 5.69 |
| text-muted on sidebar / surface | 4.89 (sidebar) | 5.12 (surface) |
| accent as text on bg | 4.66 | 6.75 |
| on-accent label on accent fill | 4.98 | 6.75 |
| on-accent on accent-hover | 6.38 | 8.00 |
| danger on bg | 6.00 | 7.00 |
| success on bg | 6.01 | 8.04 |
| warning on bg | 5.96 | 8.02 |
| decorative --relay-border vs bg | 1.56 (not a control boundary) | 1.81 (idem) |

All text pairs pass AA (4.5:1); body text passes AAA. `--relay-border-strong` values
(#8F887A, #7A7365) were chosen for >= 3:1 but I did NOT compute them; compute and adjust
before use. Accent-soft backgrounds and selection overlays were not contrast-tested.

## 9. Unverified or open items

1. Live claude.ai shell values (sidebar width, composer metrics, message spacing, selection
   colour, dark shell hexes): inspect the running app via chrome-devtools if Mohamed approves.
2. Whether Claude's assistant message text is serif or sans in the current build.
3. Official ChatGPT Windows shortcut table and settings structure (help.openai.com blocked).
4. Font licences listed from knowledge: confirm each downloaded font file's OFL text.
5. Conflicts between third-party extractions on brand clay hex and motion timings.
