# Claude integration notes (read 2026-10-14)

Sources: https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool ,
https://code.claude.com/docs/en/headless , /cli-reference , /mcp , /permissions , /hooks ,
/legal-and-compliance . Re-check these when Claude Code or the toolset version changes.

## computer_toolset_20260801
- Current computer-use version on the Claude API; Claude 5.5+ models accept only this version. No beta header. Declared as `{"type":"computer_toolset_20260801","configs":{"zoom":{"enabled":true}}}`; rejects display size params.
- 17 members, each a separate tool name (no `action` field), `toolset_name: "computer"`:
  - `screenshot {}`; `zoom {region:[x0,y0,x1,y1]}`
  - `left_click|right_click|middle_click|double_click|triple_click {coordinate?, text?(modifiers)}`
  - `left_click_drag {start_coordinate, coordinate, text?}`; `mouse_move {coordinate}`
  - `left_mouse_down {}`; `left_mouse_up {}`; `cursor_position {}`
  - `scroll {scroll_direction: up|down|left|right, scroll_amount, coordinate?, text?}`
  - `type {text}`; `key {text, repeat?: 1-100}` (`"Return"`, `"ctrl+s"`, `"alt+Tab"`); `hold_key {text, duration<=300}`; `wait {duration<=300}`
  - Modifiers: `shift`, `ctrl`, `alt`, `super` (Windows key), `+`-joined.
- Batch: "Run the blocks sequentially in the order they appear ... and if one fails, don't run the rest." Succeeded -> normal result; failed -> `is_error` + description; later -> `is_error` with exactly `Not executed: an earlier computer action in this turn failed.`
- Human confirmation "before each block runs, because a batch can complete a multistep action within one turn."
- Coordinates are in the pixel space of the full-display screenshots returned; after `zoom` still in full-screenshot space. The client resizes and scales; oversized images are rejected.
- Image limits (Opus 4.7+): <= 2576 px long edge and <= 4784 visual tokens (~3.75 MP). With > 20 images in a request keep sides <= 2000 px. Recommended <= 1920x1080 effective; 1280x800 / 1366x768 for web.
- Zoom: capture region at full res, scale to fit normal screenshot size, aspect preserved; disable zoom rather than erroring if unsupported.
- Only screenshot/zoom return images; others `OK`; `cursor_position` returns coordinates as text.
- Safety guidance: minimal privileges, no sensitive data/logins to the model, domain allowlists, human confirmation for consequential actions; Claude may follow instructions found in screen content.

## Claude Code headless (claude 2.1.296 on the owner's PC)
- `claude -p "<prompt>" --output-format stream-json --verbose` (verbose required for streaming). `--input-format stream-json` for multi-turn over stdin. `--include-partial-messages` for deltas.
- `--mcp-config relay.mcp.json --strict-mcp-config` to load only Relay's servers (plus connectors if configured). `MCP_TIMEOUT` (startup, 30 s default), `MCP_TOOL_TIMEOUT` (per call).
- `--permission-mode default|acceptEdits|plan|auto|dontAsk|bypassPermissions` - always pass explicitly in `-p` runs. `--permission-prompts none` auto-denies anything that would prompt.
- `--permission-prompt-tool mcp__relay-screen__approve`: only sees calls not already decided by hooks/rules/mode. Returns a text block containing JSON `{"behavior":"allow","updatedInput":{...}}` or `{"behavior":"deny","message":"..."}`. Input shape (`tool_name`, `input`, `tool_use_id`) is community-documented, not official: verify in M1-06.
- `--json-schema '<schema>'` -> `structured_output` in the result (use for judgment fields).
- `--resume <session_id>`, `--session-id <uuid>`, `--append-system-prompt[-file]`, `--max-turns`, `--max-budget-usd`, `--model`.
- Never `--bare`: it skips the subscription login and requires an API key.
- Stream events: `system/init` (tools, mcp_servers status), `assistant` (message with tool_use), `user` (tool_result), `system/api_retry`, `permission_denied`, final `result` (`subtype`, `result`, `session_id`, `num_turns`, `is_error`, `permission_denials`, `structured_output?`). Verify exact fields against real output in M2.

## MCP in Claude Code
- `claude mcp add --scope project relay-screen -- uv run --project relay-screen relay-screen` or `.mcp.json` `{"mcpServers":{"relay-screen":{"command":"...","args":[...]}}}`.
- Tool names: `mcp__relay-screen__<tool>`.
- Image results (PNG/JPEG/WebP) are shown inline and may be downscaled; originals saved under `~/.claude/projects/.../tool-results`.
- Output cap 25,000 tokens by default (`MAX_MCP_OUTPUT_TOKENS`), images count; warning above 10k.
- settings.json: allow `"mcp__relay-screen__screenshot"` per tool or `"mcp__relay-screen__*"` for all. PreToolUse hook matcher `"mcp__relay-screen__.*"`.

## Python MCP SDK
- PyPI `mcp` 2.3.0 (2026-10-02) renamed FastMCP -> `MCPServer` (`from mcp.server.mcpserver import MCPServer, Image`). Relay pins `mcp>=1.28,<2` and uses `from mcp.server.fastmcp import FastMCP, Image` for now (D-010).

## Subscription policy (quotes from legal-and-compliance)
- OAuth "is intended exclusively for purchasers of Claude Free, Pro, Max, Team, and Enterprise subscription plans and is designed to support ordinary use of Claude Code and other native Anthropic applications".
- Third parties may not "offer Claude.ai login into their own applications, or ... route requests through Free, Pro, or Max plan credentials on behalf of their users"; developers "may not collect, store, or intermediate Claude.ai credentials or session tokens".
- Does not "prevent an end user from signing in to the unmodified Claude Code binary with their own Claude subscription."
- Usage limits "assume ordinary, individual usage". Relay keeps cloud calls rare (playbooks first) partly for this reason.
