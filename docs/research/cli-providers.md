# Driving Claude Code CLI and Codex CLI programmatically

Research date: 2026-10-16. Versions: `claude` 2.1.300 (installed, `--help` and `auth status` run locally); `@openai/codex` 0.163.0 (npm registry `latest`, confirmed via registry JSON; **codex was not run**, no login performed, nothing installed).

Labels used throughout:
- **[V-doc]** quoted or closely paraphrased from an official doc page fetched in this session.
- **[V-local]** observed by running the local `claude` binary.
- **[V-src]** from official source on github.com/openai/codex fetched in this session (summarised by the fetch tool; field lists are reliable, exact serde spelling of rarely used fields is not).
- **[CONSTRUCTED]** an illustrative JSON line I built from the documented type definitions. NOT captured from a real run. Do not treat as ground truth.
- **[UNVERIFIED]** not found in an official source. Test before relying on it.

---

## 0. Licensing and terms position (read first)

### Anthropic (Claude Code) [V-doc: code.claude.com/docs/en/legal-and-compliance, /agent-sdk/overview]
- Official headless route is `claude -p` and the Agent SDK. The docs explicitly say to drive the same loop from other languages by "run the CLI as a subprocess with the `-p` flag and `--output-format json`". So spawning `claude -p` from a Rust app is the supported mechanism.
- Authentication restriction (important for Relay):
  - OAuth (claude.ai login) "is intended exclusively for purchasers of Claude Free, Pro, Max, Team, and Enterprise subscription plans and is designed to support ordinary use of Claude Code and other native Anthropic applications."
  - Developers building products "should use API key authentication through Claude Console or a supported cloud provider. Anthropic does not permit third-party developers to offer Claude.ai login into their own applications, or to route requests through Free, Pro, or Max plan credentials on behalf of their users." Developers "may not collect, store, or intermediate Claude.ai credentials or session tokens".
  - Agent SDK page repeats: "Unless previously approved, Anthropic does not allow third party developers to offer claude.ai login or rate limits for their products, including agents built on the Claude Agent SDK."
  - Carve-out: this "does not prevent an end user from signing in to the unmodified Claude Code binary with their own Claude subscription". Hosting the unmodified binary is allowed if the binary is not modified, no auth method is removed/restricted, and each end user authenticates with their own credentials; the vendor may not pay for/resell usage.
  - "Advertised usage limits for Pro and Max plans assume ordinary, individual usage of Claude Code and the Agent SDK."
- Branding: do not call your product "Claude Code" or mimic it. Allowed: "Claude Agent", "Claude", "{Name} Powered by Claude". (Relay already follows a no-vendor-name rule, D-111.)
- **Practical reading (my interpretation, not legal advice):** Relay spawning the user's own, unmodified, already-logged-in `claude` binary on the user's own PC, for the user's own work, never touching tokens, is the pattern the docs describe as permitted ("end user signs in to the unmodified binary with their own subscription"). What is NOT allowed: Relay offering its own "Sign in with Claude" UI, reading/storing OAuth tokens or keychain entries, pooling or reselling access. Gray area to confirm with Anthropic sales if Relay is distributed commercially: a desktop app that automatically drives the user's subscription in the background at scale. Recommend: Relay only checks `claude auth status`, tells the user to run `claude auth login` themselves, and offers API key (`ANTHROPIC_API_KEY`) as the sanctioned alternative.
- `--bare` mode never reads OAuth/keychain (API key only), so a `--bare` run cannot use the subscription [V-doc/V-local help].

### OpenAI (Codex CLI) [V-doc: learn.chatgpt.com/docs/auth, non-interactive-mode]
- `codex exec` is the documented non-interactive mode. It "reuses saved CLI authentication by default".
- Two auth modes: Sign in with ChatGPT (subscription, included plan credits) and API key (usage-based API pricing). The docs: "We recommend API key authentication for programmatic Codex CLI workflows, such as CI/CD jobs. Don't expose Codex execution in untrusted or public environments." ChatGPT-managed auth in CI is "an advanced option" and warned against for public/open-source repos.
- Treat `auth.json` "like a password"; don't share it.
- I did **not** find an explicit OpenAI Terms-of-Use sentence about driving a personal ChatGPT login from a third-party desktop app. **[UNVERIFIED]** whether that is permitted. The docs only steer automation to API keys / Enterprise "Codex access tokens" (admin-granted, for trusted non-interactive workflows).
- Practical reading: same pattern as Anthropic. Spawn the user's own logged-in `codex` binary; never read or copy `auth.json`; never implement the OAuth flow yourself; let the user run `codex login`.
- Relay hard rule (CLAUDE.md D-002): never read/store Cloud AI OAuth tokens. Both CLIs satisfy this if Relay only runs `auth status` commands and the user logs in themselves.

---

## 1. Claude Code CLI (`claude`)

### 1.1 One-shot run, prompt on stdin
```
claude -p --output-format json --permission-mode dontAsk --allowedTools "Read,Grep" < prompt.txt
echo "Summarize README.md" | claude -p --output-format json
```
- `-p/--print` non-interactive. Prompt can be a positional arg or piped stdin. Piped stdin is capped at 10 MB [V-doc]. Exit code 0 on success, non-zero on failure; failures inside the run (e.g. missing auth) are printed as the result on stdout [V-doc]. Invalid flags go to stderr before the run.
- `-p` skips the workspace trust dialog and runs project hooks and `.mcp.json` servers unless `--bare` is used [V-doc]. For a desktop app that points at arbitrary folders, consider `--bare` (needs API key) or `--setting-sources user` / `--strict-mcp-config`.
- If stdin is unreadable, claude warns on stderr and continues with the CLI prompt [V-doc]. Always close/explicitly provide stdin to avoid a hang (see §4).
- `--output-format json` returns one JSON object (the `result` message, §2.1) at the end.

### 1.2 Streaming JSON (output)
```
claude -p "<prompt>" --output-format stream-json --verbose --include-partial-messages
```
- `stream-json` requires `--verbose` in practice (docs examples always pair them) [V-doc]. `--include-partial-messages` adds token-level `stream_event` lines. Last line is always a `result` message.
- Extra opt-ins: `--include-hook-events`, `--forward-subagent-text` (v2.1.211+), `--replay-user-messages`, `--prompt-suggestions`.
- Slow consumer: claude waits up to 30 s for queued output to drain on exit [V-doc, v2.1.214+]. Read stdout continuously.

### 1.3 Streaming JSON input (multi-turn, interrupts, permission callbacks)
```
claude -p --input-format stream-json --output-format stream-json --verbose [--replay-user-messages]
```
- Persistent process; you write NDJSON user messages to stdin. Shape (from `SDKUserMessage` type) [V-doc]:
  `{"type":"user","message":{"role":"user","content":"Analyze this codebase"},"parent_tool_use_id":null}`
  Content can be an array with `text` and `image` (base64) blocks.
- Also on this channel: `control_request` / `control_response` envelopes with `request_id` (initialize, interrupt, set_permission_mode, can_use_tool). The docs only reference these by name; **their exact JSON wire format is not documented on the pages fetched [UNVERIFIED]**. Types that exist: `SDKControlInitializeResponse`, `SDKControlInterruptResponse {still_queued: string[], cancelled?: string[]}`. Safest route for a Rust app: either mirror what the TS/Python SDK sends (read `@anthropic-ai/claude-agent-sdk` source/types and capture traffic), or avoid control messages and use the simpler options below.
- Python SDK note [V-doc]: a `can_use_tool` callback needs a dummy PreToolUse hook to keep the stream open.

### 1.4 Session resume
```
claude -p "first" --output-format json                       # read .session_id from result
claude -p "next" --resume <session_id> --output-format json
claude -p "next" --continue                                   # most recent in cwd (includes -p sessions when combined with -p)
claude -p "next" --resume <id> --fork-session                 # new id, same history
claude -p "x" --session-id <uuid>                             # pre-assign id (must be a valid UUID)
claude -p "x" --no-session-persistence                        # not saved, cannot be resumed
```
- `--resume` also accepts a session name (`-n`) or the absolute path to a `.jsonl` transcript. Resume finds the session by id in any project on the machine [V-doc].
- Pre-assigning `--session-id <uuid>` lets Relay know the id before the first line arrives (useful for cancel/cleanup). Prompt system text is recorded per conversation and reused on resume unless `--system-prompt-snapshot off` [V-doc].
- Cost `total_cost_usd` on a resumed run reports the conversation's cumulative total [V-doc].

### 1.5 Model and reasoning selection
```
claude -p --model sonnet|opus|haiku|fable|<full-model-name> --effort low|medium|high|xhigh|max|ultracode
--fallback-model sonnet,haiku        # comma list, tried in order on overload
```
- `--effort` levels depend on model [V-doc]. `ultracode` needs v2.1.203+. Init message reports the active `effort` (`low|medium|high|xhigh|max|null`) [V-doc].
- `/model sonnet` and `/effort high` can be sent as the prompt text in `-p` mode [V-doc]. Env: `ANTHROPIC_MODEL`.
- Local `claude --help` (2.1.300) confirms `--model`, `--effort`, `--fallback-model`, `--max-budget-usd`, `--permission-prompts`, `--session-id`, `--fork-session`, `--no-session-persistence`, `--json-schema`, `--include-partial-messages`, `--replay-user-messages`, `--bare`, `--safe-mode`, `--restricted`, `--tools`, `--setting-sources`, `--settings` [V-local]. Note: `--max-turns` is in the online CLI reference but did **not** appear in the local `--help` listing [V-local]; treat as available-but-undocumented-in-help [UNVERIFIED locally].
- Extended thinking budget flags (`--max-thinking-tokens`) were not found in the docs fetched; use `--effort` [UNVERIFIED for others].

### 1.6 Structured output schema
```
claude -p "Extract function names from auth.py" --output-format json \
  --json-schema '{"type":"object","properties":{"functions":{"type":"array","items":{"type":"string"}}},"required":["functions"]}'
```
- Result object contains `structured_output` (validated). Invalid schema => `Error: --json-schema is not a valid JSON Schema` and exit. `format` keyword is accepted as annotation only (not enforced) [V-doc]. Error subtype `error_max_structured_output_retries` exists if the model cannot comply [V-doc types].
- Windows: pass the schema from a file/inline JSON carefully (quote escaping). In Rust use `Command::arg` with the raw string; do not go through `cmd /c`.

### 1.7 Sandbox / permission flags
```
--permission-mode default|manual|acceptEdits|plan|auto|dontAsk|bypassPermissions
--allowedTools "Bash(git diff *)" "Read"       # auto-approve (rule syntax; trailing " *" = prefix match)
--disallowedTools "Edit" "Bash(rm *)"          # bare name removes the tool from context
--tools "Read,Grep" | "" | "default"           # restrict available built-in tools
--permission-prompts host|none                 # none: anything that would prompt is auto-denied (v2.1.259+)
--permission-prompt-tool mcp__server__tool     # MCP tool answers permission prompts
--add-dir <dirs>                               # extra working dirs
--dangerously-skip-permissions                 # = bypassPermissions
--max-budget-usd 5.00   --max-turns 3          # (print mode)
```
- A `-p` run with nothing set takes the built-in starting mode, "which can be `auto`" [V-doc]. Always pass `--permission-mode` explicitly.
- `dontAsk` + `--allowedTools` is the documented locked-down headless pattern; denials appear as `system/permission_denied` events and in result `permission_denials` [V-doc].
- `--permission-prompt-tool`: per secondary search results (zh-TW SDK docs mirror), the MCP tool receives the tool name + input and must return a JSON-stringified payload `{"behavior":"allow","updatedInput":{...}}` or a deny payload (`{"behavior":"deny","message":"..."}`), and the MCP tool must also be in `--allowedTools` as `mcp__<server>__<tool>` [partly V-doc via CLI reference, payload shape from search summary = **[UNVERIFIED]** against the current docs]. Allow without `updatedInput` is accepted since v2.1.207 [V-doc].
- There is no OS-level sandbox flag on `claude` equivalent to codex `--sandbox`; isolation is permission rules + `--restricted` (removes command-running tools, confines file tools) [V-doc/V-local help].

### 1.8 MCP attach for a single run
```
claude -p "..." --strict-mcp-config --mcp-config ./relay-mcp.json
claude -p "..." --strict-mcp-config --mcp-config '{"mcpServers":{"relay":{"type":"http","url":"http://127.0.0.1:PORT/mcp","headers":{"Authorization":"Bearer TOKEN"}}}}'
```
- `--mcp-config` takes files or JSON strings (space separated); `--strict-mcp-config` ignores all other MCP sources [V-doc]. With `-p`, claude waits for pending servers up to `MCP_TIMEOUT` (30 s default) before the first turn (v2.1.221+) [V-doc].
- Entries that fail validation (e.g. `url` without `type`) are silently skipped; check `mcp_server_errors` and `mcp_servers[].status` in `system/init` [V-doc]. **Always assert in code that your server shows `connected`.**
- `mcpServers` JSON schema (stdio `command/args/env`; http/sse `type`,`url`,`headers`): standard Claude Code format [from general knowledge; entries documented in code.claude.com/docs/en/mcp, **page not fetched here, UNVERIFIED in this session**].
- MCP tools are exposed as `mcp__<server>__<tool>`; pre-approve with `--allowedTools "mcp__relay__*"` (glob allowed only after literal `mcp__<server>__` prefix) [V-doc].

### 1.9 Auth status check
```
claude auth status          # JSON; exit 0 if logged in, 1 if not
claude auth status --text
claude auth login | logout | setup-token
```
Real output observed locally [V-local] (identifying fields redacted):
```json
{"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty","analyticsDisabled":false,"projectsDirectory":"C:\\Users\\<user>\\.claude\\projects","configDirectory":"C:\\Users\\<user>\\.claude","email":"<redacted>","orgId":"<redacted>","orgName":"<redacted>","subscriptionType":"max"}
```
`authMethod` is one of `none, claude.ai, oauth_token, api_key, api_key_helper, third_party` [V-doc]. `subscriptionType` is useful for UI ("max"). Do not log email/orgId.
- Env auth: `ANTHROPIC_API_KEY`; `CLAUDE_CODE_OAUTH_TOKEN` from `claude setup-token` is for CI (long-lived token requiring a subscription). Relay must not generate or store this token itself (Section 0). **[`CLAUDE_CODE_OAUTH_TOKEN` name from general knowledge, UNVERIFIED here]**

### 1.10 Cancel behaviour [V-doc]
- SIGINT ends the current turn gracefully (a `result` is recorded). On Windows, signals are not available to a Rust child in the same way, see §4.
- SIGTERM: exit code 143; no result recorded for the in-flight turn; claude terminates the process tree of any running Bash command, runs `SessionEnd` hooks, exits. Interrupted turn is left as-is on resume (set `CLAUDE_CODE_RESUME_INTERRUPTED_TURN=1` to continue it).
- Streaming-input mode: send an `interrupt` control_request (wire format [UNVERIFIED]); response `{still_queued, cancelled}`; or just close stdin, which cancels pending permission prompts [V-doc].
- A `-p` run may stay open waiting for background work (background bash, subagents, Monitor, loop wakeups); 10 min idle cap, `CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS`, bare mode runs no background tasks [V-doc]. Relay should enforce its own wall-clock timeout.

---

## 2. Claude event schemas (`--output-format stream-json`, and `json`)

Source for all type shapes: Agent SDK TypeScript reference, `SDKMessage` union [V-doc]. **No real captured JSONL line is quoted in the docs I fetched; every example line below is [CONSTRUCTED] from the types.** Capture a real transcript with a trivial prompt on your own machine to build golden fixtures.

Top-level `type` values: `system` (many subtypes), `assistant`, `user`, `stream_event`, `result`, `rate_limit_event`, plus `tool_progress`, `auth_status`, and others in the union (`SDKMessage` also lists CompactBoundary, Status, TaskStarted/Progress/Updated/Notification, ThinkingTokens, SessionStateChanged, Notification, ToolUseSummary, MemoryRecall, Elicitation, PromptSuggestion, MirrorError, Informational, ConversationReset). **Parser rule: unknown `type`/`subtype` must be ignored, never fatal.**

### 2.1 Events with fields documented

| type / subtype | Key fields | Notes |
|---|---|---|
| `system`/`init` | uuid, session_id, claude_code_version, cwd, tools[], mcp_servers[{name,status,source?}], mcp_server_errors?, model, permissionMode, slash_commands[], skills[], plugins[], plugin_errors?, apiKeySource, output_style, agents?, betas?, effort?, fast_mode_state?, capabilities?[] | First event normally; hook_started/hook_response or plugin_install can precede it |
| `system`/`api_retry` | attempt, max_retries, retry_delay_ms, error_status (int/null), no_response?{waited_ms,retry_wait_ms}, error (authentication_failed, oauth_org_not_allowed, account_on_hold, billing_error, rate_limit, overloaded, invalid_request, model_not_found, server_error, max_output_tokens, cloud_credential_error, unknown), uuid, session_id | [V-doc table] |
| `system`/`permission_denied` | tool_name, tool_use_id, agent_id?, decision_reason_type?, decision_reason?, message, uuid, session_id | [V-doc type] |
| `system`/`plugin_install` | status (started/installed/failed/completed), name?, error? | only with CLAUDE_CODE_SYNC_PLUGIN_INSTALL |
| `system`/`hook_started|hook_progress|hook_response` | see SDKHook*Message | SessionStart/Setup always emitted |
| `assistant` | uuid, session_id, message (Anthropic `BetaMessage`: content blocks `text`, `thinking`, `tool_use`), parent_tool_use_id (null for main), error?, aborted?, agent_id?, timestamp?, usage_report? | One per completed assistant message |
| `user` | message (MessageParam; `tool_result` blocks), parent_tool_use_id, tool_use_result?, isSynthetic? | Tool results come back as `user` messages |
| `stream_event` | event (Anthropic raw stream event: message_start, content_block_start/delta/stop, message_delta, message_stop), parent_tool_use_id, uuid, session_id, ttft_ms? | Only with `--include-partial-messages`. Text deltas: `event.delta.type == "text_delta"`, `event.delta.text` [V-doc jq example] |
| `rate_limit_event` | rate_limit_info{status: allowed/allowed_warning/rejected, resetsAt?, utilization?, errorCode?: "credits_required", canUserPurchaseCredits?, hasChargeableSavedPaymentMethod?}, uuid, session_id | v2.1.181+. **This is the usage/limit signal for subscription auth** |
| `result`/`success` | uuid, session_id, duration_ms, duration_api_ms, is_error, api_error_status?, num_turns, result (string), stop_reason, total_cost_usd, usage, modelUsage{model:...}, permission_denials[], structured_output?, deferred_tool_use?, terminal_reason?, fast_mode_state? | Last line of stream |
| `result`/`error_max_turns` `error_during_execution` `error_max_budget_usd` `error_max_structured_output_retries` | same common fields + errors[] , startup_failure_reason? | `is_error` true |

### 2.2 Constructed example lines (NOT real captures)
```json
{"type":"system","subtype":"init","session_id":"550e8400-e29b-41d4-a716-446655440000","cwd":"C:\\Projects\\x","model":"<model>","permissionMode":"dontAsk","tools":["Read","Grep"],"mcp_servers":[{"name":"relay","status":"connected"}],"apiKeySource":"none","claude_code_version":"2.1.300","uuid":"...","output_style":"default","slash_commands":[],"skills":[],"plugins":[]}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hel"}},"parent_tool_use_id":null,"uuid":"...","session_id":"..."}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_01","name":"Read","input":{"file_path":"README.md"}}]},"parent_tool_use_id":null,"uuid":"...","session_id":"..."}
{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_01","content":"..."}]},"parent_tool_use_id":null,"uuid":"...","session_id":"..."}
{"type":"system","subtype":"api_retry","attempt":1,"max_retries":10,"retry_delay_ms":2000,"error_status":529,"error":"overloaded","uuid":"...","session_id":"..."}
{"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","resetsAt":1760000000,"utilization":0.82},"uuid":"...","session_id":"..."}
{"type":"result","subtype":"success","is_error":false,"duration_ms":4200,"duration_api_ms":3900,"num_turns":2,"result":"final text","stop_reason":"end_turn","total_cost_usd":0.0123,"usage":{"input_tokens":0,"output_tokens":0},"modelUsage":{},"permission_denials":[],"session_id":"...","uuid":"..."}
```
Units unverified: `resetsAt` (epoch seconds vs ms) and `utilization` (0-1 vs percent) are not specified in the docs fetched **[UNVERIFIED]**.

### 2.3 `--output-format json`
Single object = the `result` message (above) incl. `session_id`, `result`, `structured_output`, `total_cost_usd`, `usage`, `modelUsage`. Cost is a client-side estimate, not billing [V-doc]. For subscription users the dollar figure is notional.

### 2.4 Token usage
`usage` (Anthropic usage object: input/output/cache tokens) and per-model `modelUsage` in `result`; assistant `message.usage` per message; `context_usage` / `usage_report` optional on assistant messages. Exact inner field names follow Anthropic API usage **[inner shape UNVERIFIED here]**.

---

## 3. Codex CLI (`codex`)

### 3.1 One-shot run, prompt on stdin
```
codex exec --json --sandbox read-only --skip-git-repo-check -C <workdir> - < prompt.txt
cat prompt.txt | codex exec -  --json
```
- Prompt rules [V-doc/V-src]: positional `PROMPT`; if omitted or `-`, read entire prompt from stdin; if both a prompt arg and piped stdin are given, stdin is appended as a `<stdin>` block. Use `-` explicitly.
- `codex exec` (alias `codex e`) never prompts interactively. Default sandbox for exec is `read-only` [V-doc]. Requires a git repo unless `--skip-git-repo-check`.
- Progress/logging goes to stderr; JSONL events go to stdout with `--json` [V-doc: "stdout becomes a JSON Lines stream"]. Exact stderr content **[UNVERIFIED]**.
- `-o/--output-last-message <file>` also writes the final message to a file (a robust fallback to parsing the stream) [V-doc].
- `--ephemeral` = no rollout files persisted (then no resume). `--ignore-user-config` skips `$CODEX_HOME/config.toml` (auth still from CODEX_HOME). `--ignore-rules` skips execpolicy `.rules`. `--color never`. `--strict-config` errors on unknown config fields [V-src].

### 3.2 Streaming JSON
`--json` (alias `--experimental-json`) prints one JSON object per line. Event stream is item-level, **not token-level** (see 4.3). No `--include-partial` equivalent exists in `exec`.

### 3.3 Session resume
```
codex exec resume --last "follow-up"                  # most recent session for cwd (add --all for any cwd)
codex exec resume <SESSION_ID|name> "follow-up" --json
echo "follow-up" | codex exec resume <SESSION_ID> -  --json
codex exec fork <SESSION_ID> "..."                    # [V-src] fork subcommand
```
- Session id = `thread_id` from the `thread.started` event [V-doc/V-src]. Resume flags accepted: `-i/--image`, prompt (`-` = stdin). Whether `--json`, `-m`, `--sandbox`, `-c` flags are accepted after `resume` follows the global flags design (they are `global = true` in clap per V-src for several), but **the combination `codex exec resume <id> --json` should be tested [UNVERIFIED]**.
- Sessions stored under `$CODEX_HOME/sessions` (rollout files) unless `--ephemeral` [path layout UNVERIFIED].

### 3.4 Model and reasoning selection
```
codex exec -m <model> -c model_reasoning_effort="high" -c model_reasoning_summary="auto" ...
```
- `-m/--model` [V-doc]. `-c key=value` parses value as TOML, else literal string; repeatable; dotted keys for nested tables (`-c sandbox_workspace_write.network_access=true`) [V-doc for TOML parsing; dotted-path syntax is standard but **UNVERIFIED in the page**].
- `model_reasoning_effort` values: documented as "examples include low, medium, high, xhigh, max, ultra. Available levels depend on the model and client" [V-doc config reference]. Older codex used minimal|low|medium|high. Do not hardcode; discover per model **[how to enumerate models non-interactively: UNVERIFIED; the app-server has a model list method, not fetched]**.
- `model_reasoning_summary`: auto|concise|detailed|none. `model_verbosity`: low|medium|high. `service_tier` e.g. fast [V-doc].
- `-p/--profile name` layers `$CODEX_HOME/<name>.config.toml` [V-doc]. `--oss --local-provider ollama|lmstudio` for local models.

### 3.5 Structured output
```
codex exec "Extract project metadata" --output-schema ./schema.json -o ./out.json --json
```
- `--output-schema <file>`: JSON Schema file for the final response; docs say Codex validates output against it. Example schema in docs: object with `project_name` (string) and `programming_languages` (array of string), both required, `additionalProperties:false` [V-doc]. (OpenAI structured outputs typically require `additionalProperties:false` and all properties required; **strictness rules for the schema UNVERIFIED**.)
- Final JSON text arrives as the `agent_message` item text (and in `-o` file). Relay must JSON-parse that text itself.

### 3.6 Sandbox / approval flags
```
-s, --sandbox read-only|workspace-write|danger-full-access
-a, --ask-for-approval on-request|never          # global flag per CLI reference; config key approval_policy
--approve-for-me / --not-so-yolo                  # exec: automatic review + on-request + workspace-write; conflicts with --sandbox, --yolo
--dangerously-bypass-approvals-and-sandbox / --yolo
-C, --cd <path>      --add-dir <path> (extra writable dirs, repeatable)
--full-auto           # REMOVED ("no longer supported. Use --sandbox workspace-write")
-c sandbox_workspace_write.network_access=true
```
- Approvals values now `on-request | never | {granular=...}`; `untrusted` retired; `on-failure` deprecated [V-doc]. In `exec` there is no human to approve, so use `--sandbox workspace-write` (docs) and/or `-a never`. Docs: CI pattern `--sandbox read-only --ask-for-approval never`. **Whether `exec` accepts `-a` directly is not shown in the exec flag table [UNVERIFIED]; use `-c approval_policy="never"` as the portable form.** Behaviour when a command needs approval in exec: docs do not say (likely denied/declined, `command_execution.status = "declined"` exists in the schema) [UNVERIFIED].
- Network is off by default inside sandboxes; web search defaults to `cached`. `.git`, `.agents`, `.codex`, `.aws` stay read-only inside workspace-write [V-doc].
- Windows: native sandbox, modes `mxc` (Windows 11 24H2 build 26100.9278+ / 25H2), `elevated` (needs admin-approved setup), `unelevated` (weaker, legacy). Set `[windows] sandbox = "elevated"|"unelevated"` and `[features] prefer_mxc = true` in config.toml. "With MXC, remaining child processes stop when the foreground command exits." Windows 10 best effort. Logs: `CODEX_HOME/.sandbox/sandbox.log`. The Windows page does not cover `exec` specifically [V-doc]. Without setup the first run may require interactive/admin setup **[UNVERIFIED for headless exec]**.

### 3.7 MCP attach for a single run
Persistent route: `codex mcp add <name> --url https://... --bearer-token-env-var ENV` or `codex mcp add <name> --env K=V -- <command...>`; `codex mcp list --json`, `get <name> --json`, `remove`, `login/logout` [V-doc]. That edits the user's config (not single-run, avoid in Relay).

Single-run via `-c` overrides (preferred; nothing written to disk) [syntax derived from documented config keys + `-c` TOML parsing; **the exact quoting is UNVERIFIED, test on Windows**]:
```
codex exec --json \
  -c 'mcp_servers.relay.url="http://127.0.0.1:PORT/mcp"' \
  -c 'mcp_servers.relay.bearer_token_env_var="RELAY_MCP_TOKEN"' \
  -c 'mcp_servers.relay.required=true' \
  -c 'mcp_servers.relay.startup_timeout_sec=20' \
  -c 'mcp_servers.relay.default_tools_approval_mode="approve"' -
```
and set `RELAY_MCP_TOKEN=<secret>` in the child's environment only (never on the command line). Stdio form: `-c 'mcp_servers.relay.command="C:\\path\\relay-mcp.exe"' -c 'mcp_servers.relay.args=["--stdio"]'`.
- Config keys [V-doc]: stdio `command,args,env,env_vars,cwd`; HTTP `url,bearer_token_env_var,http_headers,env_http_headers,http_headers_helper,auth(oauth|chatgpt)`; shared `startup_timeout_sec (default 10), tool_timeout_sec (60), enabled, required (fail startup if server cannot init), enabled_tools, disabled_tools, default_tools_approval_mode (auto|prompt|writes|approve), tools.<tool>.approval_mode`.
- `required=true` is how you make exec fail fast if the server is down. Alternative isolation: `--ignore-user-config` so the user's own MCP servers/config do not load (auth still works).
- MCP tool calls appear as `mcp_tool_call` items (§4).

### 3.8 Auth status
```
codex login status      # prints active auth mode; exit 0 when logged in [V-doc]
codex login --device-auth | --with-api-key (key on stdin) | --with-access-token (stdin)
codex logout
```
- Output text format of `login status` is **not documented [UNVERIFIED]**; rely on exit code, and treat stdout as display text only.
- Credentials: `~/.codex/auth.json` or OS credential store; `cli_auth_credentials_store = file|keyring|auto|ephemeral`; `forced_login_method = chatgpt|api` [V-doc]. `CODEX_HOME` relocates config and auth (default `~/.codex`) [V-doc/V-src]. `CODEX_API_KEY` env var overrides auth for one `codex exec` run (also `codex review`, TS SDK) [V-doc]. `OPENAI_API_KEY` is also read by the CLI; docs warn to keep either out of jobs running untrusted repo code.
- Relay should NOT read `auth.json`. If you want a per-app isolated login, set `CODEX_HOME` to an app-owned dir and have the user run `codex login` there themselves (the docs describe `CODEX_HOME` as the credentials root; isolation behaviour with keyring store on Windows **UNVERIFIED**).

### 3.9 Usage / rate-limit info
- `exec --json` emits token usage on `turn.completed` only. Plan rate-limit/credit info is **not** in the `exec` event schema [V-src]. Docs do not describe rate limits per auth mode [V-doc].
- `codex app-server` (JSON-RPC over stdio JSONL) exposes `account/read`, `account/rateLimits/read`, and an `account/rateLimits/updated` notification (from a PR + mirror search, not from official docs page text I could read) **[request/response shapes UNVERIFIED; generate with `codex app-server generate-json-schema --out DIR`]**.

### 3.10 Cancel behaviour
- Not documented for `exec` **[UNVERIFIED]**. Expect: Ctrl+C (SIGINT / CTRL_C_EVENT) interrupts the turn and exits; hard kill leaves a partial rollout. Under the Windows sandbox, commands run in child processes; killing only `codex.exe` may leave grandchildren (§5).
- For clean interrupts the supported mechanism is `app-server`'s `turn/interrupt` [V-doc], after which the turn ends with `status:"interrupted"`.

### 3.11 Alternative, richer interface: `codex app-server`
[V-doc learn.chatgpt.com/docs/app-server] JSON-RPC 2.0 (without the `"jsonrpc"` field) over stdio JSONL (`--listen stdio://` default). Handshake: send `{"method":"initialize","id":0,"params":{"clientInfo":{"name":"...","title":"...","version":"..."}}}` then `{"method":"initialized","params":{}}`. Then `thread/start {model}` -> `{thread:{id}}`; `turn/start {threadId,input:[{type:"text",text}]}`; `turn/interrupt {threadId,turnId}`. Notifications: `thread/started`, `turn/started`, `turn/completed`, `item/started`, `item/completed`, `item/agentMessage/delta` (**real token deltas**), `command/exec/outputDelta`, `thread/status/changed`, etc. Server requests for approvals: `item/permissions/requestApproval`, `mcpServer/elicitation/request`, `tool/requestUserInput`. Schemas are version-specific: `codex app-server generate-json-schema --out DIR` / `generate-ts`.
Caveat from the same page: "use app-server ... for a deep integration inside your own product: authentication, conversation history, approvals, and streamed agent events"; for automation/CI prefer the Codex SDK; the **WebSocket transport is experimental/unsupported**; `process/*` methods are experimental and run outside the sandbox. The page's wording on stdio production support: only the command/WebSocket are called experimental; protocol schema "unstable" per README mirror [partly UNVERIFIED]. Given deltas, interrupt, approvals and rate limits, this is the better long-term adapter for a desktop app, but it is a larger surface than `exec --json`. The TypeScript SDK (`@openai/codex-sdk`) wraps `codex exec`'s JSONL [from general knowledge; **UNVERIFIED here**].

---

## 4. Codex `exec --json` event schema

Source: official docs page (4 real example lines quoted) + `codex-rs/exec/src/exec_events.rs` type summary [V-src]. All lines are `{"type": "...", ...}`.

### 4.1 Thread events (serde tag = `type`)
| type | payload |
|---|---|
| `thread.started` | `thread_id` (string; use for resume) |
| `turn.started` | (empty) |
| `turn.completed` | `usage`{input_tokens, cached_input_tokens, output_tokens, reasoning_output_tokens, optional cache_write_input_tokens (defaults 0)} all integers |
| `turn.failed` | `error`{message} |
| `item.started` / `item.updated` / `item.completed` | `item` |
| `error` | `message` (shape of top-level `error` event: doc lists it but gives no example; V-src `ThreadErrorEvent{message}`) |

### 4.2 Item (`item` = `{id, type, ...fields}`; `type` snake_case)
| item type | fields | statuses |
|---|---|---|
| `agent_message` | text | |
| `reasoning` | text (summary text, only if reasoning summaries enabled **[UNVERIFIED]**) | |
| `command_execution` | command, aggregated_output, exit_code (int or null), status | in_progress, completed, failed, declined |
| `file_change` | changes[{path, kind: add/delete/update}], status | in_progress, completed, failed |
| `mcp_tool_call` | server, tool, arguments(JSON), result?{content[], structured_content?, _meta?}, error?{message}, status | in_progress, completed, failed |
| `collab_tool_call` | tool (spawn_agent/send_input/wait/close_agent), sender_thread_id, receiver_thread_ids, prompt?, agents_states{id:{status,message?}}, status | |
| `web_search` | id, query, action, results? | |
| `todo_list` | items[{text, completed}] | |
| `error` | message | |

### 4.3 Real example lines (quoted from the official non-interactive docs page) [V-doc]
```json
{"type":"thread.started","thread_id":"0199a213-81c0-7800-8aa1-bbab2a035a53"}
{"type":"turn.started"}
{"type":"item.started","item":{"id":"item_1","type":"command_execution","command":"bash -lc ls","status":"in_progress"}}
{"type":"turn.completed","usage":{"input_tokens":24763,"cached_input_tokens":24448,"output_tokens":122,"reasoning_output_tokens":0}}
```
(The page also shows an `item.completed` with an `agent_message` whose `text` holds the message; I did not capture its exact line.)

### 4.4 Constructed lines for the rest [CONSTRUCTED from types, not real captures]
```json
{"type":"item.completed","item":{"id":"item_1","type":"command_execution","command":"bash -lc ls","aggregated_output":"README.md\n","exit_code":0,"status":"completed"}}
{"type":"item.completed","item":{"id":"item_2","type":"agent_message","text":"Done."}}
{"type":"item.completed","item":{"id":"item_3","type":"file_change","changes":[{"path":"src/a.rs","kind":"update"}],"status":"completed"}}
{"type":"item.started","item":{"id":"item_4","type":"mcp_tool_call","server":"relay","tool":"screenshot","arguments":{},"status":"in_progress"}}
{"type":"item.completed","item":{"id":"item_4","type":"mcp_tool_call","server":"relay","tool":"screenshot","arguments":{},"result":{"content":[{"type":"text","text":"..."}]},"status":"completed"}}
{"type":"item.updated","item":{"id":"item_5","type":"todo_list","items":[{"text":"step","completed":false}]}}
{"type":"turn.failed","error":{"message":"..."}}
{"type":"error","message":"..."}
```
Notes:
- **No message deltas / token streaming in `exec --json`.** `agent_message` arrives complete in `item.completed`. `command_execution` output is delivered in aggregate at completion (the `item.started` has no output). `item.updated` is only evidenced for `todo_list` (others UNVERIFIED).
- Errors: `error` (stream/transport/config problems, may be retryable reconnect notices **[UNVERIFIED]**), `turn.failed` (turn ended in error), plus `error` item. Process exit code non-zero on failure (exact codes UNVERIFIED).
- Usage on `turn.completed`; there is no cost field, no model name, and no sandbox/approval echo event.
- Field presence varies by version; schema pinned by the binary. Parse leniently (`#[serde(other)]`, `Option` for everything but `type`).

---

## 5. Capability comparison

| Capability | Claude Code `claude -p` | Codex `codex exec` |
|---|---|---|
| Official headless mode | Yes (`-p`, Agent SDK) | Yes (`exec`; app-server for rich integration) |
| Prompt via stdin | Yes (pipe, 10 MB cap) | Yes (`-` sentinel, or stdin w/o prompt) |
| Streaming events | `stream-json` (needs `--verbose`) | `--json` JSONL |
| Token/text deltas | Yes with `--include-partial-messages` | No in exec; yes in app-server |
| Tool-call events | `assistant` tool_use + `user` tool_result | `item.*` command_execution, file_change, mcp_tool_call, web_search |
| Multi-turn in one process | Yes (`--input-format stream-json`) | No (resume = new process); yes via app-server |
| Mid-run interrupt | control_request interrupt (format UNVERIFIED), SIGINT | SIGINT (UNVERIFIED), app-server `turn/interrupt` |
| Permission callbacks to host | `--permission-prompt-tool` (MCP) or SDK control_request | None in exec (never prompts); app-server approval requests |
| Session resume | `--resume/--continue/--session-id/--fork-session` | `exec resume <id>|--last`, `fork` |
| Structured output | `--json-schema` -> `structured_output` | `--output-schema` + `-o` file |
| Model select | `--model`, `--fallback-model` | `-m`, `--oss`, profiles |
| Reasoning select | `--effort low..max/ultracode` | `-c model_reasoning_effort=...` |
| Sandbox | permission rules/modes only (+`--restricted`); no OS sandbox flag | `--sandbox` read-only/workspace-write/danger-full-access (+Windows native sandbox) |
| Approval policy | `--permission-mode`, `--permission-prompts none` | `approval_policy` (on-request/never/granular) |
| MCP per run | `--mcp-config` + `--strict-mcp-config` (stdio/http/sse) | `-c mcp_servers.<id>.*` overrides (stdio, streamable HTTP, bearer env var); `required=true` |
| Cost info | `total_cost_usd` (estimate), per-model usage | token counts only |
| Rate-limit info | `rate_limit_event` in stream | not in exec; app-server `account/rateLimits/*` (UNVERIFIED shapes) |
| Auth check | `claude auth status` (JSON, exit 0/1) | `codex login status` (text, exit 0/1) |
| Auth methods | claude.ai OAuth (subscription), API key, Bedrock/Vertex/Foundry | ChatGPT sign-in, API key (`CODEX_API_KEY`), Enterprise access tokens |
| Isolated config | `--bare`, `--safe-mode`, `--setting-sources`, `--settings`, `CLAUDE_CONFIG_DIR`* | `--ignore-user-config`, `CODEX_HOME` |
| Cancel semantics | SIGTERM => 143, no result; SIGINT ends turn | undocumented |
| Image input | stream-json image blocks | `-i/--image` |
| Third-party subscription-auth policy | Explicit restriction (Section 0) | No explicit statement found (UNVERIFIED); API key recommended |

*`CLAUDE_CONFIG_DIR` from general knowledge, not fetched here [UNVERIFIED]; `claude auth status` does report `configDirectory`.

---

## 6. Recommended Rust adapter design and pitfalls

### 6.1 Shape
- One `ProviderAdapter` trait: `status()`, `run(request) -> impl Stream<Item=NormalizedEvent>`, `cancel(run_id)`. Normalized events: `Started{session_id}`, `TextDelta?`, `Message{text}`, `ToolStarted/ToolFinished{kind,name,args,output}`, `Usage{input,output,cached,cost?}`, `RateLimit{status,resets_at?}`, `Error{retryable,msg}`, `Finished{ok,final_text,structured?}`. Keep the raw JSON line in every normalized event for debugging and golden tests.
- Claude: spawn `claude -p --output-format stream-json --verbose --include-partial-messages --permission-mode <explicit> --strict-mcp-config --mcp-config <inline-or-temp-file> --model .. --effort .. [--session-id <uuid> | --resume <id>]`, prompt on stdin. Prefer `--input-format text` (default) for one-shot; use `--input-format stream-json` only if you need images, queued messages, or host-side permission callbacks.
- Codex: spawn `codex exec --json -C <dir> -s <mode> -m .. -c ... [--output-schema f] [-o last.txt] -` with prompt on stdin. Consider `codex app-server` later for deltas, interrupt and rate limits.
- Resolve binaries explicitly (`where claude`, `where codex`); on Windows npm installs are `.cmd` shims. See below.

### 6.2 Windows pitfalls
1. **`.cmd` shims**: `codex` installed by npm is `codex.cmd` (runs `node bin/codex.js` which spawns a native `codex.exe`). `Command::new("codex")` finds only `.exe`; Rust (since the 2024 BatBadBut fix) refuses/escapes oddly for `.cmd` args containing special chars. Prefer resolving to the real binary in `node_modules/@openai/codex/.../codex.exe` or call `node <path>\bin\codex.js` directly **[exact vendor path layout UNVERIFIED]**. Native Claude install is `claude.exe` (fine).
2. **Quoting JSON args** (`--json-schema`, `--mcp-config` inline, `-c key="value"`): with `Command::arg`, Rust quotes per Windows rules, but a `.cmd` shim re-parses via cmd.exe and mangles `"`, `%`, `^`, `&`. Write schemas and MCP configs to temp files (restrictive ACLs, delete after) and pass paths; for `-c` values avoid shells entirely. Never build a command string.
3. **Process-tree kill**: put the child in a Windows **Job Object** with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` (crate `win32job`, or `windows` crate) so claude/codex, node shims, MCP servers and sandboxed commands die together when Relay cancels or crashes. `child.kill()` only kills the direct process (`TerminateProcess`) and leaves grandchildren. Create child with `CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW` (0x00000200 | 0x08000000) to avoid console windows flashing and to allow `GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid)` as a graceful interrupt attempt before the Job kill. Claude's own SIGTERM handler kills its Bash children, but there is no SIGTERM on Windows, so the Job Object is the reliable path. Note Codex MXC sandbox stops remaining child processes when the foreground command exits (V-doc).
4. **Graceful then forceful**: cancel = (a) try interrupt (Claude stream-json interrupt, Codex app-server `turn/interrupt`, or CTRL_BREAK), (b) wait ~2-5 s for `result`/`turn.completed`, (c) close the Job. Record the run as cancelled regardless of what the CLI printed.
5. **Console/ANSI**: pass `--color never` to codex; set `NO_COLOR=1`, `TERM=dumb` for both. Ensure stdout is a pipe (not a TTY) so claude uses non-interactive paths.
6. **Encoding**: JSONL is UTF-8; read bytes and split on `\n` (tolerate `\r\n`), decode lossy per line. Do not use `BufRead::lines` on a `String` decode that fails on a bad byte; do not assume a BOM.
7. **Line size**: single events can be large (tool results, aggregated command output, base64 images). Use a growable line reader with a high cap (e.g. 16-64 MB) rather than a fixed buffer; claude warns that slow readers delay exit.
8. Claude Code on native Windows historically needs Git Bash and honours `CLAUDE_CODE_GIT_BASH_PATH` **[general knowledge, UNVERIFIED here]**; the local install works, so verify on target machines.
9. Antivirus/first-run: Codex Windows sandbox may need one-time elevated setup (`codex sandbox setup --elevated`, V-doc) and can fail with error 1385 under restrictive policies. Surface `sandbox.log` path to the user.

### 6.3 stdin handling
- Write the prompt, then **close stdin** (drop the handle) so the CLI sees EOF; otherwise `codex exec -` or `claude -p` waits forever. Write from a separate task/thread while concurrently draining stdout and stderr, to avoid pipe-buffer deadlock on large prompts.
- If you run claude with `--input-format stream-json`, keep stdin open and send NDJSON; closing stdin ends the session and cancels pending permission prompts (V-doc).
- Never pass the prompt as a CLI argument for long/secret content (Windows 32k command-line limit, visible in process list).

### 6.4 stdout/stderr, timeouts, health
- Drain stderr always (log ring buffer); surface last N lines on failure. Claude prints failures like missing auth as a `result` on stdout, not only stderr.
- Timeouts: (1) startup timeout waiting for first event (`system/init` for claude, `thread.started` for codex; 30-60 s); (2) idle timeout since last event (long tool calls legit, make it generous and reset on any event incl. `api_retry`); (3) hard wall clock per run. Claude may wait for background work after the final turn (up to 10 min) so wait for the `result` event, then give the process a short grace period and kill the Job.
- Treat `result.is_error` / non-success subtype / `turn.failed` as failure even with exit code 0. Treat missing terminal event + exit non-zero as crash. Retry only on `api_retry`-class errors, which claude already retries internally.
- Verify MCP attach: claude => `init.mcp_servers[name].status == "connected"` and empty `mcp_server_errors`; codex => `required=true` plus watch for `mcp_tool_call` errors **[no init-style event in exec; UNVERIFIED]**.

### 6.5 Security and policy hygiene
- Auth: only run `auth status` commands; never open `~/.claude*` credentials, keychain, or `~/.codex/auth.json`. Do not forward the user's subscription to anyone else, no pooling. Offer API-key mode as the compliant path for any shared/commercial distribution.
- Pass secrets (MCP bearer tokens, API keys) via the child's environment, not argv. Use a per-run random token for the local Relay MCP endpoint, bind to 127.0.0.1 only.
- Pin and record `claude --version` / `codex --version` per run; feature-detect with claude's `init.capabilities` and by version. Both CLIs change quickly (claude 2.1.x releases weekly; many flags are version-gated). Keep a compatibility table and golden JSONL fixtures per version.
- Do not rely on `-p` defaults: set permission mode, tools, MCP config strictness, settings sources, and session persistence explicitly for reproducibility. Consider `--setting-sources` / `--bare` for claude and `--ignore-user-config --ignore-rules` for codex so a user's personal hooks/rules/MCP servers cannot change behaviour.
- Untrusted repo content: `-p` runs project hooks and `.mcp.json` without trust prompt (V-doc). If Relay points the CLI at arbitrary folders, isolate with `--bare`/`--safe-mode`/`--setting-sources user`.
- Naming: per Relay D-111, never surface vendor names in UI/docs; read CLI names from the `cli_names` modules.

---

## 7. Sources (all fetched 2026-10-16 unless noted)

Anthropic:
- https://code.claude.com/docs/en/headless (run programmatically; stream-json, retries, init, SIGTERM, resume)
- https://code.claude.com/docs/en/cli-reference (all flags and `auth` subcommands)
- https://code.claude.com/docs/en/legal-and-compliance (license, authentication and credential use)
- https://code.claude.com/docs/en/agent-sdk/overview (third-party login restriction, branding)
- https://code.claude.com/docs/en/agent-sdk/typescript (SDKMessage types; first ~200k of 292k chars read)
- https://code.claude.com/docs/en/agent-sdk/permissions , /user-input , /streaming-vs-single-mode
- Local: `claude --version` (2.1.300), `claude --help`, `claude auth status`
- Search result only (secondary mirror, UNVERIFIED): https://docs.anthropic.com/zh-TW/docs/claude-code/sdk (permission-prompt-tool payload), https://code.claude.com/docs/en/agent-sdk/approvals (not fetched directly)

OpenAI / Codex (developers.openai.com/codex/* now 308-redirects to learn.chatgpt.com/docs/*; I followed those redirects, the redirect target is server-supplied and I could not independently confirm that learn.chatgpt.com is an official OpenAI property beyond the redirect; the content matched the GitHub source where overlapping):
- https://learn.chatgpt.com/docs/non-interactive-mode (from developers.openai.com/codex/noninteractive)
- https://learn.chatgpt.com/docs/developer-commands?surface=cli (CLI flag reference; first 100k of 140k chars read)
- https://learn.chatgpt.com/docs/auth
- https://learn.chatgpt.com/docs/extend/mcp?surface=cli
- https://learn.chatgpt.com/docs/config-file/config-reference (first 100k of 157k chars read)
- https://learn.chatgpt.com/docs/agent-approvals-security
- https://learn.chatgpt.com/docs/windows/windows-sandbox
- https://learn.chatgpt.com/docs/app-server (first 100k of 143k chars read)
- https://raw.githubusercontent.com/openai/codex/main/codex-rs/exec/src/exec_events.rs (event types; summarised by fetch tool, not verbatim)
- https://raw.githubusercontent.com/openai/codex/main/codex-rs/exec/src/cli.rs (exec clap args; summarised)
- https://raw.githubusercontent.com/openai/codex/main/codex-rs/app-server/README.md (fetched, little relevant)
- https://registry.npmjs.org/@openai/codex/latest (version 0.163.0 confirmed)
- Search only: https://github.com/openai/codex/pull/5302 (rateLimits/read), forge.lthn.ai mirrors, third-party guides (not relied on)

## 8. Unverified / to test before building

1. Real captured JSONL for both CLIs (every "CONSTRUCTED" line). Run a trivial prompt once per CLI and save as golden fixtures. Codex was deliberately not run.
2. Claude `control_request`/`control_response` wire format (interrupt, can_use_tool, set_permission_mode, initialize) and whether `--permission-prompt-tool` payload keys match the search-summary description.
3. Claude `rate_limit_event` units (`resetsAt`, `utilization`) and whether events fire on `allowed` or only on warnings.
4. Codex: `exec resume` flag combinations with `--json`/`-m`/`-c`; whether `-a/--ask-for-approval` is accepted by `exec`; behaviour when approval would be needed in exec; `-c` dotted-path and Windows quoting for `mcp_servers.*`; per-run MCP via `-c` actually connecting in `exec`; `login status` output text; cancel/Ctrl-C behaviour and exit codes; stderr content; whether `reasoning` items appear and when; whether `item.updated` is used beyond `todo_list`; supported reasoning-effort values per model and how to list models headlessly; Windows first-run sandbox setup in a non-interactive context; `.cmd` shim vs native binary path in npm package.
5. OpenAI terms: whether automating a personal ChatGPT login from a third-party app is permitted (no explicit statement found). Anthropic: whether a desktop app that auto-drives the user's own subscription session at scale counts as "ordinary individual usage"; confirm with Anthropic if distributing commercially.
6. Claude `--max-turns` (in online docs, absent from local `--help`), `CLAUDE_CODE_OAUTH_TOKEN`, `CLAUDE_CONFIG_DIR`, Git Bash requirement on Windows, `mcpServers` JSON schema details (mcp docs page not fetched).
7. Token `usage` inner field names for Claude (assume Anthropic API usage object).
