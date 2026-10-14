"""MCP server entry point.

Claude Code starts this over stdio (see the repo's .mcp.json). Tools are exposed to
Claude as ``mcp__relay-screen__<tool>``.
"""

from __future__ import annotations

import json
from dataclasses import dataclass

from mcp.server.fastmcp import FastMCP, Image

from relay_screen import __version__, capture
from relay_screen.capture import Grabber
from relay_screen.state import ScreenState

SERVER_NAME = "relay-screen"


@dataclass
class Services:
    """Platform services the tools use; tests pass fakes."""

    state: ScreenState
    grabber: Grabber


def default_services() -> Services:
    from relay_screen import displays
    from relay_screen.capture import MssGrabber

    return Services(
        state=ScreenState(displays.enumerate_displays, displays.foreground_display),
        grabber=MssGrabber(),
    )


def build_server(services: Services | None = None) -> FastMCP:
    """Create the server and register its tools. Services are created on first use."""
    mcp = FastMCP(
        SERVER_NAME,
        instructions=(
            "Relay's hands on this Windows PC. Coordinates are screenshot pixels of the "
            "active display (see list_displays). Prefer UI Automation and OCR tools before "
            "vision."
        ),
    )
    holder: dict[str, Services] = {}
    if services is not None:
        holder["s"] = services

    def svc() -> Services:
        if "s" not in holder:
            holder["s"] = default_services()
        return holder["s"]

    @mcp.tool()
    def ping() -> str:
        """Health check. Returns 'pong' and the relay-screen version."""
        return f"pong (relay-screen {__version__})"

    @mcp.tool()
    def list_displays() -> str:
        """List monitors with their physical layout, and which one is active.

        Screenshots and every coordinate refer to the active display only.
        """
        s = svc()
        active = s.state.active()
        result = []
        for d in s.state.displays():
            geometry = s.state.geometry(d)
            entry = d.describe()
            entry["active"] = d.id == active.id
            entry["screenshot_size"] = [geometry.width, geometry.height]
            result.append(entry)
        return json.dumps(result)

    @mcp.tool()
    def select_display(display_id: int) -> str:
        """Make another monitor active. Take a new screenshot afterwards."""
        s = svc()
        d = s.state.select(display_id)
        geometry = s.state.geometry(d)
        size = f"{geometry.width}x{geometry.height}"
        return f"Active display is now {d.id} ({d.name}), screenshots are {size}."

    @mcp.tool()
    def screenshot() -> Image:
        """Capture the active display. Coordinates for other tools are in this image's pixels."""
        s = svc()
        geometry = s.state.geometry()
        image = capture.screenshot(s.grabber, geometry)
        return Image(data=capture.encode_png(image), format="png")

    @mcp.tool()
    def zoom(region: list[int]) -> Image:
        """Capture region [x0, y0, x1, y1] of the active display at full resolution.

        The region is in screenshot pixels. Coordinates you send afterwards stay in the
        full screenshot's space, not the zoomed image's.
        """
        if len(region) != 4:
            raise ValueError("region must be [x0, y0, x1, y1]")
        s = svc()
        geometry = s.state.geometry()
        x0, y0, x1, y1 = region
        image = capture.zoom(s.grabber, geometry, (x0, y0, x1, y1))
        return Image(data=capture.encode_png(image), format="png")

    return mcp


def main() -> None:
    # DPI awareness must be set before anything captures the screen or reads geometry.
    from relay_screen.dpi import set_dpi_awareness

    set_dpi_awareness()
    build_server().run(transport="stdio")


if __name__ == "__main__":
    main()
