# Screen-lane design notes (2026-10-14)

Ideas for how Relay sees and drives the desktop, gathered while planning M1
and M2. They feed D-017 to D-021 and the final architecture proposal.

1. **UI framework.** Tauri 2 + React + Untitled UI stays (D-007). A
   click-through overlay can be a second transparent, click-through,
   always-on-top Tauri window, so no extra binary is needed.
2. **Turn on screen-reader mode before reading UIA.** Many apps (Qt apps,
   Chromium and Electron) keep their UIA/IA2 tree dormant until an assistive
   technology is present. Relay should call
   `SystemParametersInfo(SPI_SETSCREENREADER, TRUE)` while armed (and restore
   it on exit) and use `--force-renderer-accessibility` when it launches a
   Chromium browser. Do not bundle NVDA (GPL-2.0); try the flag first and
   measure on the M1 form demo.
3. **DOM lane via Playwright ARIA snapshots (feeds D-016).** Playwright is
   Apache-2.0 and the Python package exposes `locator.aria_snapshot()`. It is
   the cheapest high-quality "what is on this web page" representation for an
   LLM. Use it for the M2 browser lane, with field-value redaction before
   anything goes to the cloud.
4. **Verify after every action (architecture §4.3).** Make "re-observe and
   compare" part of the action contract: after click/type, re-read the UIA
   element or a crop and return a short success/unclear verdict with the tool
   result.
5. **Playbooks as Agent Skills.** Use the same `SKILL.md` folder format Cloud AI
   Code uses, so the cloud brain loads Relay's playbooks (architecture §7)
   natively.
6. **Office through an API, not pixels.** For Word/Excel/PowerPoint, an add-in
   or COM automation beats screen control. COM via `pywin32` is the lighter
   first step for Relay.
7. **Launch apps by AppUserModelID.** Cache installed apps with their AUMIDs
   and start them through the shell, instead of searching the Start menu by
   pixels.
8. **Models (D-005 stays).** A small local LLM paired with an element
   detector, OCR and a captioner is a known pattern. If UI-Venus grounding
   misses icons, OmniParser v2 is the fallback, but its detector is AGPL-3.0:
   keep it optional and out-of-process. PP-OCRv4 (Apache-2.0) is a faster OCR
   alternative to Windows OCR for small text if needed.
9. **Tray and close UX (M3).** First-close dialog with a Recommended "keep
   running in tray" vs "quit", remembered and changeable in Settings; tray
   icon with idle / busy / needs-approval states; a quick-input hotkey window;
   a small floating plan/progress chip; toasts that stay while hovered; an
   "away briefing" summarising unattended runs; a visible budget/context meter.
10. **Approval UX.** Approvals must be hard to fake (D-011, D-019): prompts in
    Relay's own top-most window that the agent cannot click (Relay ignores
    synthetic input on its own approval window), plus the kill switch.
11. **Proactive suggestions are a later idea, not M1-M3.** Watching chat apps
    for promises the user makes ("I'll send it tomorrow") and offering to do
    them is parked in the roadmap as "post-M3".
