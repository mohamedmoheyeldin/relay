"""MCP server entry point.

Claude Code starts this over stdio (see the repo's .mcp.json). Tools are exposed to
Claude as ``mcp__relay-screen__<tool>``. The computer tools mirror the members of
``computer_toolset_20260801`` by name and parameters (D-009); ``batch`` runs several
of them in one call with the toolset's stop-at-first-failure rule.
"""

from __future__ import annotations

import base64
import json
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any, Literal

import anyio.to_thread
from mcp.server.fastmcp import FastMCP
from mcp.types import CallToolResult, ContentBlock, ImageContent, TextContent

from relay_screen import __version__
from relay_screen.actions import ActionError, ActionResult, Computer
from relay_screen.capture import Grabber
from relay_screen.input.driver import InputDriver
from relay_screen.state import ScreenState

SERVER_NAME = "relay-screen"

ScrollDirection = Literal["up", "down", "left", "right"]


@dataclass
class Services:
    """Platform services the tools use; tests pass fakes."""

    state: ScreenState
    grabber: Grabber
    driver: InputDriver | None = None
    report_changes: bool = True
    sleep: Callable[[float], None] | None = None

    def computer(self) -> Computer:
        computer = Computer(self.state, self.grabber, self.driver, self.report_changes)
        if self.sleep is not None:
            computer.sleep = self.sleep
        return computer


def default_services() -> Services:
    from relay_screen import displays
    from relay_screen.capture import MssGrabber
    from relay_screen.input.sendinput import SendInputBackend

    return Services(
        state=ScreenState(displays.enumerate_displays, displays.foreground_display),
        grabber=MssGrabber(),
        driver=InputDriver(SendInputBackend()),
    )


def _content(result: ActionResult) -> list[ContentBlock]:
    blocks: list[ContentBlock] = []
    if result.image is not None:
        data = base64.b64encode(result.image).decode("ascii")
        blocks.append(ImageContent(type="image", data=data, mimeType="image/png"))
    if result.text is not None:
        blocks.append(TextContent(type="text", text=result.text))
    return blocks


def _single(result: ActionResult) -> CallToolResult:
    return CallToolResult(content=_content(result), isError=result.is_error)


def _error(message: str) -> CallToolResult:
    return CallToolResult(content=[TextContent(type="text", text=message)], isError=True)


def _batch_result(names: list[str], results: list[ActionResult]) -> CallToolResult:
    content: list[ContentBlock] = []
    for i, (name, result) in enumerate(zip(names, results, strict=True), start=1):
        detail = result.text if result.text is not None else "image follows"
        content.append(TextContent(type="text", text=f"{i}. {name}: {detail}"))
        if result.image is not None:
            content.extend(b for b in _content(result) if isinstance(b, ImageContent))
    return CallToolResult(content=content, isError=any(r.is_error for r in results))


def build_server(services: Services | None = None) -> FastMCP:
    """Create the server and register its tools. Services are created on first use."""
    mcp = FastMCP(
        SERVER_NAME,
        instructions=(
            "Relay's hands on this Windows PC. Coordinates are screenshot pixels of the "
            "active display (see list_displays). Prefer UI Automation and OCR tools before "
            "vision. Use batch to run several computer actions in one call."
        ),
    )
    holder: dict[str, Services] = {}
    if services is not None:
        holder["s"] = services

    def svc() -> Services:
        if "s" not in holder:
            holder["s"] = default_services()
        return holder["s"]

    async def run(name: str, params: dict[str, Any]) -> CallToolResult:
        """Run one member off the event loop; drop parameters Claude left unset."""
        params = {k: v for k, v in params.items() if v is not None}
        try:
            computer = svc().computer()
            result = await anyio.to_thread.run_sync(computer.run, name, params)
        except ActionError as exc:
            return _error(str(exc))
        except Exception as exc:  # e.g. InputBlockedError: report it to Claude
            return _error(f"{exc.__class__.__name__}: {exc}")
        return _single(result)

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

    # -- computer_toolset_20260801 members ------------------------------------

    @mcp.tool()
    async def screenshot() -> CallToolResult:
        """Capture the active display. Coordinates for other tools are in this image's pixels."""
        return await run("screenshot", {})

    @mcp.tool()
    async def zoom(region: list[int]) -> CallToolResult:
        """Capture region [x0, y0, x1, y1] of the active display at full resolution.

        The region is in screenshot pixels. Coordinates you send afterwards stay in the
        full screenshot's space, not the zoomed image's.
        """
        return await run("zoom", {"region": region})

    @mcp.tool()
    async def cursor_position() -> CallToolResult:
        """Report the mouse position in screenshot pixels as 'X=.., Y=..'."""
        return await run("cursor_position", {})

    @mcp.tool()
    async def wait(duration: float) -> CallToolResult:
        """Wait for duration seconds (up to 300) before the next action."""
        return await run("wait", {"duration": duration})

    @mcp.tool()
    async def mouse_move(coordinate: list[int]) -> CallToolResult:
        """Move the mouse to [x, y] in screenshot pixels."""
        return await run("mouse_move", {"coordinate": coordinate})

    @mcp.tool()
    async def left_click(
        coordinate: list[int] | None = None, text: str | None = None
    ) -> CallToolResult:
        """Left click at [x, y] (or where the mouse is). text holds modifiers, e.g. 'ctrl+shift'.

        The reply says whether the area around the click visibly changed.
        """
        return await run("left_click", {"coordinate": coordinate, "text": text})

    @mcp.tool()
    async def right_click(
        coordinate: list[int] | None = None, text: str | None = None
    ) -> CallToolResult:
        """Right click at [x, y] (or where the mouse is). text holds modifiers."""
        return await run("right_click", {"coordinate": coordinate, "text": text})

    @mcp.tool()
    async def middle_click(
        coordinate: list[int] | None = None, text: str | None = None
    ) -> CallToolResult:
        """Middle click at [x, y] (or where the mouse is). text holds modifiers."""
        return await run("middle_click", {"coordinate": coordinate, "text": text})

    @mcp.tool()
    async def double_click(
        coordinate: list[int] | None = None, text: str | None = None
    ) -> CallToolResult:
        """Double click at [x, y] (or where the mouse is). text holds modifiers."""
        return await run("double_click", {"coordinate": coordinate, "text": text})

    @mcp.tool()
    async def triple_click(
        coordinate: list[int] | None = None, text: str | None = None
    ) -> CallToolResult:
        """Triple click at [x, y] (or where the mouse is). text holds modifiers."""
        return await run("triple_click", {"coordinate": coordinate, "text": text})

    @mcp.tool()
    async def left_click_drag(
        start_coordinate: list[int], coordinate: list[int], text: str | None = None
    ) -> CallToolResult:
        """Press the left button at start_coordinate, drag to coordinate and release."""
        return await run(
            "left_click_drag",
            {"start_coordinate": start_coordinate, "coordinate": coordinate, "text": text},
        )

    @mcp.tool()
    async def left_mouse_down() -> CallToolResult:
        """Press and hold the left mouse button where the mouse is."""
        return await run("left_mouse_down", {})

    @mcp.tool()
    async def left_mouse_up() -> CallToolResult:
        """Release the left mouse button where the mouse is."""
        return await run("left_mouse_up", {})

    @mcp.tool()
    async def scroll(
        scroll_direction: ScrollDirection,
        scroll_amount: int,
        coordinate: list[int] | None = None,
        text: str | None = None,
    ) -> CallToolResult:
        """Scroll scroll_amount notches (1-100) at [x, y] (or where the mouse is).

        text holds modifiers held while scrolling, e.g. 'shift' or 'ctrl'.
        """
        return await run(
            "scroll",
            {
                "scroll_direction": scroll_direction,
                "scroll_amount": scroll_amount,
                "coordinate": coordinate,
                "text": text,
            },
        )

    @mcp.tool()
    async def type(text: str) -> CallToolResult:  # toolset member name
        """Type text into the focused control. Newlines press Return, tabs press Tab."""
        return await run("type", {"text": text})

    @mcp.tool()
    async def key(text: str, repeat: int | None = None) -> CallToolResult:
        """Press a key or chord, e.g. 'Return', 'ctrl+s', 'alt+Tab', 'super'. repeat: 1-100."""
        return await run("key", {"text": text, "repeat": repeat})

    @mcp.tool()
    async def hold_key(text: str, duration: float) -> CallToolResult:
        """Hold a key or chord down for duration seconds (up to 300), then release it."""
        return await run("hold_key", {"text": text, "duration": duration})

    @mcp.tool()
    async def batch(actions: list[dict[str, Any]]) -> CallToolResult:
        """Run several computer actions in order in one call.

        Each action is {"name": <tool name above>, "input": {<that tool's parameters>}}.
        Execution stops at the first failure; every later action is answered with
        "Not executed: an earlier computer action in this turn failed."
        """
        computer = svc().computer()
        names = [str(a.get("name", "?")) if isinstance(a, dict) else "?" for a in actions]
        results = await anyio.to_thread.run_sync(computer.run_batch, actions)
        return _batch_result(names, results)

    return mcp


def main() -> None:
    # DPI awareness must be set before anything captures the screen or reads geometry.
    from relay_screen.dpi import set_dpi_awareness

    set_dpi_awareness()
    build_server().run(transport="stdio")


if __name__ == "__main__":
    main()
