// In-browser mock of the Relay backend, used when the page is not inside the Tauri window.
// It follows docs/ui-contract.md v2 plus the v3 additions (`used` tags, chat answers with
// `intent: "chat"`, web tasks with `step_added`, browser control in get_hybrid): hybrid routing (native / Relay Local AI / cloud), plans with
// verified, done, failed and refused steps, approvals, the privacy ledger, provider install and
// sign-in states, cloud failure with local fallback, agent mode, task history, Relay Local AI
// setup progress and the setup/uninstall screens. Scenarios: see ./scenario.ts.
import type { Transport } from "../backend";
import { applyEvent, emptyChat, addUser, type ChatState } from "../chat";
import {
  applyHybridPatch,
  type ConvSummary,
  type Engines,
  type Hybrid,
  type HybridPatch,
  type ModelInfo,
  type PlanSource,
  type Platform,
  type Project,
  type ProviderId,
  type ProviderInfo,
  type RunOutcome,
  type Settings,
  type StatusPayload,
  type StepRisk,
  type UiEvent,
  type UsedTag,
  type Workflow,
  type WorkflowOutcome,
} from "../schemas";
import { flowFor, readScenario, type Flow, type Scenario } from "./scenario";

type StoredConv = ConvSummary & { items: unknown[]; searchText: string };

const HOUR = 3_600_000;
const DAY = 24 * HOUR;

let uidSeq = 0;
function uid(prefix = ""): string {
  uidSeq += 1;
  return `${prefix}${uidSeq.toString(36)}${Math.random().toString(36).slice(2, 7)}`;
}

type StepDef = {
  id: string;
  title: string;
  tool: string;
  risk: StepRisk;
  result: "verified" | "done" | "failed";
  summary: string;
  confirm?: { title: string; detail: string };
  via?: "native" | "mcp";
  server?: string;
  sensitive_data?: boolean;
};

/** The contract's step info for a mock step (v3 adds via/server/sensitive_data). */
function stepInfo({ id, title, tool, risk, via, server, sensitive_data }: StepDef) {
  return { id, title, tool, risk, via: via ?? "native", ...(server ? { server } : {}), ...(sensitive_data ? { sensitive_data } : {}) };
}

const LOCAL_MODEL_LABEL = "Qwen3 14B";

// v3 chat answer: a question gets a normal reply with a code block (no plan, no task card).
const CODE_ANSWER = `Here is a small Playwright test that searches a store and checks the results. It uses the test runner, so you can run it with \`npx playwright test\`.

\`\`\`ts
import { test, expect } from "@playwright/test";

test("search shows matching results", async ({ page }) => {
  await page.goto("https://www.example-shop.com/");
  await page.getByRole("searchbox", { name: "Search" }).fill("wireless headphones");
  await page.keyboard.press("Enter");

  const results = page.getByTestId("search-result");
  await expect(results.first()).toBeVisible();
  await expect(results).not.toHaveCount(0);
  await expect(page.getByRole("heading", { level: 1 })).toContainText("wireless headphones");
});
\`\`\`

A few tips:

- Prefer \`getByRole\` and \`getByTestId\` over CSS selectors; they survive layout changes.
- Real stores often block automated traffic, so test against a staging site you control.
- Run \`npx playwright codegen <url>\` to record a first draft of a test.`;

const CHAT_GREETING = "Hi! I can work on this PC for you: tidy folders, find files, open apps and check the result of every step. What should we do?";

// v3 web task: the step list grows one browser action at a time, then the run asks the user to act.
const WEB_STEPS: StepDef[] = [
  { id: "w1", title: "Open dice.com in the Relay browser", tool: "browser_navigate", risk: "safe", result: "verified", summary: "The Dice home page is open", via: "mcp", server: "playwright" },
  { id: "w2", title: "Accept only the necessary cookies", tool: "browser_click", risk: "safe", result: "done", summary: "Clicked “Necessary only”", via: "mcp", server: "playwright" },
  { id: "w3", title: "Type “Rust developer” into the job search", tool: "browser_type", risk: "safe", result: "verified", summary: "The search box shows “Rust developer”", via: "mcp", server: "playwright" },
  { id: "w4", title: "Set the location to Remote", tool: "browser_select", risk: "safe", result: "verified", summary: "Location filter: Remote", via: "mcp", server: "playwright" },
  { id: "w5", title: "Run the search", tool: "browser_click", risk: "safe", result: "verified", summary: "86 jobs found", via: "mcp", server: "playwright" },
  { id: "w6", title: "Filter to jobs posted in the last 7 days", tool: "browser_click", risk: "safe", result: "verified", summary: "23 jobs left", via: "mcp", server: "playwright" },
  { id: "w7", title: "Read the first page of results", tool: "browser_snapshot", risk: "safe", result: "done", summary: "Read 20 job cards", via: "mcp", server: "playwright" },
  { id: "w8", title: "Save the 5 best matches to a note", tool: "write_file", risk: "sensitive", result: "verified", summary: "Saved Documents\\dice-rust-jobs.md and opened it to check", via: "native" },
  { id: "w9", title: "Open “Easy Apply” for the top match", tool: "browser_click", risk: "safe", result: "done", summary: "Dice asks you to sign in first", via: "mcp", server: "playwright" },
];
const WEB_ANSWER =
  "I found **23 remote Rust jobs** posted this week and saved the 5 best matches to `Documents\\dice-rust-jobs.md`.\n\nTo apply, Dice needs you to sign in. **Sign in to Dice in the Relay browser window, then say continue** and I'll fill in the application for the top match. I won't submit anything without asking you first.";
type PlanDef = { source: PlanSource; objective: string; notes: string | null; steps: StepDef[]; answer: string };

const SHARE_DEFAULT = [
  { category: "task", items: 1 },
  { category: "window_titles", items: 6 },
  { category: "file_names", items: 41 },
];

function genericPlan(source: PlanSource, text: string): PlanDef {
  return {
    source,
    objective: text.trim().replace(/\s+/g, " ").slice(0, 120),
    notes: null,
    steps: [
      { id: "s1", title: "Find the right app for the job", tool: "find_app", risk: "safe", result: "verified", summary: "Found File Explorer" },
      { id: "s2", title: "Carry out the request", tool: "ui_action", risk: "sensitive", result: "verified", summary: "Changed 3 items; checked each one afterwards" },
      { id: "s3", title: "Check the final result on screen", tool: "screen_check", risk: "safe", result: "verified", summary: "The window shows the expected result" },
    ],
    answer: "Done. Relay carried out all three steps and checked each one afterwards.",
  };
}

const PLANS: Record<"native" | "local" | "download" | "verify-fail" | "refused" | "declined", (source: PlanSource) => PlanDef> = {
  native: () => ({
    source: "native",
    objective: "Open Notepad and type a note",
    notes: null,
    steps: [
      { id: "n1", title: "Open Notepad", tool: "open_app", risk: "safe", result: "verified", summary: "Notepad is open and focused" },
      { id: "n2", title: "Type the note", tool: "type_text", risk: "sensitive", result: "verified", summary: "The editor shows the typed text" },
    ],
    answer: "Notepad is open with your note typed in. Relay checked both steps on screen.",
  }),
  local: (source) => ({
    source,
    objective: "Find out what is using disk space on C:",
    notes: "Read-only: nothing is moved or deleted.",
    steps: [
      { id: "l1", title: "Measure the largest folders on C:", tool: "measure_folders", risk: "safe", result: "done", summary: "Scanned 412,903 files" },
      { id: "l2", title: "Check browser and app caches", tool: "measure_folders", risk: "safe", result: "verified", summary: "19 GB in caches, totals match the drive" },
    ],
    answer:
      "Your **C:** drive has 61 GB free of 476 GB.\n\n1. `AppData\\Local`: **118 GB** (mostly launchers and caches)\n2. `Program Files`: **74 GB**\n3. `Windows`: **31 GB**\n\nThe quickest win is the browser and launcher caches, about **19 GB**.",
  }),
  download: (source) => ({
    source,
    objective: "Tidy up the Downloads folder",
    notes: "Files go to the Recycle Bin, so they can be restored.",
    steps: [
      { id: "d1", title: "List files in Downloads", tool: "list_files", risk: "safe", result: "verified", summary: "1,284 files, 18.3 GB" },
      { id: "d2", title: "Find installers older than 30 days", tool: "find_files", risk: "safe", result: "done", summary: "14 matches" },
      {
        id: "d3",
        title: "Move 23 files to the Recycle Bin",
        tool: "recycle_files",
        risk: "destructive",
        result: "verified",
        summary: "23 files are in the Recycle Bin; Downloads no longer lists them",
        confirm: { title: "Move 23 files to the Recycle Bin?", detail: "C:\\Users\\you\\Downloads\n14 installers older than 30 days and 9 duplicates (3.6 GB)." },
      },
      { id: "d4", title: "Remove empty folders", tool: "remove_empty_dirs", risk: "sensitive", result: "verified", summary: "6 empty folders removed" },
    ],
    answer: "I tidied up **Downloads**: 23 files (3.6 GB) went to the Recycle Bin and 6 empty folders were removed. Everything can be restored from the Recycle Bin.",
  }),
  "verify-fail": (source) => ({
    source,
    objective: "Set the display brightness to 60%",
    notes: null,
    steps: [
      { id: "v1", title: "Open display settings", tool: "open_settings", risk: "safe", result: "verified", summary: "Settings › Display is open" },
      { id: "v2", title: "Set brightness to 60%", tool: "set_slider", risk: "sensitive", result: "failed", summary: "Ran, but the check failed: brightness still reads 100%" },
      { id: "v3", title: "Close Settings", tool: "close_window", risk: "safe", result: "done", summary: "" },
    ],
    answer: "I couldn't confirm the brightness change. The slider moved, but the display still reports 100%. This monitor may control brightness with its own buttons.",
  }),
  refused: (source) => ({
    source,
    objective: "Clear old log files in C:\\Windows\\System32",
    notes: null,
    steps: [
      { id: "r1", title: "List log files older than 7 days", tool: "find_files", risk: "safe", result: "verified", summary: "38 files, 212 MB" },
      {
        id: "r2",
        title: "Delete 38 log files in C:\\Windows\\System32\\LogFiles",
        tool: "delete_files",
        risk: "destructive",
        result: "failed",
        summary: "Refused: C:\\Windows is a protected folder and is not in your allowed folders",
      },
    ],
    answer: "Relay refused to delete files in **C:\\Windows**: it is a protected folder. Nothing was deleted. You can manage allowed folders in Settings › AI & Privacy.",
  }),
  declined: (source) => ({
    source,
    objective: "Empty the Recycle Bin",
    notes: "This cannot be undone.",
    steps: [
      { id: "e1", title: "Check what is in the Recycle Bin", tool: "list_files", risk: "safe", result: "verified", summary: "212 items, 4.1 GB" },
      {
        id: "e2",
        title: "Empty the Recycle Bin",
        tool: "empty_recycle_bin",
        risk: "destructive",
        result: "verified",
        summary: "The Recycle Bin is empty",
        confirm: { title: "Permanently delete 212 items (4.1 GB)?", detail: "Emptying the Recycle Bin cannot be undone." },
      },
    ],
    answer: "The Recycle Bin is empty. 4.1 GB was freed.",
  }),
};

const AGENT_TOOLS: [string, string, string][] = [
  ["screenshot", "Take a screenshot", "1568×882"],
  ["open_app", "Open Mail", "Opened"],
  ["left_click", "Click the Inbox", "OK · screen changed"],
  ["screenshot", "Read the unread emails", "8 unread"],
];
const AGENT_ANSWER = `I read the 8 unread emails in your inbox:

- **Two invoices** are due this week (electricity, internet).
- Your **package** ships tomorrow.
- **Mia** asked whether Thursday at 3 pm still works. No reply sent.
- The rest are newsletters.

Nothing was sent or deleted.`;

function providerBase(id: ProviderId): ProviderInfo {
  return id === "claude"
    ? {
        id: "claude",
        label: "Claude Code",
        installed: true,
        logged_in: true,
        version: "2.4.1",
        auth_method: "Subscription",
        detail: "Signed in",
        caps: {
          structured_output: true,
          session_resume: true,
          mcp_tools: true,
          streaming_deltas: true,
          reasoning_levels: ["default", "think", "think hard"],
          models: ["default", "sonnet", "opus"],
        },
        model: null,
        reasoning: null,
      }
    : {
        id: "codex",
        label: "Codex",
        installed: true,
        logged_in: false,
        version: "0.61.0",
        auth_method: null,
        detail: "Not signed in. Run codex login in a terminal.",
        caps: {
          structured_output: true,
          session_resume: true,
          mcp_tools: true,
          streaming_deltas: false,
          reasoning_levels: ["low", "medium", "high"],
          models: ["default", "gpt-5-codex"],
        },
        model: null,
        reasoning: null,
      };
}

function makeProviders(sc: Scenario): ProviderInfo[] {
  const claude = providerBase("claude");
  const codex = providerBase("codex");
  if (sc.flags.has("all-ready")) Object.assign(codex, { logged_in: true, auth_method: "ChatGPT account", detail: "Signed in" });
  if (sc.flags.has("codex-missing")) Object.assign(codex, { installed: false, logged_in: false, version: null, detail: "Not installed" });
  if (sc.flags.has("claude-signed-out")) Object.assign(claude, { logged_in: false, auth_method: null, detail: "Not signed in. Run claude auth login in a terminal." });
  return [claude, codex];
}

function platformFor(sc: Scenario): Platform {
  if (sc.flags.has("mac")) return { os: "macos", materials: "vibrancy", title_bar: "overlay", version: "15.4" };
  if (sc.flags.has("mica")) return { os: "windows", materials: "mica", title_bar: "native", version: "10.0.26100" };
  const mac = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform);
  return { os: mac ? "macos" : "windows", materials: "none", title_bar: "native", version: "dev" };
}

/** Builds UI-owned conversation items by replaying contract events through the real reducer. */
function replay(text: string, events: UiEvent[], t0: number): unknown[] {
  let s: ChatState = addUser(emptyChat(), text, [], t0);
  events.forEach((ev, i) => {
    if (ev.type === "status" || ev.type === "progress") return;
    s = applyEvent(s, ev as Parameters<typeof applyEvent>[1], t0 + 400 + i * 900);
  });
  return s.items;
}

const PROVIDER_NAMES: Record<string, string> = { claude: "Claude Code", codex: "Codex" };

/** `used` events for a run: the planner, then the tools its steps use (once per label). */
function usedEvents(runId: number, planner: "native" | "local" | "cloud", provider: string | null, steps: StepDef[]): UiEvent[] {
  const out: UiEvent[] = [];
  if (planner === "local") out.push({ type: "used", run_id: runId, kind: "local_ai", label: LOCAL_MODEL_LABEL, detail: "qwen3:14b on this PC" });
  if (planner === "cloud") out.push({ type: "used", run_id: runId, kind: "cloud_ai", label: PROVIDER_NAMES[provider ?? "claude"] ?? "Cloud AI" });
  if (planner === "native" || steps.some((s) => (s.via ?? "native") === "native")) out.push({ type: "used", run_id: runId, kind: "native", label: "Windows" });
  if (steps.some((s) => s.via === "mcp")) out.push({ type: "used", run_id: runId, kind: "mcp", label: "Playwright browser", detail: "MCP server: playwright" });
  return out;
}

const NATIVE_STEP: StepDef[] = [{ id: "-", title: "", tool: "", risk: "safe", result: "done", summary: "" }];

/** The same tags as `usedEvents`, in the shape workflow_get returns. */
function usedTags(planner: "native" | "local" | "cloud", provider: string | null, steps: StepDef[]): UsedTag[] {
  const out: UsedTag[] = [];
  for (const e of usedEvents(0, planner, provider, steps)) if (e.type === "used") out.push({ kind: e.kind, label: e.label, detail: e.detail ?? null });
  return out;
}

function plannerOf(strategy: string): "native" | "local" | "cloud" {
  return strategy === "native" ? "native" : strategy === "local_plan" ? "local" : "cloud";
}

function planEvents(runId: number, route: Extract<UiEvent, { type: "route" }>, plan: PlanDef, share: boolean, outcome: RunOutcome): UiEvent[] {
  const ev: UiEvent[] = [{ type: "run_started", run_id: runId, conv_id: "seed" }, route];
  ev.push(...usedEvents(runId, plannerOf(route.strategy), route.provider, plan.steps));
  if (share) ev.push({ type: "context_shared", run_id: runId, provider: route.provider ?? "claude", categories: SHARE_DEFAULT, bytes: 12_480 });
  ev.push({
    type: "plan_proposed",
    run_id: runId,
    source: plan.source,
    objective: plan.objective,
    notes: plan.notes,
    steps: plan.steps.map(stepInfo),
  });
  for (const st of plan.steps) {
    ev.push({ type: "step_started", run_id: runId, step_id: st.id });
    if (st.confirm) {
      ev.push({ type: "confirm", id: runId * 10, run_id: runId, title: st.confirm.title, detail: st.confirm.detail, risk: "destructive" });
      ev.push({ type: "confirm_resolved", id: runId * 10, approved: true });
    }
    ev.push({ type: "step_finished", run_id: runId, step_id: st.id, status: st.result, summary: st.summary });
    if (st.result === "failed") break;
  }
  ev.push({ type: "assistant_message", run_id: runId, text: plan.answer });
  ev.push({ type: "run_finished", run_id: runId, outcome });
  return ev;
}

/** A chat answer: route with intent "chat", the engine tag, then the streamed reply. No plan. */
function chatEvents(runId: number, provider: string, text: string): UiEvent[] {
  return [
    { type: "run_started", run_id: runId, conv_id: "seed" },
    { type: "route", run_id: runId, strategy: "cloud_plan", provider, reason: "A question: answered in chat", intent: "chat" },
    { type: "used", run_id: runId, kind: "cloud_ai", label: PROVIDER_NAMES[provider] ?? "Cloud AI" },
    { type: "context_shared", run_id: runId, provider, categories: [{ category: "task", items: 1 }], bytes: 96 },
    { type: "assistant_delta", run_id: runId, text: text.slice(0, 60) },
    { type: "assistant_message", run_id: runId, text },
    { type: "run_finished", run_id: runId, outcome: "completed" },
  ];
}

/** A web task: a short plan, then one step_added + started + finished per browser action. */
function webEvents(runId: number, provider: string): UiEvent[] {
  const ev: UiEvent[] = [
    { type: "run_started", run_id: runId, conv_id: "seed" },
    { type: "route", run_id: runId, strategy: "cloud_plan", provider, reason: `${PROVIDER_NAMES[provider] ?? "Cloud AI"} works out each browser step; Relay does them`, intent: "web" },
    ...usedEvents(runId, "cloud", provider, WEB_STEPS),
    { type: "context_shared", run_id: runId, provider, categories: [{ category: "task", items: 1 }, { category: "ui_tree", items: 9 }], bytes: 18_200 },
    { type: "plan_proposed", run_id: runId, source: provider === "codex" ? "codex" : "claude", objective: "Find remote Rust jobs on Dice and get ready to apply", notes: null, steps: [] },
  ];
  for (const st of WEB_STEPS) {
    ev.push({ type: "step_added", run_id: runId, step: stepInfo(st) });
    ev.push({ type: "step_started", run_id: runId, step_id: st.id });
    ev.push({ type: "step_finished", run_id: runId, step_id: st.id, status: st.result, summary: st.summary });
  }
  ev.push({ type: "assistant_message", run_id: runId, text: WEB_ANSWER });
  ev.push({ type: "run_finished", run_id: runId, outcome: "completed" });
  return ev;
}

export function createMockTransport(scenario: Scenario = readScenario()): Transport {
  const sc = scenario;
  const speed = sc.flags.has("fast") ? 0.12 : sc.flags.has("slow") ? 4 : 1;
  const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, Math.max(1, ms * speed)));
  const handlers = new Set<(raw: unknown) => void>();
  const emit = (ev: UiEvent) => {
    // Simulate the IPC boundary: deliver asynchronously and as plain JSON.
    const copy = JSON.parse(JSON.stringify(ev)) as unknown;
    queueMicrotask(() => handlers.forEach((h) => h(copy)));
  };

  const now = Date.now();
  const empty = sc.flags.has("empty");

  // ---- data ------------------------------------------------------------------------------
  const projects: Project[] = empty
    ? []
    : [
        { id: "p-home", name: "Home PC", color: "green", instructions: "Keep things tidy. Never delete anything outside my user folder.", created_at: now - 20 * DAY, updated_at: now - 2 * DAY },
        { id: "p-work", name: "Work", color: "blue", instructions: "", created_at: now - 15 * DAY, updated_at: now - 3 * DAY },
        { id: "p-games", name: "Games", color: "red", instructions: "Steam library lives on D:.", created_at: now - 9 * DAY, updated_at: now - 9 * DAY },
      ];

  const convs: StoredConv[] = [];
  const workflows: Workflow[] = [];

  const settings: Settings = {
    approval_mode: "destructive",
    user_name: "Mohamed Moheyeldin",
    custom_instructions: "",
    auto_setup: true,
    hotkey: "Ctrl+Alt+Shift+Esc",
    version: "0.3.0 (preview)",
    paths: {
      data: "C:\\Users\\you\\AppData\\Local\\dev.relay.operator",
      audit: "C:\\Users\\you\\AppData\\Local\\dev.relay.operator\\runs",
      logs: "C:\\Users\\you\\AppData\\Local\\dev.relay.operator\\logs",
      mcp: "C:\\Users\\you\\AppData\\Local\\dev.relay.operator\\mcp.json",
    },
  };

  let hybrid: Hybrid = {
    privacy_mode: sc.flags.has("local-only") ? "local_only" : sc.flags.has("cloud-preferred") ? "cloud_preferred" : "hybrid_auto",
    default_provider: "claude",
    providers: makeProviders(sc),
    confirm_level: sc.flags.has("confirm-off") ? "off" : "destructive",
    share: { window_titles: true, ui_tree: true, file_names: true, file_contents: false, screenshots: false },
    allowed_roots: ["C:\\Users\\you\\Documents", "C:\\Users\\you\\Downloads", "C:\\Users\\you\\Pictures", "D:\\Projects"],
    protected_roots: ["C:\\Windows", "C:\\Program Files", "C:\\Program Files (x86)", "C:\\ProgramData"],
    disabled_tools: ["run_powershell"],
    tools: [
      { id: "open_app", label: "Open apps", risk: "safe" },
      { id: "screen_check", label: "Read the screen", risk: "safe" },
      { id: "list_files", label: "List and find files", risk: "safe" },
      { id: "measure_folders", label: "Measure folder sizes", risk: "safe" },
      { id: "type_text", label: "Type text", risk: "sensitive" },
      { id: "ui_action", label: "Click and use controls", risk: "sensitive" },
      { id: "set_slider", label: "Change system settings", risk: "sensitive" },
      { id: "remove_empty_dirs", label: "Remove empty folders", risk: "sensitive" },
      { id: "recycle_files", label: "Move files to the Recycle Bin", risk: "destructive" },
      { id: "delete_files", label: "Delete files permanently", risk: "destructive" },
      { id: "empty_recycle_bin", label: "Empty the Recycle Bin", risk: "destructive" },
      { id: "run_powershell", label: "Run PowerShell commands", risk: "destructive" },
    ],
    browser: sc.flags.has("no-node")
      ? { enabled: false, available: false, server: "playwright", detail: "Node.js was not found. Install Node.js 20 or newer, then restart Relay." }
      : { enabled: true, available: true, server: "playwright", detail: "Ready" },
  };
  // Probing the CLIs takes seconds in the real app; `slow-probe` simulates it (not shortened by `fast`).
  const probe = (ms: number) => new Promise<void>((r) => setTimeout(r, sc.flags.has("slow-probe") ? 3000 : Math.max(1, ms * speed)));

  let models: ModelInfo[] = sc.flags.has("local-setup") ? [] : [{ name: "qwen3:14b", size_bytes: 9.3e9, vision: false, tools: true }];
  let selected: string | null = null;
  let localAvailable = !sc.flags.has("local-setup");
  let setup: Engines["setup"] = localAvailable
    ? { state: "ready", stage: "Ready", percent: 100, error: null }
    : { state: "idle", stage: "", percent: null, error: null };
  let setupCancel = false;
  let mcpServers: Record<string, unknown> = {
    github: { command: "npx", args: ["-y", "@modelcontextprotocol/server-github"], env: { GITHUB_TOKEN: "<set me>" } },
  };

  let runSeq = 100;
  let busy = false;
  let cancel = false;
  let confirmSeq = 1;
  const confirmWaiters = new Map<number, (approved: boolean) => void>();

  // ---- seed ------------------------------------------------------------------------------
  const seedConv = (title: string, ageMs: number, extra: Partial<StoredConv> = {}, items?: unknown[]) => {
    const t = now - ageMs;
    const its =
      items ??
      replay(title, [{ type: "run_started", run_id: 1, conv_id: "seed" }, { type: "assistant_message", run_id: 1, text: `This is a saved conversation about **${title.toLowerCase()}**.` }, { type: "run_finished", run_id: 1, outcome: "completed" }], t);
    const c: StoredConv = { id: uid("c"), title, project_id: null, pinned: false, archived: false, created_at: t, updated_at: t, items: its, searchText: title, ...extra };
    convs.push(c);
    return c;
  };

  if (!empty) {
    const t0 = now - 25 * 60_000;
    const downloads = PLANS.download("claude");
    const seededTask = seedConv(
      "Tidy up my Downloads folder",
      25 * 60_000,
      { pinned: true },
      replay(
        "Tidy up my Downloads folder",
        planEvents(
          1,
          { type: "route", run_id: 1, strategy: "cloud_plan", provider: "claude", reason: "Hybrid: Claude Code plans it, Relay runs it here" },
          downloads,
          true,
          "completed",
        ),
        t0,
      ),
    );
    seedConv("Rename vacation photos by date", 2 * HOUR);
    seedConv(
      "Set display brightness to 60%",
      5 * HOUR,
      {},
      replay(
        "Set display brightness to 60%",
        planEvents(
          2,
          { type: "route", run_id: 2, strategy: "local_plan", provider: "local", reason: "Private: Relay Local AI plans it on this PC" },
          PLANS["verify-fail"]("local"),
          false,
          "failed",
        ),
        now - 5 * HOUR,
      ),
    );
    seedConv(
      "Playwright test for a store search",
      3 * HOUR,
      {},
      replay("Example of Playwright code for testing a store search?", chatEvents(3, "claude", CODE_ANSWER), now - 3 * HOUR),
    );
    const webConv = seedConv("Find remote Rust jobs on Dice", 4 * HOUR, {}, replay("Find remote Rust jobs on Dice and help me apply", webEvents(4, "claude"), now - 4 * HOUR));
    seedConv("Uninstall old games from D:", 26 * HOUR, { project_id: "p-games" });
    seedConv("Compare SSD prices", 28 * HOUR);
    seedConv("Export calendar to CSV", 3 * DAY, { project_id: "p-work" });
    seedConv("Find large videos", 4 * DAY);
    seedConv("Clean temp files", 6 * DAY, { project_id: "p-home" });
    seedConv("Organise screenshots folder", 12 * DAY);
    seedConv("Printer driver troubleshooting", 45 * DAY);
    seedConv("Old archived chat", 50 * DAY, { archived: true });

    // Task history: two linked to seeded chats, the rest synthetic.
    const objectives = [
      "Open Notepad and type a shopping list",
      "Find what is using my disk space",
      "Tidy up the Desktop",
      "Turn on night light",
      "Rename screenshots by date",
      "Clear old log files in C:\\Windows\\System32",
      "Empty the Recycle Bin",
      "Check for Windows updates",
      "Move photos from Downloads to Pictures",
      "Close all browser windows",
      "Back up Documents to D:",
      "Set display brightness to 60%",
    ];
    const outcomes: WorkflowOutcome[] = ["completed", "completed", "completed", "failed", "completed", "denied", "declined", "completed", "cancelled", "completed", "timed_out", "failed"];
    const strategies = ["native", "local_plan", "cloud_plan", "cloud_plan", "local_plan", "cloud_plan", "native", "cloud_plan", "local_plan", "native", "cloud_plan", "local_plan"];
    for (let i = 0; i < 40; i++) {
      const k = i % objectives.length;
      const strategy = strategies[k]!;
      const outcome = outcomes[k]!;
      const total = 2 + (i % 4);
      const verified = outcome === "completed" ? (i % 5 === 0 ? total - 1 : total) : Math.max(0, Math.floor(total / 2) - 1);
      const created = now - (i + 1) * 7 * HOUR - (i % 3) * 13 * 60_000;
      const provider = strategy === "cloud_plan" ? (i % 2 ? "codex" : "claude") : strategy === "local_plan" ? "local" : null;
      const steps = Array.from({ length: total }, (_, j) => {
        const failedHere = (outcome === "failed" || outcome === "denied") && j === verified;
        const status = j < verified ? "verified" : failedHere ? "failed" : outcome === "completed" ? "done" : "skipped";
        return {
          id: `w${i}s${j}`,
          title: j === 0 ? "Look at the current state" : j === total - 1 ? "Check the result" : `Step ${j + 1} of the plan`,
          tool: j === 0 ? "screen_check" : "ui_action",
          risk: j === total - 1 ? "safe" : j === 1 ? "sensitive" : "safe",
          status,
          summary: status === "verified" ? "Checked afterwards" : status === "failed" ? (outcome === "denied" ? "Refused: outside your allowed folders" : "The check failed") : "",
          attempts: 1,
        };
      });
      workflows.push({
        id: `wf-${i + 1}`,
        conv_id: i === 0 ? seededTask.id : null,
        objective: i === 0 ? "Tidy up the Downloads folder" : objectives[k]!,
        strategy,
        provider,
        outcome,
        steps_total: total,
        steps_verified: verified,
        created_at: created,
        updated_at: created + (20 + (i % 7) * 9) * 1000,
        plan_source: strategy === "native" ? "native" : provider === "local" ? "local" : (provider ?? "claude"),
        steps,
        shared: strategy === "cloud_plan" ? SHARE_DEFAULT : [],
        shared_bytes: strategy === "cloud_plan" ? 12_480 : 0,
        approvals: outcome === "declined" ? ["Declined: Empty the Recycle Bin"] : i % 4 === 0 ? ["Approved: Move 23 files to the Recycle Bin"] : [],
        errors: outcome === "failed" ? ["The setting didn't change, so the check failed"] : outcome === "timed_out" ? ["No progress for 5 minutes"] : [],
        notes: null,
        used: usedTags(plannerOf(strategy), provider, NATIVE_STEP),
      });
    }
    // A web task (v3): browser steps through the Playwright MCP server, ending with the user's turn.
    workflows.push({
      id: "wf-web",
      conv_id: webConv.id,
      objective: "Find remote Rust jobs on Dice and get ready to apply",
      strategy: "cloud_plan",
      provider: "claude",
      outcome: "completed",
      steps_total: WEB_STEPS.length,
      steps_verified: WEB_STEPS.filter((s) => s.result === "verified").length,
      created_at: now - 4 * HOUR,
      updated_at: now - 4 * HOUR + 48_000,
      plan_source: "claude",
      steps: WEB_STEPS.map((s) => ({ ...stepInfo(s), status: s.result, summary: s.summary, attempts: 1 })),
      shared: [{ category: "task", items: 1 }, { category: "ui_tree", items: 9 }],
      shared_bytes: 18_200,
      approvals: [],
      errors: [],
      notes: null,
      used: usedTags("cloud", "claude", WEB_STEPS),
      intent: "web",
    });
    // The newest entry is the seeded Downloads chat, so history and chat tell the same story.
    Object.assign(workflows[0]!, {
      strategy: "cloud_plan",
      provider: "claude",
      outcome: "completed",
      plan_source: "claude",
      steps_total: downloads.steps.length,
      steps_verified: downloads.steps.filter((s) => s.result === "verified").length,
      steps: downloads.steps.map((s) => ({ ...stepInfo(s), status: s.result, summary: s.summary, attempts: 1 })),
      used: usedTags("cloud", "claude", downloads.steps),
      shared: SHARE_DEFAULT,
      shared_bytes: 12_480,
      approvals: ["Approved: Move 23 files to the Recycle Bin?"],
      errors: [],
      notes: downloads.notes,
      created_at: t0,
      updated_at: t0 + 9_000,
    });
  }

  // ---- helpers ---------------------------------------------------------------------------
  const providerOf = (id: ProviderId) => hybrid.providers.find((p) => p.id === id)!;
  const ready = (p: ProviderInfo) => p.installed && p.logged_in;

  const status = (): StatusPayload => {
    const p = providerOf(hybrid.default_provider);
    return {
      provider: hybrid.privacy_mode === "local_only" ? "Relay Local AI" : p.label,
      ready: hybrid.privacy_mode === "local_only" ? localAvailable : ready(p) || localAvailable,
      detail: hybrid.privacy_mode === "local_only" ? (localAvailable ? "Relay Local AI ready" : "Relay Local AI not set up") : ready(p) ? `${p.label} ready` : `${p.label}: ${p.detail}`,
      busy,
      kill_hotkey: settings.hotkey,
      local_model: localAvailable ? (selected ?? models[0]?.name ?? null) : null,
      route_mode: hybrid.privacy_mode,
    };
  };

  const engines = (): Engines => ({
    route_mode: "auto",
    local: {
      available: localAvailable,
      managed: true,
      base_url: "http://127.0.0.1:11435",
      models,
      selected,
      active: localAvailable ? (models.find((m) => m.name === selected) ?? models[0] ?? null) : null,
      recommended: "qwen3:14b",
      gpu: { name: "NVIDIA GeForce RTX 4070", vram_mb: 12282 },
    },
    cloud: { name: "Claude Code", installed: true, logged_in: true, detail: "Signed in", path: "C:\\Users\\you\\.local\\bin\\claude.exe" },
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
    for (let i = 0; i < parts.length; i += 3) {
      if (cancel) return false;
      emit({ type: "assistant_delta", run_id: runId, text: parts.slice(i, i + 3).join("") });
      await sleep(22);
    }
    return true;
  }

  const askConfirm = async (runId: number, title: string, detail: string): Promise<boolean> => {
    const id = confirmSeq++;
    emit({ type: "confirm", id, run_id: runId, title, detail, risk: "destructive" });
    const approved = await new Promise<boolean>((resolve) => {
      confirmWaiters.set(id, resolve);
      setTimeout(() => resolve(false), 120_000);
    });
    confirmWaiters.delete(id);
    emit({ type: "confirm_resolved", id, approved });
    return approved;
  };

  type Record_ = {
    route: Extract<UiEvent, { type: "route" }> | null;
    plan: PlanDef | null;
    stepStatus: Map<string, string>;
    stepSummary: Map<string, string>;
    approvals: string[];
    errors: string[];
    shared: boolean;
    used: UsedTag[];
  };

  function recordWorkflow(convId: string, text: string, rec: Record_, outcome: WorkflowOutcome, started: number) {
    const plan = rec.plan;
    const steps =
      plan?.steps.map((s) => ({
        ...stepInfo(s),
        status: rec.stepStatus.get(s.id) ?? "skipped",
        summary: rec.stepSummary.get(s.id) ?? "",
        attempts: 1,
      })) ?? [];
    workflows.unshift({
      id: uid("wf-"),
      conv_id: convId,
      objective: plan?.objective ?? text.slice(0, 120),
      strategy: rec.route?.strategy ?? "cloud_agent",
      provider: rec.route?.provider ?? null,
      outcome,
      steps_total: steps.length,
      steps_verified: steps.filter((s) => s.status === "verified").length,
      created_at: started,
      updated_at: Date.now(),
      plan_source: plan?.source ?? "claude",
      steps,
      shared: rec.shared ? SHARE_DEFAULT : [],
      shared_bytes: rec.shared ? 12_480 : 0,
      approvals: rec.approvals,
      errors: rec.errors,
      notes: plan?.notes ?? null,
      used: rec.used,
      intent: rec.route?.intent ?? "action",
    });
  }

  /** Runs a plan: proposal, then each step with optional approval; returns the workflow outcome. */
  async function executePlan(runId: number, plan: PlanDef, rec: Record_): Promise<WorkflowOutcome> {
    rec.plan = plan;
    emit({
      type: "plan_proposed",
      run_id: runId,
      source: plan.source,
      objective: plan.objective,
      notes: plan.notes,
      steps: plan.steps.map(stepInfo),
    });
    await sleep(500);
    for (let i = 0; i < plan.steps.length; i++) {
      const st = plan.steps[i]!;
      if (cancel) return "cancelled";
      if (hybrid.disabled_tools.includes(st.tool)) {
        emit({ type: "step_started", run_id: runId, step_id: st.id });
        const summary = `Refused: the "${hybrid.tools.find((t) => t.id === st.tool)?.label ?? st.tool}" tool is switched off`;
        emit({ type: "step_finished", run_id: runId, step_id: st.id, status: "failed", summary });
        rec.stepStatus.set(st.id, "failed");
        rec.stepSummary.set(st.id, summary);
        rec.errors.push(summary);
        return "denied";
      }
      emit({ type: "step_started", run_id: runId, step_id: st.id });
      const needsConfirm = Boolean(st.confirm) && (hybrid.confirm_level === "always" || (hybrid.confirm_level === "destructive" && st.risk === "destructive"));
      const needsAlways = !st.confirm && hybrid.confirm_level === "always" && st.risk !== "safe";
      if (needsConfirm || needsAlways) {
        const title = st.confirm?.title ?? `${st.title}?`;
        const detail = st.confirm?.detail ?? `Step ${i + 1} of the plan uses ${st.tool}.`;
        const approved = await askConfirm(runId, title, detail);
        rec.approvals.push(`${approved ? "Approved" : "Declined"}: ${title}`);
        if (!approved) {
          emit({ type: "step_finished", run_id: runId, step_id: st.id, status: "skipped", summary: cancel ? "Stopped before it ran" : "Declined by you; not run" });
          rec.stepStatus.set(st.id, "skipped");
          for (const rest of plan.steps.slice(i + 1)) {
            emit({ type: "step_finished", run_id: runId, step_id: rest.id, status: "skipped", summary: "Not run" });
            rec.stepStatus.set(rest.id, "skipped");
          }
          return cancel ? "cancelled" : "declined";
        }
      }
      await sleep(650 + (i % 2) * 300);
      if (cancel) return "cancelled";
      emit({ type: "step_finished", run_id: runId, step_id: st.id, status: st.result, summary: st.summary });
      rec.stepStatus.set(st.id, st.result);
      rec.stepSummary.set(st.id, st.summary);
      if (st.result === "failed") {
        rec.errors.push(st.summary);
        for (const rest of plan.steps.slice(i + 1)) {
          emit({ type: "step_finished", run_id: runId, step_id: rest.id, status: "skipped", summary: "Not run because an earlier step failed" });
          rec.stepStatus.set(rest.id, "skipped");
        }
        return st.summary.startsWith("Refused") ? "denied" : "failed";
      }
    }
    return "completed";
  }

  async function run(runId: number, convId: string, text: string) {
    busy = true;
    cancel = false;
    const started = Date.now();
    const rec: Record_ = { route: null, plan: null, stepStatus: new Map(), stepSummary: new Map(), approvals: [], errors: [], shared: false, used: [] };
    emit({ type: "status", ...status() });
    emit({ type: "run_started", run_id: runId, conv_id: convId });
    await sleep(300);

    const finish = (outcome: RunOutcome, wf: WorkflowOutcome | null) => {
      busy = false;
      if (wf) recordWorkflow(convId, text, rec, wf, started);
      emit({ type: "run_finished", run_id: runId, outcome });
      emit({ type: "status", ...status() });
    };
    const route = (strategy: Extract<UiEvent, { type: "route" }>["strategy"], provider: string | null, reason: string, intent: "chat" | "action" | "web" = "action") => {
      const r = { type: "route" as const, run_id: runId, strategy, provider, reason, intent };
      rec.route = r;
      emit(r);
    };
    /** Emits the `used` tags (once per label) and keeps them for the task history. */
    const tagUsed = (planner: "native" | "local" | "cloud", prov: string | null, steps: StepDef[]) => {
      for (const e of usedEvents(runId, planner, prov, steps)) {
        if (e.type !== "used") continue;
        const model = selected ?? models[0]?.name ?? null;
        const label = e.kind === "local_ai" && model && model !== "qwen3:14b" ? model : e.label;
        if (rec.used.some((u) => u.kind === e.kind && u.label === label)) continue;
        rec.used.push({ kind: e.kind, label, detail: e.detail ?? null });
        emit({ ...e, label });
      }
    };
    const share = (provider: string) => {
      const cats = SHARE_DEFAULT.filter((c) => c.category === "task" || hybrid.share[c.category as keyof Hybrid["share"]]);
      rec.shared = true;
      emit({ type: "context_shared", run_id: runId, provider, categories: cats, bytes: 2_048 + cats.length * 3_400 });
    };
    const outcomeOf = (wf: WorkflowOutcome): RunOutcome => (wf === "completed" ? "completed" : wf === "cancelled" ? "cancelled" : "failed");

    let flow: Flow = flowFor(text, sc.flow);
    const provider = providerOf(hybrid.default_provider);
    const mode = hybrid.privacy_mode;

    if (flow === "chat") {
      // A question: answered in chat by Local AI or the cloud provider; no plan, no task card.
      const local = mode === "local_only" || !ready(provider);
      const answer = /\b(code|example|playwright|script|test)\b/i.test(text) ? CODE_ANSWER : CHAT_GREETING;
      if (local) {
        route("local_plan", "local", "A question: answered in chat", "chat");
        tagUsed("local", null, []);
      } else {
        route("cloud_plan", provider.id, "A question: answered in chat", "chat");
        tagUsed("cloud", provider.id, []);
        emit({ type: "context_shared", run_id: runId, provider: provider.id, categories: [{ category: "task", items: 1 }], bytes: new TextEncoder().encode(text).length + 64 });
      }
      await sleep(250);
      // Streaming providers send deltas first; the final assistant_message replaces the streamed text.
      const ok = local || provider.caps.streaming_deltas ? await stream(runId, answer) : true;
      if (ok) emit({ type: "assistant_message", run_id: runId, text: answer });
      return finish(ok ? "completed" : "cancelled", null);
    }

    if (flow === "not-installed" || flow === "not-signed-in") {
      route("unavailable", provider.id, `${provider.label} is needed for this task but is not ready`);
      await sleep(300);
      emit({
        type: "error",
        run_id: runId,
        kind: flow === "not-installed" ? "cli_not_installed" : "not_logged_in",
        message: flow === "not-installed" ? `${provider.label} was not found on this PC.` : `${provider.label} is not signed in.`,
      });
      rec.errors.push(flow === "not-installed" ? "Provider not installed" : "Provider not signed in");
      return finish("failed", "unavailable");
    }

    if (flow === "native") {
      route("native", null, "Relay can do this itself: open an app and type");
      tagUsed("native", null, PLANS.native("native").steps);
      const wf = await executePlan(runId, PLANS.native("native"), rec);
      if (wf === "completed") emit({ type: "assistant_message", run_id: runId, text: PLANS.native("native").answer });
      return finish(outcomeOf(wf), wf);
    }

    // Everything else needs a planner: local in local_only, otherwise the selected cloud provider.
    const cloudReady = ready(provider);
    const useLocal = mode === "local_only" || flow === "local";
    if (!useLocal && !cloudReady) {
      if (mode === "cloud_preferred") {
        route("unavailable", provider.id, `${provider.label} is preferred for planning but is not ready`);
        emit({
          type: "error",
          run_id: runId,
          kind: provider.installed ? "not_logged_in" : "cli_not_installed",
          message: provider.installed ? `${provider.label} is not signed in.` : `${provider.label} was not found on this PC.`,
        });
        return finish("failed", "unavailable");
      }
      emit({ type: "notice", text: `${provider.label} isn't ready (${provider.installed ? "not signed in" : "not installed"}), so Relay Local AI is planning this one.` });
      flow = flow === "agent" ? "local" : flow;
    }
    const planner: PlanSource = useLocal || !cloudReady ? "local" : provider.id;

    if (flow === "agent" && planner !== "local") {
      route("cloud_agent", provider.id, `${provider.label} works the desktop directly, one step at a time`);
      share(provider.id);
      emit({ type: "engine", run_id: runId, kind: "cloud", name: provider.label });
      tagUsed("cloud", provider.id, NATIVE_STEP);
      const streams = provider.caps.streaming_deltas;
      if (streams && !(await stream(runId, "I'll open Mail and read your unread emails. "))) return finish("cancelled", "cancelled");
      for (let i = 0; i < AGENT_TOOLS.length; i++) {
        const [name, summ, result] = AGENT_TOOLS[i]!;
        if (cancel) return finish("cancelled", "cancelled");
        const toolId = `t${runId}-${i}`;
        emit({ type: "tool_started", run_id: runId, tool_id: toolId, name, summary: summ });
        await sleep(500 + (i % 2) * 400);
        if (cancel) return finish("cancelled", "cancelled");
        emit({ type: "tool_finished", run_id: runId, tool_id: toolId, ok: true, summary: result });
      }
      if (streams) {
        const ok = await stream(runId, AGENT_ANSWER);
        return finish(ok ? "completed" : "cancelled", ok ? "completed" : "cancelled");
      }
      emit({ type: "assistant_message", run_id: runId, text: AGENT_ANSWER });
      return finish("completed", "completed");
    }

    if (flow === "cloud-fail" && planner !== "local") {
      route("cloud_plan", provider.id, `${provider.label} makes the plan; Relay does the steps on this PC`);
      share(provider.id);
      await sleep(900);
      emit({ type: "notice", text: `${provider.label} didn't answer (network error), so Relay Local AI is making the plan instead. Nothing more was sent.` });
      rec.errors.push(`${provider.label}: network error`);
      route("local_plan", "local", "Relay Local AI makes the plan on this PC, because the cloud provider didn't answer");
      emit({ type: "engine", run_id: runId, kind: "local", name: "qwen3:14b" });
      const plan = genericPlan("local", text);
      tagUsed("local", null, plan.steps);
      const wf = await executePlan(runId, plan, rec);
      if (wf === "completed") emit({ type: "assistant_message", run_id: runId, text: "Done with Relay Local AI after the cloud provider didn't answer. Relay checked every step." });
      return finish(outcomeOf(wf), wf);
    }

    if (flow === "web") {
      // Web task: the step list grows one browser action at a time; the run ends with the user's turn.
      const cloud = planner !== "local";
      route(cloud ? "cloud_plan" : "local_plan", cloud ? provider.id : "local", `${cloud ? provider.label : "Relay Local AI"} works out each browser step; Relay does them`, "web");
      tagUsed(cloud ? "cloud" : "local", cloud ? provider.id : null, WEB_STEPS);
      if (cloud) share(provider.id);
      const plan: PlanDef = { source: planner, objective: "Find remote Rust jobs on Dice and get ready to apply", notes: null, steps: WEB_STEPS, answer: WEB_ANSWER };
      rec.plan = plan;
      emit({ type: "plan_proposed", run_id: runId, source: planner, objective: plan.objective, notes: null, steps: [] });
      for (const st of WEB_STEPS) {
        await sleep(350);
        if (cancel) return finish("cancelled", "cancelled");
        emit({ type: "step_added", run_id: runId, step: stepInfo(st) });
        emit({ type: "step_started", run_id: runId, step_id: st.id });
        await sleep(450);
        if (cancel) return finish("cancelled", "cancelled");
        emit({ type: "step_finished", run_id: runId, step_id: st.id, status: st.result, summary: st.summary });
        rec.stepStatus.set(st.id, st.result);
        rec.stepSummary.set(st.id, st.summary);
      }
      emit({ type: "assistant_message", run_id: runId, text: WEB_ANSWER });
      return finish("completed", "completed");
    }

    if (planner === "local") {
      route("local_plan", "local", mode === "local_only" ? "Local only: Relay Local AI makes the plan on this PC" : "Relay Local AI makes the plan on this PC; nothing leaves it");
      emit({ type: "engine", run_id: runId, kind: "local", name: selected ?? models[0]?.name ?? "Relay Local AI" });
    } else {
      route("cloud_plan", provider.id, `${provider.label} makes the plan; Relay does the steps on this PC`);
      share(provider.id);
      emit({ type: "engine", run_id: runId, kind: "cloud", name: provider.label });
    }
    await sleep(500);

    const plan: PlanDef =
      flow === "download" || flow === "verify-fail" || flow === "refused" || flow === "declined" || flow === "local"
        ? PLANS[flow](planner)
        : genericPlan(planner, text);
    tagUsed(planner === "local" ? "local" : "cloud", planner === "local" ? null : provider.id, plan.steps);
    const wf = await executePlan(runId, plan, rec);
    if (wf === "completed" || wf === "failed" || wf === "denied") {
      if (provider.caps.streaming_deltas && planner === "claude") await stream(runId, plan.answer);
      else emit({ type: "assistant_message", run_id: runId, text: wf === "completed" ? plan.answer : plan.answer });
    } else if (wf === "declined") {
      emit({ type: "assistant_message", run_id: runId, text: "Okay, I didn't do that. Nothing was changed." });
    }
    finish(outcomeOf(wf), wf);
  }

  async function runSetup(kind: "setup" | "pull", model?: string) {
    setupCancel = false;
    setup = { state: "running", stage: "Starting", percent: 0, error: null };
    const stages: [string, number][] =
      kind === "setup"
        ? [
            ["Downloading Relay Local AI", 30],
            [`Downloading model ${model ?? "qwen3:14b"}`, 50],
            ["Starting Relay Local AI", 10],
          ]
        : [[`Downloading model ${model}`, 50]];
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
        await sleep(70);
      }
    }
    const name = model ?? "qwen3:14b";
    if (!models.some((m) => m.name === name)) models = [...models, { name, size_bytes: 9.3e9, vision: false, tools: true }];
    localAvailable = true;
    setup = { state: "ready", stage: "Ready", percent: 100, error: null };
    emit({ type: "progress", id: "local-ai", label: "Relay Local AI is ready", percent: 100, done: true, error: null });
    emit({ type: "status", ...status() });
  }

  const commands: Record<string, (a: Record<string, unknown>) => unknown> = {
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
      await sleep(200);
      return status();
    },
    get_platform: () => platformFor(sc),
    conv_list: (a) =>
      convs
        .filter((c) => (a.archived ? c.archived : !c.archived))
        .sort((x, y) => y.updated_at - x.updated_at)
        .map(summary),
    conv_get: async (a) => {
      await sleep(60);
      const c = find(a.id);
      return { ...summary(c), items: structuredClone(c.items) };
    },
    conv_create: (a) => {
      const t = Date.now();
      const c: StoredConv = { id: uid("c"), title: "", project_id: (a.projectId as string) ?? null, pinned: false, archived: false, created_at: t, updated_at: t, items: [], searchText: "" };
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
      for (const w of workflows) if (w.conv_id === a.id) w.conv_id = null;
    },
    search: async (a) => {
      await sleep(80);
      const q = String(a.query ?? "")
        .trim()
        .toLowerCase();
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
      const p: Project = { id: uid("p"), name: String(a.name), color: String(a.color), instructions: String(a.instructions ?? ""), created_at: t, updated_at: t };
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
    get_hybrid: async () => {
      await probe(250);
      return structuredClone(hybrid);
    },
    set_hybrid: async (a) => {
      await sleep(60);
      const patch = a as HybridPatch;
      if (patch.defaultProvider && !hybrid.providers.some((p) => p.id === patch.defaultProvider)) throw new Error("unknown provider");
      if (patch.browserEnabled !== undefined && !hybrid.browser?.available) throw new Error(`Browser control isn't available: ${hybrid.browser?.detail ?? "unknown reason"}`);
      hybrid = applyHybridPatch(hybrid, patch);
      emit({ type: "status", ...status() });
      return structuredClone(hybrid);
    },
    refresh_providers: async () => {
      await probe(1200);
      return structuredClone(hybrid.providers);
    },
    workflow_list: (a) => {
      const convId = typeof a.convId === "string" ? a.convId : null;
      const limit = typeof a.limit === "number" ? a.limit : 200;
      return workflows
        .filter((w) => !convId || w.conv_id === convId)
        .sort((x, y) => y.created_at - x.created_at)
        .slice(0, limit)
        .map(({ id, conv_id, objective, strategy, provider, outcome, steps_total, steps_verified, created_at, updated_at }) => ({
          id,
          conv_id,
          objective,
          strategy,
          provider,
          outcome,
          steps_total,
          steps_verified,
          created_at,
          updated_at,
        }));
    },
    workflow_get: async (a) => {
      await sleep(120);
      const w = workflows.find((x) => x.id === a.id);
      if (!w) throw new Error("task not found");
      return structuredClone(w);
    },
    get_engines: async () => {
      await sleep(500);
      return engines();
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
      await sleep(100);
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
      await sleep(150);
      return ["C:\\Users\\you\\Documents\\budget-2026.xlsx", "C:\\Users\\you\\Pictures\\receipt.png"];
    },
    open_link: () => undefined,
    open_url: () => undefined,
    installer_info: () => {
      const q = new URLSearchParams(typeof location !== "undefined" ? location.search : "");
      const m = q.get("mode");
      const hashMode = typeof location !== "undefined" && location.hash.startsWith("#/installer") ? "install" : null;
      const mode = m === "install" || m === "uninstall" ? m : (hashMode ?? "app");
      return {
        mode,
        version: "0.3.0",
        install_dir: "C:\\Program Files\\Relay",
        installed_version: q.get("update") ? "0.2.0" : null,
        data_dir: settings.paths.data,
        data_bytes: 9.8e9,
      };
    },
    installer_install: async () => {
      const fail = new URLSearchParams(typeof location !== "undefined" ? location.search : "").get("fail");
      const steps = ["Preparing", "Copying Relay", "Creating shortcuts", "Registering Relay with Windows", "Finishing"];
      for (let i = 0; i <= 50; i++) {
        const label = steps[Math.min(steps.length - 1, Math.floor(i / 11))]!;
        if (fail && i === 30) {
          emit({ type: "progress", id: "installer", label, percent: 60, done: true, error: "Could not write to the install folder (access denied)." });
          throw new Error("Could not write to the install folder (access denied).");
        }
        emit({ type: "progress", id: "installer", label, percent: i * 2, done: false, error: null });
        await sleep(40);
      }
      emit({ type: "progress", id: "installer", label: "Relay is installed", percent: 100, done: true, error: null });
    },
    installer_launch: () => undefined,
    uninstall_run: async (a) => {
      const steps = ["Stopping Relay", "Removing files", a.deleteData ? "Deleting your data" : "Keeping your data", "Cleaning up"];
      for (let i = 0; i <= 40; i++) {
        emit({ type: "progress", id: "installer", label: steps[Math.min(steps.length - 1, Math.floor(i / 10.5))]!, percent: Math.round(i * 2.5), done: false, error: null });
        await sleep(40);
      }
      emit({ type: "progress", id: "installer", label: "Relay was removed", percent: 100, done: true, error: null });
    },
    app_quit: () => undefined,
  };

  // Relay Local AI sets itself up on first launch when auto setup is on.
  if (sc.flags.has("local-setup") && sc.flags.has("auto-setup")) setTimeout(() => void runSetup("setup"), 800);

  return {
    kind: "mock",
    async invoke(cmd, args) {
      const f = commands[cmd];
      if (!f) throw new Error(`unknown command: ${cmd}`);
      await sleep(8);
      const v = await f(args ?? {});
      return v === undefined ? undefined : (JSON.parse(JSON.stringify(v)) as unknown);
    },
    async listen(handler) {
      handlers.add(handler);
      return () => handlers.delete(handler);
    },
  };
}
