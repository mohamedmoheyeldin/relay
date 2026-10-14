"""Run every local check before a push or merge (D-050). No GitHub Actions.

    uv run python scripts/check.py            # run the checks
    uv run python scripts/check.py --report   # ...and report the result to GitHub

Lint, format, type check and unit tests. Tests marked `desktop` (they open
windows on a real desktop) are skipped unless --desktop is given, and should only
be run when the owner asks.

--report posts a `local-checks` commit status (success or failure) on the
current HEAD commit, which must already be pushed. The `dev` and `main` rulesets
require that status, so "Auto-merge when ready" on a PR merges as soon as a local
run reports success (D-051). It uses the GitHub CLI's login (`gh auth login`).
"""

from __future__ import annotations

import shutil
import subprocess
import sys

STATUS_CONTEXT = "local-checks"

STEPS = [
    ["ruff", "check"],
    ["ruff", "format", "--check"],
    ["pyright"],
    ["pytest", "-q", "-m", "not desktop"],
]


# The Rust core and the Tauri app (relay-core, relay-ui). They run when cargo and pnpm
# are installed. Off Windows the app is checked for the MinGW target, which needs no
# Linux GUI libraries (rustup target add x86_64-pc-windows-gnu).
APP_STEPS = [
    ["cargo", "fmt", "--all", "--check"],
    ["cargo", "clippy", "-p", "relay-core", "--all-targets", "--", "-D", "warnings"],
    ["cargo", "test", "-p", "relay-core"],
    ["pnpm", "--dir", "relay-ui", "exec", "tsc", "--noEmit"],
]
APP_CLIPPY = ["cargo", "clippy", "-p", "relay-ui", "--", "-D", "warnings"]


def app_steps() -> list[list[str]]:
    if not (shutil.which("cargo") and shutil.which("pnpm")):
        print("Skipping the app checks: cargo and pnpm are needed.")
        return []
    steps = [list(s) for s in APP_STEPS]
    clippy = list(APP_CLIPPY)
    if sys.platform != "win32":
        clippy[4:4] = ["--target", "x86_64-pc-windows-gnu"]
    return [*steps, clippy]


def git(*args: str) -> str:
    return subprocess.run(["git", *args], capture_output=True, text=True, check=True).stdout.strip()


def report(passed: bool, summary: str) -> bool:
    """Post the result as a commit status on HEAD; False if it could not be posted."""
    if git("status", "--porcelain", "--untracked-files=no"):
        print("Not reporting: the working tree has uncommitted changes.")
        return False
    sha = git("rev-parse", "HEAD")
    state = "success" if passed else "failure"
    command = [
        "gh",
        "api",
        "--method",
        "POST",
        f"repos/{{owner}}/{{repo}}/statuses/{sha}",
        "-f",
        f"state={state}",
        "-f",
        f"context={STATUS_CONTEXT}",
        "-f",
        f"description={summary[:140]}",
    ]
    try:
        posted = subprocess.run(command, capture_output=True, text=True, check=False)
    except FileNotFoundError:
        print("Not reported: the GitHub CLI (gh) is not installed.")
        return False
    if posted.returncode != 0:
        print(f"Not reported: {posted.stderr.strip() or posted.stdout.strip()}")
        return False
    print(f"Reported {STATUS_CONTEXT}={state} on {sha[:7]}.")
    return True


def main() -> int:
    args = sys.argv[1:]
    steps = [list(s) for s in STEPS]
    if "--desktop" in args:
        steps[-1] = ["pytest", "-q", "-m", "desktop or not desktop"]
    failed = []
    for step in steps:
        print(f"$ uv run {' '.join(step)}", flush=True)
        if subprocess.run(["uv", "run", *step], check=False).returncode != 0:
            failed.append(" ".join(step))
    for step in app_steps():
        print(f"$ {' '.join(step)}", flush=True)
        if subprocess.run(step, check=False).returncode != 0:
            failed.append(" ".join(step))
    summary = "ALL CHECKS PASS" if not failed else f"FAILED: {', '.join(failed)}"
    print(summary)
    if "--report" in args and not report(not failed, summary):
        return 2
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
