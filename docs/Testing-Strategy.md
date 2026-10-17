# Testing strategy

Last run: 2026-10-16, Windows 11, Node 24.21, Rust stable. **Passed means passed on this PC.** Where a layer could not
run, it says so.

| # | layer | tool | where | result |
|---|---|---|---|---|
| 1 | Unit (Rust) | `cargo test` | inline in every crate | 343 passed, 0 failed, 1 ignored (a test that moves the real mouse) |
| 1 | Unit (UI) | Vitest | `ui/src/**` | 170 passed (18 files): reducers, schemas, mock flows, contrast |
| 2 | Component | Vitest + Testing Library + axe | `ui/src/**` | included in the 170 |
| 3 | Integration | Rust integration tests | `crates/*/tests` | agent 13, hybrid 17, local 11, engine 6, provider 17, host 27, MCP stdio 8 (included in the 343) |
| 4 | Desktop E2E | Playwright over CDP against the real `relay-app.exe` (`pnpm test:desktop`) | `ui/tests/desktop` | 6/6 passed, report `reports/e2e/desktop.json` |
| 4 | Browser E2E | Playwright against the mock backend | `ui/tests/e2e` | 22 passed (reported by the UI agent; not re-run by me) |
| 5 | Native OS | Windows UI Automation / SendInput smoke tests | `relay-os` tests | tolerant smoke tests pass; real input tests are `#[ignore]` |
| 6 | Performance | manual measurement (see Performance.md) | `docs/Performance.md` | baselines recorded, no automated budget gate yet |
| 7 | Security / reliability | tests in `permissions`, `plan`, `hybrid`, MCP stdio; CSP check in desktop E2E | | policy, injection-shaped plans, local-only no-egress, decline, stop switch covered; no fuzzing, no adversarial corpus |
| 8 | Visual regression | Playwright screenshots | `ui/tests/visual` | 22 baselines, pass (mock backend, Chromium) |
| 8 | Accessibility | axe-core | component + browser E2E | no serious/critical violations on Home, Task history, AI & Privacy in both themes |
| 9 | Installer / release | | | **not exercised**: the elevated install has never been run in this work; only unit tests of its logic and a render of the Setup window in real WebView2 |

## Deviations from the brief (and why)
- **Desktop E2E uses CDP, not WebdriverIO + tauri-driver.** It drives the same real WebView2 and runs without installing
  `tauri-driver` and a matching `msedgedriver`. The WebdriverIO route is still the plan for the macOS runner and is not set up.
- **Test hook:** the debug port is WebView2's own switch. A build made without the `e2e` cargo feature removes it from the
  environment at startup, so a shipped build cannot be driven this way.
- **No GitHub Actions desktop job yet.** `.github/workflows/ci.yml` runs Rust (Windows + macOS) and the UI checks; it has
  not been run on GitHub from this branch.
- **macOS:** compile-checked for both Apple targets only. No macOS test of any kind has run.
- **Real Codex:** not installed on this PC, so only the scripted `fake_cli` tests cover the Codex adapter.

## Real-provider and real-OS runs this round
- Claude Code (real): planned a task through the full app (routing, redaction ledger, plan, native execution, honest
  "ran but not verified"). Used again as an MCP client against `relay-mcp`.
- Windows native dialog: appeared, Yes worked, timeout refused.
- Real WebView2: Setup window and app window rendered; screenshots in `reports/screenshots/`.

## Requirements traceability (summary)
| requirement | evidence |
|---|---|
| Hybrid routing: Local only / Hybrid auto / Cloud preferred | `router.rs` tests, `hybrid.rs::local_only_mode_never_reaches_the_cloud`, desktop E2E privacy round trip |
| Cloud returns plan; Rust validates | `plan.rs` tests, `hybrid.rs::unreadable_or_unsafe_plans_are_rejected_and_nothing_runs` |
| Native execution + local verification | `executor.rs` tests, `hybrid.rs::a_step_that_runs_but_cannot_be_verified...`, MCP `changes_are_verified_on_the_machine` |
| Retry / recover / replan | `hybrid.rs::failures_are_repaired...`, `local_failures_escalate...` |
| Confirmation of destructive actions | `permissions.rs` tests, `hybrid.rs` decline/approve, MCP decline test, real dialog run |
| Minimal, redacted context to cloud | `context.rs` tests, `hybrid.rs::heavy_tasks_go_to...`, `sharing_switches...`, live run ("0 categories, 393 B") |
| Provider independence, fallback, per-provider model | `hybrid.rs::a_failing_provider_falls_back...`, `provider.rs` |
| Cancel / kill switch | `hybrid.rs::stopping_cancels...`, MCP `the_stop_switch_blocks...` |
| History persisted | `relay-store` workflow test, `store_adapter` test |
| Prompt-injection defences | closed action set tests; **no adversarial corpus yet** |
| UI shows timeline, provider/mode selectors, verified vs proposed | Vitest + browser E2E + screenshots |
| Installer works | **not verified end to end** |
