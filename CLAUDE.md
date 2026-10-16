# Relay: notes for coding agents

Relay is a local-first Windows desktop operator ("local hands, cloud brain").
Read `docs/README.md` first; design lives in `docs/architecture.md` and every
non-obvious choice is in `docs/decisions.md` (append-only, D-0xx).

## Conventions
- The owner's local checkout lives at `C:\Projects\relay` (all their projects are under `C:\Projects`).
- Branches: commit straight to `dev` for almost everything (owner, 2026-10-16: no separate feature branches), run the local checks, then merge `dev` into `main` with a ready PR that the agent merges itself, closing linked issues (D-031, D-050). GitHub deletes `dev` on that merge, so recreate it from `main` right after.
- No GitHub Actions: tests and checks run locally only. `uv run python scripts/check.py` (ruff, format, pyright, pytest) must pass before every push and merge (D-050). `--report` also posts the `local-checks` commit status that the `dev`/`main` ruleset requires, so "Auto-merge when ready" merges once it reports success (D-051). Checks that open windows on the owner's PC run only when they ask (D-049).
- The app (Tauri + React) uses pnpm, never npm or yarn (D-050).
- Small commits with clear messages. Add a `docs/session-log.md` entry at the end of a working session.
- New design decisions get a new D-entry in `docs/decisions.md`; never rewrite old ones.
- Python 3.12 via uv workspace. `uv run python scripts/check.py` runs `ruff check`, `ruff format --check`, `pyright` and `pytest`.
- Windows-only code must import lazily or be guarded so tests and lint run on any OS; tests needing a real desktop are marked `@pytest.mark.desktop`.

## Hard rules
- Never call the cloud AI's web API or read/store Cloud AI OAuth tokens. Cloud AI is reached only via the Cloud AI CLI (D-002).
- relay-screen tool names and parameters mirror `computer_toolset_20260801` members; batch semantics and halt text must match exactly (D-009).
- Coordinates exposed to Cloud AI are screenshot pixels of the active display; convert only in `relay_screen/scaling.py`.
- Never commit personal data: playbooks (except examples), profile.yaml, runs/, screenshots, .env.
- Never name the cloud AI's vendor or product anywhere: code, docs, UI, commits, PR text, git author (D-111). Say "Cloud AI" and `cloud-cli`; read the real CLI names from the `cli_names` modules. No co-author trailers or footers.
- Never copy code or assets from other products; outside software may be studied for design understanding only.
