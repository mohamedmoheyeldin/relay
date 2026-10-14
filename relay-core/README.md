# relay-core (M2)

Orchestrator and router. Takes a task, splits it into steps and assigns each step a lane:

- **service**: Claude Code + MCP connectors (Gmail, Calendar, Drive, GitHub ...)
- **screen**: relay-screen, following a playbook when one matches
- **judgment**: Claude Code text-only with structured output

Drives `claude -p --output-format stream-json`. See `docs/architecture.md` §5. Not started yet.
