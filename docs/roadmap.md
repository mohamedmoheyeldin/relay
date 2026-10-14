# Roadmap

Target (D-044): **one download, one app.** The owner runs `Relay-Setup.exe`,
the first-run wizard sets up Claude Code, the local model and the browser
extension, and everything else is managed in the app's Settings (local model
picker D-046, MCP hub D-047, all settings D-048). No other app, config file or
terminal is needed.

| Milestone | Scope | Exit |
|---|---|---|
| **M1** relay-screen (Python hands) | Computer toolset + batch, safety, UIA, OCR, app discovery (done). Still to do: model runtime manager (M1-09), local VLM tools (M1-10), model benchmark (M1-11, feeds the D-046 recommendation), playbooks v0 (M1-12), MCP over localhost HTTP (D-033). | [m1-plan.md](m1-plan.md) done, v0.1.0 on `main` |
| **M2** relay-core (Rust daemon) | Always-on daemon: `claude -p` stream-json driver, router and speed ladder (D-034), playbook runner, perception stream (D-040), model runtime and the catalog with downloads (D-037, D-046), MCP hub backend over `claude mcp` (D-047), settings store and API (D-048), browser bridge + Brave extension (D-035). | A task runs end to end from a CLI against the daemon; v0.2.0 |
| **M3** the app and the installer | Tauri + React + Untitled UI app (D-007) with the chat window, approvals, login prompts, the activity log, and the tray with its quick-input hotkey and progress chip. Then: the first-run wizard including the Claude Code setup (D-045), Settings pages (Local AI picker, Connections, Safety and the rest), the one installer bundling core + frozen relay-screen + llama.cpp (D-044), signed auto-updates from GitHub Releases, with the installer built and checked locally (D-050), phone approvals, and the optional uiAccess helper (D-043). | **v1.0.0: a clean Windows install from `Relay-Setup.exe` with no other setup** |
| **M4** later | macOS hands in Rust (D-038), newer GUI models in the catalog, fine-tuning on the owner's own data, research into an "away mode" that runs in a separate Windows session. | n/a |

## v1.0 acceptance (owner's view)
1. Download `Relay-Setup.exe` from the GitHub release and run it. Nothing else
   is installed by hand.
2. The wizard shows the GPU, installs and signs in to Claude Code, recommends a
   local model that fits, downloads it, and walks through the Brave extension.
3. Settings > Local AI lists the models with fit and benchmark scores, and
   switching takes one click.
4. Settings > Connections shows Relay's tools and every MCP server Claude
   sees. Adding one there makes it available to Claude and to Relay.
5. The canonical form flow (architecture §8) runs from the chat window, with
   approvals.
6. Updates arrive from inside the app.

## Non-goals (v1)
- Sharing one Claude subscription between users, or using Relay without
  Claude Code (D-002).
- Platforms other than Windows (macOS in M4, D-038).
- Proactive "watch me and suggest automations".
- Storing credentials in the repo or in config files.
