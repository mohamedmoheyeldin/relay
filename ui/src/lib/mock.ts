// In-browser mock of the Relay backend, used when the page is not inside the Tauri window.
// It keeps conversations and projects in memory and simulates runs, tool steps, approvals,
// errors, search and the local AI setup, following docs/ui-contract.md.
import type { Transport } from "./backend";
import type {
  ConvSummary,
  Engines,
  ModelInfo,
  Project,
  RouteMode,
  Settings,
  StatusPayload,
  UiEvent,
} from "./types";

type StoredConv = ConvSummary & { items: unknown[]; searchText: string };

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const HOUR = 3_600_000;
const DAY = 24 * HOUR;

function uid(): string {
  return Math.random().toString(36).slice(2, 10) + Date.now().toString(36).slice(-4);
}

const ANSWERS: Record<string, string> = {
  downloads: `I tidied up your **Downloads** folder.

| What | Count | Space freed |
|---|---:|---:|
| Installers (\`.exe\`, \`.msi\`) older than 30 days | 14 | 3.2 GB |
| Duplicate files | 9 | 412 MB |
| Empty folders | 6 | — |

Everything I removed went to the **Recycle Bin**, so you can restore it. Here is the command I used to find duplicates, in case you want to run it yourself:

\`\`\`powershell
Get-ChildItem "$env:USERPROFILE\\Downloads" -File -Recurse |
  Group-Object { (Get-FileHash $_.FullName).Hash } |
  Where-Object Count -gt 1 |
  ForEach-Object { $_.Group | Select-Object -Skip 1 }
\`\`\`

Anything else you'd like me to clean up?`,
  disk: `Here is where your space went on **C:** (476 GB total, 61 GB free):

1. \`C:\\Users\\you\\AppData\\Local\` — **118 GB** (mostly game launchers and caches)
2. \`C:\\Program Files\` — **74 GB**
3. \`C:\\Windows\` — **31 GB**
4. \`C:\\Users\\you\\Videos\` — **27 GB**

> Tip: the biggest quick win is the browser and launcher caches, about **19 GB** together.

\`\`\`json
{ "drive": "C:", "free_gb": 61, "largest": "AppData\\\\Local", "cache_gb": 19 }
\`\`\``,
  email: `I opened your inbox and read the 8 unread emails. Summary:

- **Two invoices** are due this week (electricity, internet).
- Your **package** from the hardware store ships tomorrow.
- **Mia** asked whether Thursday at 3 pm still works. No reply sent yet.
- The rest are newsletters.

Nothing was sent or deleted.`,
  web: `I searched the web and compared the top results.

| Option | Price | Rating |
|---|---:|---:|
| Option A | $129 | 4.6 |
| Option B | $99 | 4.3 |
| Option C | $149 | 4.8 |

**Option C** has the best reviews; **Option B** is the best value.`,
  default: `Done. Here's a quick example of what I can run for you:

\`\`\`ts
// Rename every screenshot in a folder by date
import { readdirSync, renameSync, statSync } from "node:fs";
for (const f of readdirSync(".")) {
  if (!f.endsWith(".png")) continue;
  const d = statSync(f).mtime.toISOString().slice(0, 10);
  renameSync(f, \`\${d}-\${f}\`);
}
\`\`\`

Let me know if you want me to run it on a real folder. Inline code like \`npm run build\` and [links](https://example.com) render too.`,
};

const TOOLS: Record<string, [string, string, string][]> = {
  downloads: [
    ["powershell", "List files in Downloads", "1,284 files, 18.3 GB"],
    ["powershell", "Find installers older than 30 days", "14 matches"],
    ["powershell", "Hash files to find duplicates", "9 duplicates"],
    ["delete_files", "Move 23 files to the Recycle Bin", "Moved 23 files"],
    ["powershell", "Remove empty folders", "6 removed"],
  ],
  disk: [
    ["powershell", "Measure folder sizes on C:", "Scanned 412,903 files"],
    ["powershell", "Check browser caches", "19 GB in caches"],
  ],
  email: [
    ["screenshot", "Take a screenshot", "1568×882"],
    ["open", "Open Mail", "Opened"],
    ["left_click", "Click the Inbox", "OK · screen changed"],
    ["screenshot", "Read unread emails", "8 unread"],
  ],
  web: [
    ["web_search", "Search the web", "10 results"],
    ["web_fetch", "Read 3 product pages", "3 pages"],
  ],
  default: [
    ["screenshot", "Take a screenshot", "1568×882"],
    ["powershell", "Check the current folder", "OK"],
  ],
};

function topic(text: string): string {
  const t = text.toLowerCase();
  if (t.includes("download")) return "downloads";
  if (t.includes("disk") || t.includes("space")) return "disk";
  if (t.includes("mail")) return "email";
  if (t.includes("web") || t.includes("search")) return "web";
  return "default";
}

export function createMockTransport(): Transport {
  const handlers = new Set<(ev: UiEvent) => void>();
  const emit = (ev: UiEvent) => {
    // Simulate the IPC boundary: deliver asynchronously and as plain JSON.
    const copy = JSON.parse(JSON.stringify(ev)) as UiEvent;
    queueMicrotask(() => handlers.forEach((h) => h(copy)));
  };

  const now = Date.now();
  const projects: Project[] = [
    { id: "p-home", name: "Home PC", color: "green", instructions: "Keep things tidy. Never delete anything outside my user folder.", created_at: now - 20 * DAY, updated_at: now - 2 * DAY },
    { id: "p-work", name: "Work", color: "blue", instructions: "", created_at: now - 15 * DAY, updated_at: now - 3 * DAY },
    { id: "p-games", name: "Games", color: "red", instructions: "Steam library lives on D:.", created_at: now - 9 * DAY, updated_at: now - 9 * DAY },
  ];
  const convs: StoredConv[] = [];
  const seed = (title: string, ageMs: number, extra: Partial<StoredConv> = {}) => {
    const t = now - ageMs;
    const items = [
      { kind: "user", id: uid(), text: title, attachments: [], at: t },
      { kind: "assistant", id: uid(), run_id: 1, text: `This is a saved conversation about **${title.toLowerCase()}**.`, streaming: false, engine: null },
    ];
    convs.push({ id: uid(), title, project_id: null, pinned: false, archived: false, created_at: t, updated_at: t, items, searchText: title, ...extra });
  };
  seed("Set up a backup schedule", 40 * 60_000, { pinned: true });
  seed("Rename vacation photos by date", 2 * HOUR);
  seed("Why is my fan so loud", 5 * HOUR);
  seed("Uninstall old games from D:", 26 * HOUR, { project_id: "p-games" });
  seed("Compare SSD prices", 28 * HOUR);
  seed("Export calendar to CSV", 3 * DAY, { project_id: "p-work" });
  seed("Find large videos", 4 * DAY);
  seed("Clean temp files", 6 * DAY, { project_id: "p-home" });
  seed("Organise screenshots folder", 12 * DAY);
  seed("Printer driver troubleshooting", 45 * DAY);
  seed("Old archived chat", 50 * DAY, { archived: true });

  const settings: Settings = {
    approval_mode: "off",
    user_name: "Mohamed Moheyeldin",
    custom_instructions: "",
    auto_setup: true,
    hotkey: "Ctrl+Alt+Shift+Esc",
    version: "0.2.0 (preview)",
    paths: {
      data: "C:\\Users\\you\\AppData\\Local\\dev.relay.operator",
      audit: "C:\\Users\\you\\AppData\\Local\\dev.relay.operator\\runs",
      logs: "C:\\Users\\you\\AppData\\Local\\dev.relay.operator\\logs",
      mcp: "C:\\Users\\you\\AppData\\Local\\dev.relay.operator\\mcp.json",
    },
  };

  let routeMode: RouteMode = "auto";
  let models: ModelInfo[] = [];
  let selected: string | null = null;
  let localAvailable = false;
  let setup: Engines["setup"] = { state: "idle", stage: "", percent: null, error: null };
  let setupCancel = false;
  let mcpServers: Record<string, unknown> = {
    github: { command: "npx", args: ["-y", "@modelcontextprotocol/server-github"], env: { GITHUB_TOKEN: "<set me>" } },
  };

  let runSeq = 100;
  let busy = false;
  let cancel = false;
  let confirmSeq = 1;
  const confirmWaiters = new Map<number, (approved: boolean) => void>();

  const status = (): StatusPayload => ({
    provider: routeMode === "local_only" ? "Relay Local AI" : "Claude",
    ready: true,
    detail: localAvailable ? "Local AI and Claude ready" : "Claude ready",
    busy,
    kill_hotkey: settings.hotkey,
    local_model: localAvailable ? (selected ?? models[0]?.name ?? null) : null,
    route_mode: routeMode,
  });

  const engines = (): Engines => ({
    route_mode: routeMode,
    local: {
      available: localAvailable,
      managed: true,
      base_url: "http://127.0.0.1:11434",
      models,
      selected,
      active: localAvailable ? (models.find((m) => m.name === selected) ?? models[0] ?? null) : null,
      recommended: "qwen3:14b",
      gpu: { name: "NVIDIA GeForce RTX 4070", vram_mb: 12282 },
    },
    cloud: { name: "Claude", installed: true, logged_in: true, detail: "Signed in", path: "C:\\Users\\you\\.local\\bin\\claude.exe" },
    setup,
  });

  const summary = (c: StoredConv): ConvSummary => ({
    id: c.id,
    title: c.title,
    project_id: c.project_id,
    pinned: c.pinned,
    archived: c.archived,
    created_at: c.created_at,
    updated_at: c.updated_at,
  });
  const find = (id: unknown) => {
    const c = convs.find((x) => x.id === id);
    if (!c) throw new Error("conversation not found");
    return c;
  };

  async function stream(runId: number, text: string) {
    const parts = text.match(/\S+\s*|\s+/g) ?? [text];
    for (let i = 0; i < parts.length; i += 2) {
      if (cancel) return false;
      emit({ type: "assistant_delta", run_id: runId, text: parts.slice(i, i + 2).join("") });
      await sleep(18 + Math.random() * 22);
    }
    return true;
  }

  async function run(runId: number, convId: string, text: string) {
    busy = true;
    cancel = false;
    emit({ type: "status", ...status() });
    emit({ type: "run_started", run_id: runId, conv_id: convId });
    await sleep(250);
    const t = text.toLowerCase();
    const finish = (outcome: "completed" | "cancelled" | "failed") => {
      busy = false;
      emit({ type: "run_finished", run_id: runId, outcome });
      emit({ type: "status", ...status() });
    };

    if (t.includes("error") || t.includes("login")) {
      emit({ type: "engine", run_id: runId, kind: "cloud", name: "Claude" });
      await sleep(500);
      emit({ type: "error", run_id: runId, kind: "not_logged_in", message: "Claude is not signed in." });
      return finish("failed");
    }
    if (t.includes("not installed")) {
      emit({ type: "error", run_id: runId, kind: "cli_not_installed", message: "The Claude CLI was not found on PATH." });
      return finish("failed");
    }

    const key = topic(text);
    const useLocal = routeMode !== "cloud_only" && localAvailable && key !== "web";
    emit({ type: "engine", run_id: runId, kind: useLocal ? "local" : "cloud", name: useLocal ? (selected ?? models[0]?.name ?? "local") : "Claude" });
    if (key === "web" && routeMode === "auto" && localAvailable) {
      emit({ type: "notice", text: "Handed over to Claude for web browsing." });
    }
    if (key === "default" && text.length < 12) {
      // Short chit-chat: plain answer, no tools.
      const ok = await stream(runId, "Hi! I can work on your PC for you: tidy folders, find files, open apps, fill in forms or look things up. What should we do?");
      return finish(ok ? "completed" : "cancelled");
    }
    if (!(await stream(runId, "Sure, I'll take care of that. "))) return finish("cancelled");

    const steps = TOOLS[key] ?? TOOLS.default!;
    for (let i = 0; i < steps.length; i++) {
      const [name, summ, result] = steps[i]!;
      if (cancel) return finish("cancelled");
      const toolId = `t${runId}-${i}`;
      if (name === "delete_files" && settings.approval_mode !== "off") {
        const id = confirmSeq++;
        emit({ type: "confirm", id, run_id: runId, title: "Move 23 files to the Recycle Bin?", detail: "Installers older than 30 days and duplicate files in C:\\Users\\you\\Downloads.", risk: "destructive" });
        const approved = await new Promise<boolean>((resolve) => {
          confirmWaiters.set(id, resolve);
          setTimeout(() => resolve(false), 120_000);
        });
        confirmWaiters.delete(id);
        emit({ type: "confirm_resolved", id, approved });
        if (!approved) {
          emit({ type: "tool_started", run_id: runId, tool_id: toolId, name, summary: summ });
          emit({ type: "tool_finished", run_id: runId, tool_id: toolId, ok: false, summary: "Declined" });
          continue;
        }
      }
      emit({ type: "tool_started", run_id: runId, tool_id: toolId, name, summary: summ });
      await sleep(450 + Math.random() * 600);
      if (cancel) return finish("cancelled");
      emit({ type: "tool_finished", run_id: runId, tool_id: toolId, ok: true, summary: result });
      if (i === 1 && i < steps.length - 1) {
        if (!(await stream(runId, "Found what I need, continuing. "))) return finish("cancelled");
      }
    }
    await sleep(200);
    const ok = await stream(runId, ANSWERS[key] ?? ANSWERS.default!);
    finish(ok ? "completed" : "cancelled");
  }

  async function runSetup(kind: "setup" | "pull", model?: string) {
    setupCancel = false;
    setup = { state: "running", stage: "Starting", percent: 0, error: null };
    const stages: [string, number][] =
      kind === "setup"
        ? [["Downloading Relay Local AI engine", 40], [`Downloading model ${model ?? "qwen3:14b"}`, 70], ["Starting Relay Local AI", 12]]
        : [[`Downloading model ${model}`, 60]];
    for (const [label, ticks] of stages) {
      for (let i = 0; i <= ticks; i++) {
        if (setupCancel) {
          setup = { state: "idle", stage: "", percent: null, error: null };
          emit({ type: "progress", id: "local-ai", label: "Cancelled", percent: null, done: true, error: "Cancelled" });
          return;
        }
        const percent = Math.round((i / ticks) * 100);
        setup = { ...setup, stage: label, percent };
        emit({ type: "progress", id: "local-ai", label, percent, done: false, error: null });
        await sleep(60);
      }
    }
    const name = model ?? "qwen3:14b";
    if (!models.some((m) => m.name === name)) models = [...models, { name, size_bytes: 9.3e9, vision: false, tools: true }];
    localAvailable = true;
    setup = { state: "ready", stage: "Ready", percent: 100, error: null };
    emit({ type: "progress", id: "local-ai", label: "Relay Local AI is ready", percent: 100, done: true, error: null });
    emit({ type: "status", ...status() });
  }

  const commands: Record<string, (a: Record<string, unknown>) => unknown | Promise<unknown>> = {
    send_message: (a) => {
      const text = String(a.text ?? "").trim();
      if (!text) throw new Error("Message is empty");
      if (busy) throw new Error("Relay is busy with another task");
      const c = find(a.convId);
      const runId = ++runSeq;
      c.updated_at = Date.now();
      void run(runId, c.id, text);
      return { run_id: runId };
    },
    stop_task: () => {
      cancel = true;
      for (const [, w] of confirmWaiters) w(false);
    },
    answer_confirm: (a) => {
      const w = confirmWaiters.get(Number(a.id));
      if (!w) return false;
      w(Boolean(a.approved));
      return true;
    },
    get_status: async () => {
      await sleep(250);
      return status();
    },
    conv_list: (a) =>
      convs
        .filter((c) => (a.archived ? c.archived : !c.archived))
        .sort((x, y) => y.updated_at - x.updated_at)
        .map(summary),
    conv_get: (a) => {
      const c = find(a.id);
      return { ...summary(c), items: structuredClone(c.items) };
    },
    conv_create: (a) => {
      const t = Date.now();
      const c: StoredConv = { id: uid(), title: "", project_id: (a.projectId as string) ?? null, pinned: false, archived: false, created_at: t, updated_at: t, items: [], searchText: "" };
      convs.push(c);
      return summary(c);
    },
    conv_save: (a) => {
      const c = find(a.id);
      c.items = structuredClone(a.items as unknown[]);
      c.searchText = String(a.searchText ?? "");
      if (typeof a.title === "string") c.title = a.title;
      c.updated_at = Date.now();
    },
    conv_update: (a) => {
      const c = find(a.id);
      if (typeof a.title === "string") c.title = a.title;
      if (typeof a.pinned === "boolean") c.pinned = a.pinned;
      if (typeof a.archived === "boolean") c.archived = a.archived;
      return summary(c);
    },
    conv_move: (a) => {
      const c = find(a.id);
      const pid = (a.projectId as string | null) ?? null;
      if (pid && !projects.some((p) => p.id === pid)) throw new Error("project not found");
      c.project_id = pid;
      return summary(c);
    },
    conv_delete: (a) => {
      const i = convs.findIndex((c) => c.id === a.id);
      if (i >= 0) convs.splice(i, 1);
    },
    search: async (a) => {
      await sleep(80);
      const q = String(a.query ?? "").trim().toLowerCase();
      if (!q) return { conversations: [], projects: [] };
      const conversations = convs
        .filter((c) => !c.archived && (c.title.toLowerCase().includes(q) || c.searchText.toLowerCase().includes(q)))
        .sort((x, y) => y.updated_at - x.updated_at)
        .slice(0, 20)
        .map((c) => {
          const src = c.searchText || c.title;
          const i = src.toLowerCase().indexOf(q);
          const start = Math.max(0, i - 30);
          const snippet = (start > 0 ? "…" : "") + src.slice(start, start + 110).replace(/\s+/g, " ");
          return { ...summary(c), snippet };
        });
      return { conversations, projects: projects.filter((p) => p.name.toLowerCase().includes(q)) };
    },
    project_list: () => [...projects].sort((a, b) => b.updated_at - a.updated_at),
    project_create: (a) => {
      const t = Date.now();
      const p: Project = { id: uid(), name: String(a.name), color: String(a.color), instructions: String(a.instructions ?? ""), created_at: t, updated_at: t };
      projects.push(p);
      return p;
    },
    project_update: (a) => {
      const p = projects.find((x) => x.id === a.id);
      if (!p) throw new Error("project not found");
      if (typeof a.name === "string") p.name = a.name;
      if (typeof a.color === "string") p.color = a.color;
      if (typeof a.instructions === "string") p.instructions = a.instructions;
      p.updated_at = Date.now();
      return { ...p };
    },
    project_delete: (a) => {
      const i = projects.findIndex((x) => x.id === a.id);
      if (i >= 0) projects.splice(i, 1);
      for (const c of convs) if (c.project_id === a.id) c.project_id = null;
    },
    get_engines: async () => {
      await sleep(700);
      return engines();
    },
    set_route_mode: (a) => {
      routeMode = a.mode as RouteMode;
      emit({ type: "status", ...status() });
    },
    set_local_model: (a) => {
      selected = String(a.model) || null;
      emit({ type: "status", ...status() });
    },
    local_ai_setup: (a) => {
      if (a.action === "cancel") setupCancel = true;
      else if (setup.state !== "running") void runSetup("setup");
    },
    pull_model: (a) => {
      if (setup.state !== "running") void runSetup("pull", String(a.model));
    },
    get_settings: async () => {
      await sleep(120);
      return structuredClone(settings);
    },
    set_settings: (a) => {
      Object.assign(settings, a);
      return structuredClone(settings);
    },
    mcp_get: () => ({ path: settings.paths.mcp, servers: structuredClone(mcpServers), error: null }),
    mcp_set: (a) => {
      const s = a.servers;
      if (!s || typeof s !== "object" || Array.isArray(s)) throw new Error("servers must be an object");
      mcpServers = structuredClone(s as Record<string, unknown>);
    },
    pick_files: async () => {
      await sleep(200);
      return ["C:\\Users\\you\\Documents\\budget-2026.xlsx", "C:\\Users\\you\\Pictures\\receipt.png"];
    },
    open_link: () => undefined,
    installer_info: () => {
      const q = new URLSearchParams(location.search).get("mode");
      const mode = q === "install" || q === "uninstall" ? q : "app";
      return {
        mode,
        version: "0.2.0",
        install_dir: "C:\\Users\\you\\AppData\\Local\\Programs\\Relay",
        installed_version: new URLSearchParams(location.search).get("update") ? "0.1.15" : null,
        data_dir: settings.paths.data,
        data_bytes: 9.8e9,
      };
    },
    installer_install: async () => {
      const fail = new URLSearchParams(location.search).get("fail");
      const steps = ["Preparing", "Copying Relay", "Creating shortcuts", "Registering Relay with Windows", "Finishing"];
      for (let i = 0; i <= 50; i++) {
        const label = steps[Math.min(steps.length - 1, Math.floor(i / 11))]!;
        if (fail && i === 30) {
          emit({ type: "progress", id: "installer", label, percent: 60, done: true, error: "Could not write to the install folder (access denied)." });
          throw new Error("Could not write to the install folder (access denied).");
        }
        emit({ type: "progress", id: "installer", label, percent: i * 2, done: false, error: null });
        await sleep(45);
      }
      emit({ type: "progress", id: "installer", label: "Relay is installed", percent: 100, done: true, error: null });
    },
    installer_launch: () => undefined,
    uninstall_run: async (a) => {
      const steps = ["Stopping Relay", "Removing files", a.deleteData ? "Deleting your data" : "Keeping your data", "Cleaning up"];
      for (let i = 0; i <= 40; i++) {
        emit({ type: "progress", id: "installer", label: steps[Math.min(steps.length - 1, Math.floor(i / 10.5))]!, percent: Math.round(i * 2.5), done: false, error: null });
        await sleep(45);
      }
      emit({ type: "progress", id: "installer", label: "Relay was removed", percent: 100, done: true, error: null });
    },
    app_quit: () => undefined,
  };

  return {
    kind: "mock",
    async invoke<T>(cmd: string, args?: Record<string, unknown>) {
      const f = commands[cmd];
      if (!f) throw new Error(`unknown command: ${cmd}`);
      await sleep(10);
      const v = await f(args ?? {});
      return (v === undefined ? undefined : JSON.parse(JSON.stringify(v))) as T;
    },
    async listen(handler) {
      handlers.add(handler);
      return () => handlers.delete(handler);
    },
  };
}
