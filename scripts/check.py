"""Run every local check before a push or merge (D-050). No GitHub Actions.

    uv run python scripts/check.py

Lint, format, type check and unit tests. Tests marked `desktop` (they open
windows on a real desktop) are skipped unless --desktop is given, and should only
be run when the owner asks.
"""

from __future__ import annotations

import subprocess
import sys

STEPS = [
    ["ruff", "check"],
    ["ruff", "format", "--check"],
    ["pyright"],
    ["pytest", "-q", "-m", "not desktop"],
]


def main() -> int:
    steps = [list(s) for s in STEPS]
    if "--desktop" in sys.argv[1:]:
        steps[-1] = ["pytest", "-q", "-m", "desktop or not desktop"]
    failed = []
    for step in steps:
        print(f"$ uv run {' '.join(step)}", flush=True)
        if subprocess.run(["uv", "run", *step], check=False).returncode != 0:
            failed.append(" ".join(step))
    print("ALL CHECKS PASS" if not failed else f"FAILED: {', '.join(failed)}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
