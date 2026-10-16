"""MCP server entry point.

Claude Code starts this over stdio (see the repo's .mcp.json). Tools are exposed to
Claude as ``mcp__relay-screen__<tool>``. The computer tools mirror the members of
``computer_toolset_20260801`` by name and parameters (D-009); ``batch`` runs several
of them in one call with the toolset's stop-at-first-failure rule.
"""

from __future__ import annotations

import base64
import json
import os
import re
import sys
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any, Literal

import anyio.to_thread
from mcp.server.fastmcp import FastMCP
from mcp.types import CallToolResult, ContentBlock, ImageContent, TextContent

from relay_screen import __version__
from relay_screen.actions import ActionError, ActionResult, Computer
from relay_screen.agent.local import MAX_STEPS, Outcome
from relay_screen.agent.operator import Operator
from relay_screen.apps import AppCatalog
from relay_screen.capture import Grabber
from relay_screen.input.driver import InputDriver
from relay_screen.ocr import OcrService
from relay_screen.safety.approval import (
    RISKY,
    ActRequest,
    AppPrompter,
    ApprovalGate,
    PromptUnavailableError,
    describe,
)
from relay_screen.safety.audit import AuditLog
from relay_screen.safety.killswitch import GuardedBackend, KillSwitch
from relay_screen.state import ScreenState
from relay_screen.uia import UiaService

SERVER_NAME = "relay-screen"
RELAY_TOOL_PREFIX = f"mcp__{SERVER_NAME}__"
# Relay's browser lane (Playwright MCP). Tools that only look, or only open a page, are
# allowed without asking: going to an address changes nothing on its own, and the owner
# usually typed it. Clicking, typing and uploading still ask.
BROWSER_TOOL_PREFIX = "mcp__relay-browser__"
BROWSER_NO_PROMPT = frozenset(
    {
        "browser_navigate",
        "browser_navigate_back",
        "browser_snapshot",
        "browser_take_screenshot",
        "browser_console_messages",
        "browser_network_requests",
        "browser_wait_for",
    }
)


# Browser tools that always ask in "risky" mode: they hand a file or code to the page.
BROWSER_ALWAYS_ASK = frozenset({"browser_file_upload", "browser_evaluate", "browser_run_code"})


def _browser_risky(tool_name: str, params: dict[str, Any]) -> bool:
    """In "risky" mode (D-073), whether a browser-lane call needs the owner: uploads and
    scripts always, clicks and typing when what they target reads as irreversible."""
    if tool_name.removeprefix(BROWSER_TOOL_PREFIX) in BROWSER_ALWAYS_ASK:
        return True
    return RISKY.search(json.dumps(params, ensure_ascii=False)) is not None


# Other tools in "risky" mode: lookups run, and so does everything else unless
# its name says it sends, pays or deletes, or it is a shell command that deletes, wipes,
# shuts down or pushes.
LOOKUP = re.compile(r"^(get|list|search|read|find|fetch|query|describe|show|view)(_|$)")
TOOL_RISKY = re.compile(RISKY.pattern + r"|\b(trash|forward|share|invite|respond)\b", re.I)
SHELL_TOOLS = re.compile(r"(^|__)(Bash|PowerShell|start_process|interact_with_process)$")
SHELL_RISKY = re.compile(
    r"\b(remove-item|rm|rmdir|rd|del|erase|format-volume|clear-disk|diskpart|"
    r"stop-computer|restart-computer|shutdown|remove-itemproperty|clear-content|"
    r"clear-recyclebin|uninstall-package|set-executionpolicy|send-mailmessage)\b|"
    r"\bformat\s+[a-z]:|\breg(\.exe)?\s+delete\b|\bgit\s+(push|reset\s+--hard|clean)\b|"
    r"-method\s+(post|put|delete|patch)\b|\s-X\s*(POST|PUT|DELETE|PATCH)\b",
    re.I,
)


def _tool_risky(tool_name: str, params: dict[str, Any]) -> bool:
    """In "risky" mode, whether a tool call outside Relay's lanes needs the owner."""
    if SHELL_TOOLS.search(tool_name):
        command = params.get("command") or params.get("input") or ""
        return SHELL_RISKY.search(str(command)) is not None
    short = tool_name.rsplit("__", 1)[-1]
    words = re.sub(r"(?<=[a-z])(?=[A-Z])", "_", short).lower()
    if LOOKUP.match(words):
        return False
    return TOOL_RISKY.search(words.replace("_", " ").replace("-", " ")) is not None


def _browser_no_prompt(tool_name: str) -> bool:
    return (
        tool_name.startswith(BROWSER_TOOL_PREFIX)
        and tool_name.removeprefix(BROWSER_TOOL_PREFIX) in BROWSER_NO_PROMPT
    )


ScrollDirection = Literal["up", "down", "left", "right"]


@dataclass
class Services:
    """Platform services the tools use; tests pass fakes."""

    state: ScreenState
    grabber: Grabber
    driver: InputDriver | None = None
    report_changes: bool = True
    sleep: Callable[[float], None] | None = None
    killswitch: KillSwitch | None = None
    gate: ApprovalGate | None = None
    audit: AuditLog | None = None
    uia: UiaService | None = None
    ocr: OcrService | None = None
    apps: AppCatalog | None = None
    operator: Operator | None = None  # the local operator for ui_task; made on first use

    def computer(self) -> Computer:
        hooks: list[Callable[[ActRequest], str | None]] = []
        if self.killswitch is not None:
            switch = self.killswitch

            def check_switch(request: ActRequest) -> None:
                switch.check()

            hooks.append(check_switch)
        if self.gate is not None:
            hooks.append(self.gate.check)
        computer = Computer(
            self.state,
            self.grabber,
            self.driver,
            self.report_changes,
            before_act=hooks,
            audit=self.audit,
            uia=self.uia,
            ocr=self.ocr,
            apps=self.apps,
        )
        if self.sleep is not None:
            computer.sleep = self.sleep
        elif self.killswitch is not None:
            computer.sleep = self.killswitch.wait
        return computer


def default_services() -> Services:
    from relay_screen import displays, win32
    from relay_screen.capture import MssGrabber
    from relay_screen.input.sendinput import SendInputBackend

    switch = KillSwitch()
    switch.start_cursor_watcher(win32.cursor_position)
    switch.start_hotkey_watcher()
    mode = os.environ.get("RELAY_APPROVAL", "ask")
    uia: UiaService | None = None
    try:
        from relay_screen.uia_windows import WinUiaBackend

        uia = UiaService(WinUiaBackend())
    except Exception as exc:  # the rest of Relay still works without UIA
        print(f"relay-screen: UI Automation unavailable: {exc}", file=sys.stderr)
    grabber = MssGrabber()
    ocr: OcrService | None = None
    try:
        from relay_screen.ocr_windows import WinOcrEngine

        ocr = OcrService(WinOcrEngine(), grabber)
    except Exception as exc:
        print(f"relay-screen: OCR unavailable: {exc}", file=sys.stderr)
    apps: AppCatalog | None = None
    try:
        from relay_screen.apps_windows import WinAppBackend

        apps = AppCatalog(WinAppBackend())
    except Exception as exc:
        print(f"relay-screen: app discovery unavailable: {exc}", file=sys.stderr)
    return Services(
        state=ScreenState(displays.enumerate_displays, displays.foreground_display),
        grabber=grabber,
        driver=InputDriver(GuardedBackend(SendInputBackend(), switch)),
        killswitch=switch,
        gate=ApprovalGate(AppPrompter(), mode=mode if mode in ("off", "risky") else "ask"),
        audit=AuditLog.create(),
        uia=uia,
        ocr=ocr,
        apps=apps,
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


def _ui_task_result(
    model_id: str, outcome: Outcome, step_lines: list[str] | None = None
) -> CallToolResult:
    lines = [f"Local operator ({model_id}): {outcome.status}. {outcome.message}"]
    lines += step_lines if step_lines is not None else [step.line() for step in outcome.steps]
    if outcome.status != "done":
        lines.append(
            "Look at the screen (screenshot), then hand ui_task the part that is left as a "
            "narrower goal, saying what you saw."
        )
    text = "\n".join(lines)
    return CallToolResult(
        content=[TextContent(type="text", text=text)], isError=outcome.status != "done"
    )


def build_server(services: Services | None = None) -> FastMCP:
    """Create the server and register its tools. Services are created on first use."""
    mcp = FastMCP(
        SERVER_NAME,
        instructions=(
            "Relay's hands on this Windows PC. ui_task hands a whole task on the screen, in "
            "any program or website, to Relay's local AI, which does it on this PC on its "
            "own; give it one goal at a time in plain words. Coordinates are screenshot "
            "pixels of the active display (see list_displays). Prefer UI Automation and OCR "
            "tools before vision. Use batch to run several computer actions in one call. To "
            "work in a program that is already open, focus_window it first (list_windows "
            "shows what is open); never click taskbar buttons or act on Relay's own window. "
            "When the owner says go to, open, search on or use a site or app, do it on "
            "this screen (open_url, then ui_task or the direct tools) so they see it "
            "happen; a connector that fetches the same data is not a substitute. These "
            "screen tools need no setup, so never stop to ask the owner to set something up."
        ),
    )
    holder: dict[str, Services] = {}
    # Browser tools the owner allowed for the rest of this request (one claude -p run).
    trusted_browser: list[str] = []
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

    # -- UI Automation (exact controls without vision) ------------------------

    @mcp.tool()
    async def uia_tree(
        window: str | None = None,
        depth: int = 6,
        filter: str | None = None,
        root: str | None = None,
        interactive: bool = False,
    ) -> CallToolResult:
        """List the accessibility tree of the foreground window (or the open window whose
        title contains `window`), with bounds in screenshot pixels.

        Each element has an id like e12 for uia_invoke / uia_set_value. `filter` keeps
        only elements whose name, role or automation id contains it. `root` lists from an
        element id instead of the window (for example a web page's Document), and
        `interactive` keeps only controls that can be acted on. Prefer this and uia_find
        over screenshots when an app exposes its controls.
        """
        params = {"window": window, "depth": depth, "filter": filter}
        if root:
            params.update(root=root)
        if interactive:
            params.update(interactive=True)
        return await run("uia_tree", params)

    @mcp.tool()
    async def uia_find(
        query: str, role: str | None = None, window: str | None = None
    ) -> CallToolResult:
        """Find controls by name or automation id (exact matches first), optionally only
        one role such as Button, Edit or MenuItem. Returns ids and centers in screenshot
        pixels."""
        return await run("uia_find", {"query": query, "role": role, "window": window})

    @mcp.tool()
    async def uia_invoke(element: str) -> CallToolResult:
        """Activate a control by id from uia_tree/uia_find: press a button, open a menu,
        toggle a checkbox or select an item, without moving the mouse."""
        return await run("uia_invoke", {"element": element})

    @mcp.tool()
    async def uia_set_value(element: str, value: str) -> CallToolResult:
        """Set the text of an edit field (by id from uia_tree/uia_find) directly."""
        return await run("uia_set_value", {"element": element, "value": value})

    # -- OCR ------------------------------------------------------------------

    @mcp.tool()
    async def find_text(
        text: str | None = None, region: list[int] | None = None, fuzzy: bool = True
    ) -> CallToolResult:
        """Read text on the active display with Windows OCR and say where it is.

        With `text`, returns the best matches for that word or phrase (fuzzy unless
        fuzzy=false) with centers in screenshot pixels, ready to click. Without it,
        lists the lines of text. `region` [x0, y0, x1, y1] limits the search and reads
        small text better. Use it when uia_find can't see a control (custom-drawn UIs).
        """
        return await run("find_text", {"text": text, "region": region, "fuzzy": fuzzy})

    # -- apps (D-030) ---------------------------------------------------------

    @mcp.tool()
    async def list_apps(query: str | None = None, refresh: bool = False) -> CallToolResult:
        """List the apps installed on this PC by name: Start menu apps (Store apps too),
        registered programs and web browsers, with the default browser marked.

        query filters by name (fuzzy). The list is cached for 5 minutes; refresh=true
        rescans, e.g. right after an install. Open one with launch_app.
        """
        return await run("list_apps", {"query": query, "refresh": refresh or None})

    @mcp.tool()
    async def launch_app(name: str, args: list[str] | None = None) -> CallToolResult:
        """Open an installed app by its list_apps name; "browser" opens the web browser.

        args are passed to the app, e.g. ["https://example.com"] for a browser or a file
        path for an editor. Waits for the app's window, brings it to the front and says
        its title and display.
        """
        return await run("launch_app", {"name": name, "args": args})

    @mcp.tool()
    async def open_url(url: str) -> CallToolResult:
        """Open a web page on this screen in the owner's own browser (a new tab when it
        is already running) and bring it to the front. "dice.com" becomes https://dice.com.

        Use it for "go to <site>"; then do the rest on screen with ui_task or the direct
        tools. It works without the browser extension the browser_* tools need.
        """
        return await run("open_url", {"url": url})

    @mcp.tool()
    async def list_windows(query: str | None = None) -> CallToolResult:
        """Open windows, topmost first, with their program, which one is in front and
        which are minimized. query filters by title or program ("brave", "Monster").

        Use this to find where a program already is instead of clicking taskbar buttons
        or running Get-Process. Relay's own window is marked; never act on it.
        """
        return await run("list_windows", {"query": query})

    @mcp.tool()
    async def focus_window(window: str) -> CallToolResult:
        """Bring an open window to the front (restoring it if minimized) by its title,
        a part of it, or the program's name ("Brave", "Notepad", "Monster").

        Do this before screenshotting or clicking in a program that is already open; use
        launch_app only when no window of it is open. Taskbar clicks toggle minimize.
        """
        return await run("focus_window", {"window": window})

    # -- local operator (D-067, D-069) ------------------------------------------

    @mcp.tool()
    async def ui_task(task: str, max_steps: int | None = None) -> CallToolResult:
        """Hand a whole UI task to Relay's local operator, which does it on this PC with
        the local vision model, UI Automation and the apps catalog, then reports back.

        Use it for clicking through an app or a site ("in Notepad, replace every cat with
        bird", "go to dice.com and search for FDE jobs"); keep the thinking (what to do,
        what to write, whether it worked) for yourself. Describe one goal in plain words
        with the app's or site's name. Approvals and the kill switch apply to every
        action. If the local model is not there yet, Relay starts setting it up by itself
        and says so.
        """
        from relay_screen.agent.operator import LocalOperator
        from relay_screen.agent.service import hand_off
        from relay_screen.runtime.paths import RelayPaths
        from relay_screen.runtime.supervisor import RuntimeUnavailable

        steps = max_steps or MAX_STEPS
        try:  # the app's operator service, when Relay is open (D-086)
            handed = await anyio.to_thread.run_sync(
                lambda: hand_off(RelayPaths.default().home, task, steps)
            )
        except RuntimeUnavailable as exc:
            return _error(f"The local operator is unavailable: {exc}")
        if handed is not None:
            model_id, outcome, lines = handed
            return _ui_task_result(model_id, outcome, lines)
        s = svc()
        try:
            if s.operator is None:
                s.operator = LocalOperator()
            operator = s.operator
            computer = s.computer()
            model_id, outcome = await anyio.to_thread.run_sync(
                lambda: operator.run(task, computer, steps)
            )
        except RuntimeUnavailable as exc:
            return _error(f"The local operator is unavailable: {exc}")
        except Exception as exc:
            return _error(f"The local operator failed: {exc.__class__.__name__}: {exc}")
        return _ui_task_result(model_id, outcome)

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

    # -- safety (D-011, D-012, D-019) ----------------------------------------

    @mcp.tool()
    def safety_status() -> str:
        """Whether Relay is armed, how acts are approved, and where the audit log is."""
        s = svc()
        switch, gate = s.killswitch, s.gate
        status = {
            "armed": switch is None or not switch.tripped,
            "stopped_because": switch.reason if switch and switch.tripped else None,
            "approval": gate.mode if gate else "none",
            "audit_log": str(s.audit.path) if s.audit else None,
        }
        return json.dumps(status)

    @mcp.tool()
    async def rearm() -> CallToolResult:
        """After the owner stopped Relay with the kill switch, ask them to re-arm it.

        The owner answers in Relay's own prompt; Claude cannot re-arm Relay itself.
        """
        s = svc()
        switch, gate = s.killswitch, s.gate
        if switch is None or not switch.tripped:
            return CallToolResult(content=[TextContent(type="text", text="Relay is armed.")])
        if gate is None:
            return _error("No approval prompt is available to ask the owner.")
        if s.driver is not None and switch.in_zone(*s.driver.backend.cursor()):
            return _error("The mouse is still in the kill-switch corner; the owner must move it.")
        detail = f"Relay was stopped ({switch.reason}). Let Claude act on your PC again?"
        ok = await anyio.to_thread.run_sync(gate.confirm, "Re-arm Relay?", detail, "Re-arm")
        if not ok:
            return _error("The owner did not re-arm Relay. Stay stopped and ask them in chat.")
        switch.rearm()
        return CallToolResult(content=[TextContent(type="text", text="Relay is armed again.")])

    @mcp.tool()
    async def approve(
        tool_name: str, input: dict[str, Any], tool_use_id: str | None = None
    ) -> CallToolResult:
        """Permission prompt for headless runs (claude -p --permission-prompt-tool).

        Relay's own tools are allowed here because Relay's gate asks the owner itself, and
        so are browser-lane tools that only look at or open a page. In "risky" mode other
        browser tools ask only when they upload, run code or target something that reads
        as irreversible (send, submit, pay, delete ...), and other tools only when their
        name says so or a shell command deletes, shuts down or pushes. Otherwise the
        tool is shown to the owner in Relay's prompt (in the app's chat when it runs);
        for browser tools the prompt also offers to allow the browser for the rest of
        this request.

        Claude Code wants exactly one text block back. A `str` return would also carry
        `structuredContent`, which Claude Code rejects as "an invalid result", so the
        answer goes out as a bare CallToolResult.
        """

        def answer(decision: dict[str, Any]) -> CallToolResult:
            return CallToolResult(content=[TextContent(type="text", text=json.dumps(decision))])

        s = svc()
        allow = {"behavior": "allow", "updatedInput": input}
        if (
            tool_name.startswith(RELAY_TOOL_PREFIX)
            or _browser_no_prompt(tool_name)
            or s.gate is None
            or s.gate.mode == "off"
        ):
            return answer(allow)
        browser = tool_name.startswith(BROWSER_TOOL_PREFIX)
        if browser and trusted_browser:
            return answer(allow)
        if browser and s.gate.mode == "risky" and not _browser_risky(tool_name, input):
            return answer(allow)
        if not browser and s.gate.mode == "risky" and not _tool_risky(tool_name, input):
            return answer(allow)
        detail = describe(tool_name, input)
        title = f"Claude wants to use {tool_name}"
        always = "Allow the browser for this request" if browser else ""
        try:
            decision = await anyio.to_thread.run_sync(s.gate.ask, title, detail, "Allow", always)
        except PromptUnavailableError as exc:
            return answer({"behavior": "deny", "message": str(exc)})
        if decision == "approve_batch" and browser:
            trusted_browser.append(tool_name)
        if decision in ("approve", "approve_batch"):
            return answer(allow)
        if decision == "timeout":
            message = "The owner did not answer Relay's prompt in time. Ask them in chat."
            return answer({"behavior": "deny", "message": message})
        return answer({"behavior": "deny", "message": "The owner denied this tool call."})

    return mcp


def main() -> None:
    import sys

    from relay_screen.runtime import cli

    if len(sys.argv) > 1 and sys.argv[1] in cli.COMMANDS:
        sys.exit(cli.main(sys.argv[1:]))
    if len(sys.argv) > 1 and sys.argv[1] == "agent":
        from relay_screen.agent import cli as agent_cli

        sys.exit(agent_cli.main(sys.argv[2:]))
    if len(sys.argv) > 1 and sys.argv[1] == "operator":
        from relay_screen.agent import service

        sys.exit(service.main(sys.argv[2:]))
    if len(sys.argv) > 1 and sys.argv[1] == "agent-bench":
        from relay_screen.agent import cli as agent_cli

        sys.exit(agent_cli.bench_main(sys.argv[2:]))
    # DPI awareness must be set before anything captures the screen or reads geometry.
    from relay_screen.dpi import set_dpi_awareness

    set_dpi_awareness()
    build_server().run(transport="stdio")


if __name__ == "__main__":
    main()
