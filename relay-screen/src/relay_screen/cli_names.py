"""The names Relay can't choose: the cloud AI's command-line tool is called what the
vendor calls it. They live here and nowhere else, so the rest of Relay only ever says
"Cloud AI" (relay-core keeps the same list in cli_names.rs)."""

from __future__ import annotations

import sys

COMMAND = "claude"
EXE = "claude.exe" if sys.platform == "win32" else COMMAND
WINDOWS_EXE = "claude.exe"  # how the tool shows in a Windows process list
