# Relay as an MCP server

Relay can run as a Model Context Protocol server, so an AI client you already use (Claude Code, Codex, Cursor, ...)
can do things on your PC through Relay's checked tools instead of raw shell access.

Same program, no second install: `Relay.exe --mcp` (installed at `C:\Program Files\Relay\Relay.exe`) speaks MCP over
stdio. During development the same server is built as `relay-mcp.exe` (`cargo build --release -p relay-mcp-server`).

## Connect a client
Print the exact command for your install:
```powershell
& "C:\Program Files\Relay\Relay.exe" --mcp --print-config claude
& "C:\Program Files\Relay\Relay.exe" --mcp --print-config codex
& "C:\Program Files\Relay\Relay.exe" --mcp --print-config json   # for any client that takes an mcpServers block
```
Claude Code, one line (user scope):
```powershell
claude mcp add --scope user relay -- "C:\Program Files\Relay\Relay.exe" --mcp
```
Or as a Claude Code plugin (adds the server plus a short usage skill): the `plugin/` folder in this repository, with the
marketplace file in `.claude-plugin/`:
```text
/plugin marketplace add C:\Projects\relay-rust
/plugin install relay@relay
```
Nothing is registered automatically; you run the command.

## Options
| flag | meaning |
|---|---|
| `--tools all` (default) | verified actions **and** screen control (screenshot, click, type, key, windows) |
| `--tools safe` | verified actions only: no screenshots, no pixel clicking |
| `--approvals native` (default) | destructive steps show a system dialog (Yes/No, 2 minutes, no answer = no) |
| `--approvals deny` | every step that needs approval is refused (unattended, read-only use) |
| `--data-dir PATH` | folder holding `settings.json` and logs (default: the Relay app's folder) |
Env: `RELAY_APPROVAL_TIMEOUT_SECS` shortens the dialog timeout.

## What the client gets
- **Verified actions** (`find_files`, `list_dir`, `read_text_file`, `create_dir`, `move_path`, `write_text_file`,
  `move_to_trash`, `open_app`, `open_url`, `focus_window`, `ui_click`, `ui_set_text`, `type_text`, `press_keys`) and
  `run_plan` for several steps with `verify` checks. Each result says *verified* (Relay checked the machine afterwards)
  or *ran, nothing to check against*, or failed and why.
- **Screen control** (with `--tools all`): `screenshot`, `zoom`, `left_click` and friends, `type`, `key`, `scroll`,
  `list_windows`, `list_displays`, `computer_batch`.
- `resume_after_stop`: lifts the emergency stop, only when you tell the AI to continue.
- Tools carry `readOnlyHint` / `destructiveHint` annotations so clients can auto-approve reads and always ask for writes.

## Safety
The same policy as the Relay app, read from its `settings.json` (`hybrid` section): allowed folders (default Desktop,
Documents, Downloads, Pictures, Videos, Music), protected folders, tools switched off, confirmation level.
- Paths outside the allowed folders, and anything inside protected folders (Windows, Program Files, AppData, `.ssh`...), are refused.
- Moving, overwriting and the Recycle Bin always ask you. **Reading a file's contents also asks** (the text is about to
  go to a cloud AI). Listing and finding files do not.
- **Stop:** `Ctrl+Alt+Shift+Esc` (Windows) cancels the running step and blocks further calls until the AI calls
  `resume_after_stop`. If the Relay app is open it holds the same hotkey; the server then logs that it could not register
  it and the app's own hotkey stops the app only. Use the client's own interrupt too.
- Screen control can click anything on screen, including things that bypass the folder rules. If you do not want
  that, run with `--tools safe`.
- Logs and an audit trail are written under the data folder (`logs/`, `runs/`). Nothing is uploaded by Relay itself; the
  client sends tool results to its own AI provider, as with any MCP tool.

## Tested
- 8 automated tests (in-memory JSON-RPC and the real binary over stdio): handshake and version fallback, tool list and
  annotations, folder boundaries, verified create/write/move/trash, decline leaves files untouched, plans, stop switch,
  disabled tools.
- Real Claude Code (2.1.x) used the server: listed a folder, was refused/declined on a trash request and reported it
  accurately with the file still in place; `cursor_position` and `list_displays` worked in `--tools all` mode.
- The native approval dialog appeared on Windows and "Yes" moved a file to the Recycle Bin (verified); an unanswered dialog
  closed itself and refused.
- `Relay.exe --mcp` (GUI-subsystem build) works as a stdio server.

## Not tested / limits
- Codex CLI is not installed on the development PC, so `codex mcp add` is untested; the stdio server uses standard MCP.
- macOS: the server compiles for macOS in CI but has not run on a Mac. The approval dialog uses `osascript`.
- The Claude Code plugin validates (`claude plugin validate`) but has not been installed through the marketplace flow.
- No streamable-HTTP transport yet.

## ChatGPT
ChatGPT cannot start a local stdio server: it connects to **remote HTTPS** MCP servers (Settings, Apps & Connectors,
Developer Mode; availability and write support depend on the plan). To use Relay there it would have to be exposed to the
internet through a tunnel. That would put control of your PC behind a public URL, so Relay does not do this. The supported
path today is Claude Code, Codex, and other clients that run local stdio servers. A local-only HTTP mode with
authentication could be added for clients that accept `http://127.0.0.1` URLs.

Sources: [OpenAI: connect from ChatGPT](https://developers.openai.com/apps-sdk/deploy/connect-chatgpt),
[ChatGPT MCP: how to add a server](https://designrevision.com/blog/add-mcp-server-to-chatgpt). Verify current plan limits
and menu paths on OpenAI's help pages.
