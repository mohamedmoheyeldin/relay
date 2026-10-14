"""MCP server entry point.

Claude Code starts this over stdio (see the repo's .mcp.json). Tools are exposed to
Claude as ``mcp__relay-screen__<tool>``.
"""

from __future__ import annotations

from mcp.server.fastmcp import FastMCP

from relay_screen import __version__

SERVER_NAME = "relay-screen"


def build_server() -> FastMCP:
    """Create the server and register its tools."""
    mcp = FastMCP(
        SERVER_NAME,
        instructions=(
            "Relay's hands on this Windows PC. Coordinates are screenshot pixels of the "
            "active display. Prefer UI Automation and OCR tools before vision."
        ),
    )

    @mcp.tool()
    def ping() -> str:
        """Health check. Returns 'pong' and the relay-screen version."""
        return f"pong (relay-screen {__version__})"

    return mcp


def main() -> None:
    build_server().run(transport="stdio")


if __name__ == "__main__":
    main()
