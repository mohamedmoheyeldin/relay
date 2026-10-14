import base64
import io
import json

from mcp.shared.memory import create_connected_server_and_client_session
from mcp.types import ImageContent, TextContent
from PIL import Image

from relay_screen import __version__
from relay_screen.server import Services, build_server
from relay_screen.state import ScreenState
from tests.conftest import GradientGrabber


def services(displays):
    return Services(
        state=ScreenState(lambda: displays, lambda ds: None),
        grabber=GradientGrabber(),
    )


async def call(server, name, args=None):
    async with create_connected_server_and_client_session(server._mcp_server) as client:
        return await client.call_tool(name, args or {})


def text(result) -> str:
    content = result.content[0]
    assert isinstance(content, TextContent)
    return content.text


def image_size(result) -> tuple[int, int]:
    content = result.content[0]
    assert isinstance(content, ImageContent)
    assert content.mimeType == "image/png"
    return Image.open(io.BytesIO(base64.b64decode(content.data))).size


async def test_lists_tools():
    server = build_server(services([]))
    async with create_connected_server_and_client_session(server._mcp_server) as client:
        tools = await client.list_tools()
    assert {"ping", "list_displays", "select_display", "screenshot", "zoom"} <= {
        t.name for t in tools.tools
    }


async def test_ping_returns_pong():
    result = await call(build_server(services([])), "ping")
    assert not result.isError
    assert text(result) == f"pong (relay-screen {__version__})"


async def test_list_displays(owner_displays):
    result = await call(build_server(services(owner_displays)), "list_displays")
    data = json.loads(text(result))
    assert [d["name"] for d in data] == ["DISPLAY2", "DISPLAY5", "DISPLAY1"]
    assert data[0]["active"] and data[0]["screenshot_size"] == [1568, 656]
    assert data[1]["screenshot_size"] == [1568, 441]
    assert "handle" not in data[0]


async def test_select_then_screenshot(owner_displays):
    s = services(owner_displays)
    server = build_server(s)
    result = await call(server, "select_display", {"display_id": 2})
    assert "1568x441" in text(result)
    shot = await call(server, "screenshot")
    assert image_size(shot) == (1568, 441)
    assert s.grabber.grabs[-1].left == -2560  # type: ignore[attr-defined]


async def test_select_unknown_display_is_tool_error(owner_displays):
    result = await call(build_server(services(owner_displays)), "select_display", {"display_id": 7})
    assert result.isError
    assert "available: 1, 2, 3" in text(result)


async def test_zoom(owner_displays):
    result = await call(build_server(services(owner_displays)), "zoom", {"region": [0, 0, 99, 99]})
    assert image_size(result) == (656, 656)


async def test_zoom_rejects_bad_region(owner_displays):
    server = build_server(services(owner_displays))
    assert (await call(server, "zoom", {"region": [1, 2, 3]})).isError
    assert (await call(server, "zoom", {"region": [0, 0, 5000, 10]})).isError
