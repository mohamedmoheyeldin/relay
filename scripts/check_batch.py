"""Manual check for M1-05 on a real Windows desktop (not run in CI).

    uv run python scripts/check_batch.py

Calls the relay-screen `batch` tool through an in-process MCP client, exactly as
Claude Code would: Start menu, type "notepad", Return. Prints every batch line, checks
that a new Notepad process started, runs a batch with a bad key in the middle to show
the halt text, then closes only the Notepad window this script opened.
"""

from __future__ import annotations

import asyncio
import csv
import io
import subprocess

from mcp.shared.memory import create_connected_server_and_client_session
from mcp.types import TextContent

from relay_screen.dpi import set_dpi_awareness
from relay_screen.server import build_server

OPEN_NOTEPAD = [
    {"name": "key", "input": {"text": "super"}},
    {"name": "wait", "input": {"duration": 1.5}},
    {"name": "type", "input": {"text": "notepad"}},
    {"name": "wait", "input": {"duration": 1.5}},
    {"name": "key", "input": {"text": "Return"}},
    {"name": "wait", "input": {"duration": 3}},
    {"name": "screenshot", "input": {}},
]

HALTS = [
    {"name": "wait", "input": {"duration": 0.1}},
    {"name": "key", "input": {"text": "hyper"}},
    {"name": "key", "input": {"text": "Return"}},
]


def notepad_pids() -> set[int]:
    out = subprocess.run(
        ["tasklist", "/fi", "imagename eq notepad.exe", "/fo", "csv", "/nh"],
        capture_output=True,
        text=True,
        check=False,
    ).stdout
    return {int(row[1]) for row in csv.reader(io.StringIO(out)) if len(row) > 1}


async def run_batch(client, actions) -> bool:
    result = await client.call_tool("batch", {"actions": actions})
    for block in result.content:
        if isinstance(block, TextContent):
            print("  ", block.text)
        else:
            print("   [image]")
    return not result.isError


async def main() -> None:
    print("DPI awareness:", set_dpi_awareness())
    server = build_server()
    before = notepad_pids()
    async with create_connected_server_and_client_session(server._mcp_server) as client:
        print("Open Notepad batch:")
        await run_batch(client, OPEN_NOTEPAD)
        new = notepad_pids() - before
        print("NOTEPAD OPENED" if new else "NOTEPAD NOT FOUND", sorted(new))
        print("Batch with a bad key in the middle:")
        await run_batch(client, HALTS)
    for pid in new:
        subprocess.run(["taskkill", "/pid", str(pid)], check=False)


if __name__ == "__main__":
    asyncio.run(main())
