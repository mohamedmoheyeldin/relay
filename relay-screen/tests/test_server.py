import base64
import io
import json

from mcp.shared.memory import create_connected_server_and_client_session
from mcp.types import ImageContent, TextContent
from PIL import Image

from relay_screen import __version__
from relay_screen.actions import HALT_TEXT, MEMBERS
from relay_screen.agent.local import Event, Hands, Outcome, Step
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
        self.batch_labels = []

    def ask(self, request):
        self.titles.append(request.title)
        self.batch_labels.append(request.batch_label if request.allow_batch else None)
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
    # Claude Code accepts exactly one text block and no structured content.
    assert len(relay.content) == 1 and relay.structuredContent is None
    assert prompter.titles == []  # Relay asks per act itself
    bash = {"tool_name": "Bash", "input": {"command": "dir"}, "tool_use_id": "t1"}
    assert json.loads(text(await call(server, "approve", bash)))["behavior"] == "allow"
    denied = json.loads(text(await call(server, "approve", bash)))
    assert denied == {"behavior": "deny", "message": "The owner denied this tool call."}
    assert prompter.titles == ["Claude wants to use Bash"] * 2
    look = {"tool_name": "mcp__relay-browser__browser_snapshot", "input": {}}
    assert json.loads(text(await call(server, "approve", look)))["behavior"] == "allow"
    go = {"tool_name": "mcp__relay-browser__browser_navigate", "input": {"url": "https://x.y"}}
    assert json.loads(text(await call(server, "approve", go)))["behavior"] == "allow"
    assert prompter.titles == ["Claude wants to use Bash"] * 2  # looking needs no prompt


async def test_approve_can_allow_the_browser_for_the_request(owner_displays):
    s, _, prompter = safe_services(owner_displays, "approve", "approve_batch", "deny")
    server = build_server(s)
    click = {"tool_name": "mcp__relay-browser__browser_click", "input": {"ref": "e1"}}
    for _ in range(4):
        assert json.loads(text(await call(server, "approve", click)))["behavior"] == "allow"
    assert prompter.titles == ["Claude wants to use mcp__relay-browser__browser_click"] * 2
    assert prompter.batch_labels == ["Allow the browser for this request"] * 2
    bash = {"tool_name": "Bash", "input": {"command": "dir"}}
    assert json.loads(text(await call(server, "approve", bash)))["behavior"] == "deny"
    assert prompter.batch_labels[-1] is None  # other tools can't be allowed in bulk


async def test_approve_says_when_the_prompt_is_broken(owner_displays):
    from relay_screen.safety.approval import PromptUnavailableError

    class Broken:
        def ask(self, request):
            raise PromptUnavailableError("ModuleNotFoundError: No module named 'tkinter'")

    s, _, _ = safe_services(owner_displays)
    assert s.gate is not None
    s.gate.prompter = Broken()
    server = build_server(s)
    bash = {"tool_name": "Bash", "input": {"command": "dir"}}
    answer = json.loads(text(await call(server, "approve", bash)))
    assert answer["behavior"] == "deny"
    assert "could not show its approval prompt" in answer["message"]
    assert "tkinter" in answer["message"] and "did not refuse" in answer["message"]


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


class FakeOperator:
    def __init__(self, outcome: Outcome | None = None, error: Exception | None = None):
        self.outcome, self.error = outcome or Outcome("done", ""), error
        self.calls: list[tuple[str, int]] = []

    def run(
        self, task: str, hands: Hands, max_steps: int, on_event: Event | None = None
    ) -> tuple[str, Outcome]:
        self.calls.append((task, max_steps))
        if self.error:
            raise self.error
        return "qwen3-vl-8b-instruct", self.outcome


async def test_ui_task_hands_the_task_to_the_local_operator(owner_displays):
    step = Step(1, "", {"action": "invoke", "element": "e4"}, "OK · target changed")
    s = services(owner_displays)
    operator = FakeOperator(Outcome("done", "Replaced.", [step]))
    s.operator = operator
    result = await call(build_server(s), "ui_task", {"task": "replace cat with bird"})
    assert not result.isError
    assert text(result).startswith("Local operator (qwen3-vl-8b-instruct): done. Replaced.")
    assert 'Step 1: invoke "e4" -> OK · target changed' in text(result)
    assert operator.calls == [("replace cat with bird", 25)]

    s.operator = FakeOperator(Outcome("failed", "sign-in needed"))
    result = await call(build_server(s), "ui_task", {"task": "x", "max_steps": 5})
    assert result.isError and "sign-in needed" in text(result)


async def test_ui_task_says_when_no_local_model_can_run(owner_displays):
    from relay_screen.runtime.supervisor import RuntimeUnavailable

    s = services(owner_displays)
    s.operator = FakeOperator(error=RuntimeUnavailable("No local model is downloaded."))
    result = await call(build_server(s), "ui_task", {"task": "x"})
    assert result.isError and "local operator is unavailable" in text(result)


async def test_risky_mode_lets_harmless_browser_calls_through(owner_displays):
    s, _, prompter = safe_services(owner_displays, "approve")
    assert s.gate is not None
    s.gate.mode = "risky"
    server = build_server(s)

    async def ask(tool: str, params: dict) -> str:
        args = {"tool_name": f"mcp__relay-browser__{tool}", "input": params}
        return json.loads(text(await call(server, "approve", args)))["behavior"]

    assert await ask("browser_click", {"element": "Search box", "ref": "e3"}) == "allow"
    assert await ask("browser_type", {"element": "Search", "text": "turing"}) == "allow"
    assert prompter.titles == []
    assert await ask("browser_click", {"element": "Send button", "ref": "e9"}) == "allow"
    assert prompter.titles == ["Claude wants to use mcp__relay-browser__browser_click"]


async def test_risky_mode_asks_only_before_irreversible_tools(owner_displays):
    s, _, prompter = safe_services(owner_displays, "approve", "approve", "approve")
    assert s.gate is not None
    s.gate.mode = "risky"
    server = build_server(s)

    async def ask(tool: str, params: dict) -> str:
        args = {"tool_name": tool, "input": params}
        return json.loads(text(await call(server, "approve", args)))["behavior"]

    harmless = [
        ("mcp__claude_ai_Dice__search_jobs", {"keyword": "Forward Deployed Engineer"}),
        ("PowerShell", {"command": 'Start-Process "https://www.dice.com/jobs?q=x"'}),
        ("PowerShell", {"command": "Get-Process brave | select Id, MainWindowTitle"}),
        ("Bash", {"command": "ls -la && git status"}),
        ("mcp__claude_ai_Gmail__get_message", {"id": "1"}),
        ("mcp__claude_ai_Gmail__create_draft", {"to": "a@b.c"}),
        ("WebFetch", {"url": "https://x.y"}),
        ("Write", {"file_path": "C:/Users/o/Downloads/jobs.csv"}),
    ]
    for tool, params in harmless:
        assert await ask(tool, params) == "allow", tool
    assert prompter.titles == []
    assert await ask("mcp__claude_ai_Gmail__send_message", {"to": "a@b.c"}) == "allow"
    assert await ask("PowerShell", {"command": "Remove-Item -Recurse C:/x"}) == "allow"
    assert await ask("Bash", {"command": "git push origin main"}) == "allow"
    assert prompter.titles == [
        "Claude wants to use mcp__claude_ai_Gmail__send_message",
        "Claude wants to use PowerShell",
        "Claude wants to use Bash",
    ]
