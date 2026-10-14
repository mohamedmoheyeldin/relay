import base64
import io
import json

from mcp.shared.memory import create_connected_server_and_client_session
from mcp.types import ImageContent, TextContent
from PIL import Image

from relay_screen import __version__
from relay_screen.actions import HALT_TEXT, MEMBERS
from relay_screen.input.driver import InputDriver
from relay_screen.input.events import KeyEvent, MouseButton, MouseMove
from relay_screen.input.keys import VK_LWIN, VK_RETURN
from relay_screen.server import Services, build_server
from relay_screen.state import ScreenState
from tests.conftest import GradientGrabber, RecordingBackend


def services(displays, backend=None):
    return Services(
        state=ScreenState(lambda: displays, lambda ds: None),
        grabber=GradientGrabber(),
        driver=InputDriver(backend or RecordingBackend()),
        report_changes=False,
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


async def test_lists_every_toolset_member_and_batch():
    server = build_server(services([]))
    async with create_connected_server_and_client_session(server._mcp_server) as client:
        tools = {t.name: t for t in (await client.list_tools()).tools}
    assert set(MEMBERS) | {"batch"} <= set(tools)
    assert set(tools["left_click"].inputSchema["properties"]) == {"coordinate", "text"}
    assert set(tools["scroll"].inputSchema["required"]) == {"scroll_direction", "scroll_amount"}
    assert set(tools["left_click_drag"].inputSchema["required"]) == {
        "start_coordinate",
        "coordinate",
    }


async def test_left_click_tool(owner_displays):
    backend = RecordingBackend()
    s = services(owner_displays, backend)
    result = await call(build_server(s), "left_click", {"coordinate": [784, 328]})
    assert not result.isError and text(result) == "OK"
    assert backend.events[0] == MouseMove(*s.state.geometry().to_physical(784, 328))
    assert backend.events[-1] == MouseButton("left", False)


async def test_member_error_is_tool_error(owner_displays):
    server = build_server(services(owner_displays))
    result = await call(server, "key", {"text": "hyper"})
    assert result.isError and "unknown key name" in text(result)
    result = await call(server, "left_click", {"coordinate": [5000, 0]})
    assert result.isError


async def test_input_blocked_is_tool_error(owner_displays):
    backend = RecordingBackend()

    def blocked(events):
        raise RuntimeError("Windows blocked the input")

    backend.send = blocked  # type: ignore[method-assign]
    result = await call(build_server(services(owner_displays, backend)), "type", {"text": "a"})
    assert result.isError and text(result) == "RuntimeError: Windows blocked the input"


async def test_batch_opens_start_menu_search(owner_displays):
    backend = RecordingBackend()
    server = build_server(services(owner_displays, backend))
    actions = [
        {"name": "key", "input": {"text": "super"}},
        {"name": "wait", "input": {"duration": 0.1}},
        {"name": "key", "input": {"text": "Return"}},
        {"name": "screenshot", "input": {}},
    ]
    result = await call(server, "batch", {"actions": actions})
    assert not result.isError
    lines = [c.text for c in result.content if isinstance(c, TextContent)]
    assert lines == ["1. key: OK", "2. wait: OK", "3. key: OK", "4. screenshot: image follows"]
    assert isinstance(result.content[-1], ImageContent)
    assert backend.events[0] == KeyEvent(VK_LWIN, True, True)
    assert backend.events[-1] == KeyEvent(VK_RETURN, False)


async def test_batch_halts_after_failure(owner_displays):
    backend = RecordingBackend()
    server = build_server(services(owner_displays, backend))
    actions = [
        {"name": "key", "input": {"text": "Return"}},
        {"name": "key", "input": {"text": "hyper"}},
        {"name": "key", "input": {"text": "Return"}},
    ]
    result = await call(server, "batch", {"actions": actions})
    assert result.isError
    lines = [c.text for c in result.content if isinstance(c, TextContent)]
    assert lines[0] == "1. key: OK"
    assert lines[1].startswith("2. key: ") and "hyper" in lines[1]
    assert lines[2] == f"3. key: {HALT_TEXT}"
    assert len(backend.batches) == 1


class Answers:
    def __init__(self, *answers):
        self.answers = list(answers)
        self.titles = []

    def ask(self, request):
        self.titles.append(request.title)
        return self.answers.pop(0) if self.answers else "deny"


def safe_services(displays, *answers):
    from relay_screen.safety.approval import ApprovalGate
    from relay_screen.safety.killswitch import GuardedBackend, KillSwitch

    backend = RecordingBackend()
    backend.position = (500, 500)
    switch = KillSwitch()
    s = services(displays, backend)
    s.driver = InputDriver(GuardedBackend(backend, switch))
    s.killswitch = switch
    prompter = Answers(*answers)
    s.gate = ApprovalGate(prompter)
    return s, backend, prompter


async def test_approve_tool_contract(owner_displays):
    s, _, prompter = safe_services(owner_displays, "approve", "deny")
    server = build_server(s)
    relay = await call(
        server, "approve", {"tool_name": "mcp__relay-screen__left_click", "input": {"x": 1}}
    )
    assert json.loads(text(relay)) == {"behavior": "allow", "updatedInput": {"x": 1}}
    assert prompter.titles == []  # Relay asks per act itself
    bash = {"tool_name": "Bash", "input": {"command": "dir"}, "tool_use_id": "t1"}
    assert json.loads(text(await call(server, "approve", bash)))["behavior"] == "allow"
    denied = json.loads(text(await call(server, "approve", bash)))
    assert denied == {"behavior": "deny", "message": "The owner denied this tool call."}
    assert prompter.titles == ["Claude wants to use Bash"] * 2


async def test_kill_switch_rearm_and_status(owner_displays):
    s, backend, prompter = safe_services(owner_displays, "deny", "approve")
    server = build_server(s)
    assert json.loads(text(await call(server, "safety_status")))["armed"] is True
    assert s.killswitch is not None
    s.killswitch.trip("mouse moved to the top-left corner")
    result = await call(server, "key", {"text": "a"})
    assert result.isError and "kill switch" in text(result)
    status = json.loads(text(await call(server, "safety_status")))
    assert status["armed"] is False and "corner" in status["stopped_because"]
    backend.position = (2, 2)
    assert (await call(server, "rearm")).isError  # mouse still in the corner
    backend.position = (500, 500)
    assert (await call(server, "rearm")).isError  # the owner said no
    assert text(await call(server, "rearm")) == "Relay is armed again."
    assert prompter.titles == ["Re-arm Relay?", "Re-arm Relay?"]
    result = await call(server, "key", {"text": "a"})  # armed again, so the gate asks
    assert result.isError and "denied" in text(result)
