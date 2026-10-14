# Relay: notes for Claude Code

Relay is a local-first Windows desktop operator ("local hands, cloud brain").
Read `docs/README.md` first; design lives in `docs/architecture.md` and every
non-obvious choice is in `docs/decisions.md` (append-only, D-0xx).

## Conventions
- The owner's local checkout lives at `C:\Projects\relay` (all their projects are under `C:\Projects`).
- Branches: work on a feature branch, PR into `dev`, then `dev` into `main`. Claude opens PRs ready for review (not drafts) and merges them itself once CI is green, `dev` and `main` alike, closing linked issues (owner request, D-031).
- Small commits with clear messages. Add a `docs/session-log.md` entry at the end of a working session.
- New design decisions get a new D-entry in `docs/decisions.md`; never rewrite old ones.
- Python 3.12 via uv workspace. Run `uv run ruff check`, `uv run ruff format --check`, `uv run pyright`, `uv run pytest` before pushing.
- Windows-only code must import lazily or be guarded so tests and lint run on any OS; tests needing a real desktop are marked `@pytest.mark.desktop`.

## Hard rules
- Never call the Anthropic API or read/store Claude OAuth tokens. Claude is reached only via the `claude` CLI (D-002).
- relay-screen tool names and parameters mirror `computer_toolset_20260801` members; batch semantics and halt text must match exactly (D-009).
- Coordinates exposed to Claude are screenshot pixels of the active display; convert only in `relay_screen/scaling.py`.
- Never commit personal data: playbooks (except examples), profile.yaml, runs/, screenshots, .env.
- RefBox was inspected for design understanding only; do not copy its code or assets.
