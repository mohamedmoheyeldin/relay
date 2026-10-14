from mcp.shared.memory import create_connected_server_and_client_session
from mcp.types import TextContent

from relay_screen import __version__
from relay_screen.server import build_server


async def test_lists_ping_tool():
    server = build_server()
    async with create_connected_server_and_client_session(server._mcp_server) as client:
        tools = await client.list_tools()
    assert "ping" in {t.name for t in tools.tools}


async def test_ping_returns_pong():
    server = build_server()
    async with create_connected_server_and_client_session(server._mcp_server) as client:
        result = await client.call_tool("ping", {})
    assert not result.isError
    content = result.content[0]
    assert isinstance(content, TextContent)
    assert content.text == f"pong (relay-screen {__version__})"
