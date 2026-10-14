"""Typing target for scripts/check_input.py: a plain Tk text box that reports what it received.

    python scripts/type_target.py <result.json> <seconds>

Writes {"x", "y", "width", "height"} of the text box to <result.json>.ready once it is shown,
then after <seconds> writes {"text": ...} to <result.json> and exits.
"""

from __future__ import annotations

import ctypes
import json
import sys
import tkinter as tk
from pathlib import Path

ctypes.windll.user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))

result = Path(sys.argv[1])
seconds = float(sys.argv[2])

root = tk.Tk()
root.title("Relay typing target")
root.geometry("700x300+200+200")
root.attributes("-topmost", True)
box = tk.Text(root, font=("Segoe UI", 14))
box.pack(fill="both", expand=True)


def ready() -> None:
    box.focus_force()
    geometry = {
        "x": box.winfo_rootx(),
        "y": box.winfo_rooty(),
        "width": box.winfo_width(),
        "height": box.winfo_height(),
    }
    result.with_suffix(".ready").write_text(json.dumps(geometry), encoding="utf-8")


def finish() -> None:
    result.write_text(json.dumps({"text": box.get("1.0", "end-1c")}), encoding="utf-8")
    root.destroy()


root.after(500, ready)
root.after(int(seconds * 1000), finish)
root.mainloop()
