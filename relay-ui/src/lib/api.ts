// Typed wrappers around the Rust commands in src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";

export type CloseAction = "ask" | "tray" | "quit";

export interface ActiveModel {
    model_id: string;
    quant: string;
}

export interface PausedServer {
    name: string;
    config: unknown;
}

export interface Settings {
    general: { start_at_login: boolean; close_action: CloseAction; language: string };
    claude: { model: string; effort: string };
    local_ai: {
        grounding: ActiveModel | null;
        describe: ActiveModel | null;
        /** The local operator for tasks Claude can't run (D-067); null picks one. */
        agent: { model_id: string } | null;
        idle_unload_secs: number;
        models_dir: string | null;
    };
    connections: { paused: PausedServer[]; secret_names: Record<string, string[]> };
    browser: { preferred: string; lane_enabled: boolean };
    safety: { approval_mode: string; always_ask: string[]; phone_approvals: boolean; ntfy_topic: string };
    privacy: { runs_dir: string | null; retention_days: number };
    updates: { check_on_start: boolean };
    setup: { completed: boolean };
}

export interface AppInfo {
    version: string;
    os: string;
    settings_file: string;
    config_dir: string;
    data_dir: string;
    logs_dir: string;
    playbooks_dir: string;
    default_models_dir: string;
    default_runs_dir: string;
    python: string | null;
    python_present: boolean;
}

export interface ClaudeState {
    installed: boolean;
    path: string | null;
    version: string | null;
    auth: { loggedIn: boolean; authMethod?: string | null; email?: string | null; subscriptionType?: string | null };
}

export interface Gpu {
    name: string;
    vram_total_mb: number;
    vram_free_mb: number;
    driver: string;
}

export type Fit = "fits" | "tight" | "wont_fit" | "no_gpu";

export interface QuantView {
    quant: string;
    download_bytes: number;
    vram_bytes: number;
    fit: Fit;
}

/** A model in relay-screen's store (installed.json). */
export interface InstalledModel {
    id: string;
    repo: string;
    revision: string;
    quant: string;
    size: number;
    ready: boolean;
}

export interface ModelView {
    id: string;
    name: string;
    role: "grounding" | "describe" | "both";
    summary: string;
    license: string;
    /** Hugging Face repo relay-screen downloads from; null until a trusted one is picked. */
    source: string | null;
    default_quant: string;
    quants: QuantView[];
    installed: InstalledModel | null;
    recommended_quant: string | null;
    active_as: string[];
    benchmark: { score: number; latency_ms: number } | null;
}

export interface CustomView {
    model: InstalledModel;
    fit: Fit;
    active_as: string[];
}

export interface LocalAiState {
    gpu: Gpu | null;
    recommendation: { picks: { model_id: string; quant: string; role: string }[]; reason: string };
    models: ModelView[];
    custom: CustomView[];
    grounding: ActiveModel | null;
    describe: ActiveModel | null;
    runtime: { tag: string; cuda: string; server: string } | null;
    error: string | null;
    models_dir: string;
    models_dir_bytes: number;
    idle_unload_secs: number;
}

export type McpKind = "builtin" | "claude_ai" | "plugin" | "local";
export type McpStatus = "connected" | "needs_auth" | "failed" | "disabled" | "not_configured" | "paused" | "unknown";

export interface McpEntry {
    name: string;
    kind: McpKind;
    target: string;
    transport: string;
    status: McpStatus;
    status_text: string;
    /** For plugin servers: the plugin that brings them, e.g. "productivity@synced". */
    plugin: string | null;
}

export interface Plugin {
    id: string;
    version: string | null;
    scope: string | null;
    enabled: boolean;
}

export interface BrowserInfo {
    name: string;
    path: string;
    chromium: boolean;
}

export interface BrowserState {
    enabled: boolean;
    browsers: BrowserInfo[];
    browser: BrowserInfo | null;
    node: boolean;
    profile_dir: string;
    problem: string | null;
    registered: boolean;
}

export interface CatalogServer {
    id: string;
    name: string;
    description: string;
    transport: string;
    command: string;
    args: string[];
    url: string;
    secret_env: string[];
}

export type ServerSpec =
    | { transport: "stdio"; name: string; command: string; args: string[]; env: Record<string, string> }
    | { transport: "http" | "sse"; name: string; url: string; headers: Record<string, string> };

export type ChatEvent =
    | { type: "started"; session_id: string; model: string; mcp_servers: string[] }
    | { type: "text"; text: string }
    | { type: "tool_use"; id: string; name: string; input: string }
    | { type: "tool_result"; tool_use_id: string; is_error: boolean; text: string; image: string | null }
    | { type: "done"; is_error: boolean; result: string; cost_usd: number | null; duration_ms: number | null }
    | { type: "error"; message: string }
    | { type: "fallback"; reason: string }
    | { type: "local_note"; text: string }
    | { type: "local_step"; step: number; thought: string; action: string; result: string };

export interface Check {
    name: string;
    ok: boolean;
    detail: string;
}

export interface UpdateInfo {
    current: string;
    latest: string;
    available: boolean;
    notes: string;
    url: string;
    asset_name: string;
    asset_url: string;
    asset_size: number;
}

export interface Playbook {
    name: string;
    path: string;
    bytes: number;
    modified: number;
}

export const api = {
    appInfo: () => invoke<AppInfo>("app_info"),
    getSettings: () => invoke<Settings>("get_settings"),
    saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
    closeChoice: (action: "tray" | "quit", remember: boolean) => invoke<void>("close_choice", { action, remember }),
    quit: () => invoke<void>("quit_app"),
    openPath: (path: string) => invoke<void>("open_path", { path }),
    cancelJob: (job: string) => invoke<void>("job_cancel", { job }),
    runningJobs: () => invoke<string[]>("jobs_running"),
    setupComplete: () => invoke<Settings>("setup_complete"),

    claudeState: () => invoke<ClaudeState>("claude_state"),
    claudeInstall: () => invoke<void>("claude_install"),
    claudeUpdate: () => invoke<void>("claude_update"),
    claudeVerify: () => invoke<string>("claude_verify"),
    ptyStart: (kind: "login" | "login-console", cols: number, rows: number) => invoke<number>("pty_start", { kind, cols, rows }),
    ptyWrite: (data: string) => invoke<void>("pty_write", { data }),
    ptyResize: (cols: number, rows: number) => invoke<void>("pty_resize", { cols, rows }),
    ptyKill: (id: number) => invoke<void>("pty_kill", { id }),

    localAi: () => invoke<LocalAiState>("local_ai_state"),
    runtimeInstall: () => invoke<void>("runtime_install"),
    modelInstall: (modelId: string, quant: string) => invoke<void>("model_install", { modelId, quant }),
    modelActivate: (role: string, modelId: string) => invoke<Settings>("model_activate", { role, modelId }),
    modelDelete: (modelId: string) => invoke<Settings>("model_delete", { modelId }),
    customAdd: (repo: string, quant: string) => invoke<void>("custom_add", { repo, quant }),

    mcpList: () => invoke<McpEntry[]>("mcp_list"),
    mcpGet: (name: string) => invoke<string>("mcp_get", { name }),
    mcpCatalog: () => invoke<CatalogServer[]>("mcp_catalog"),
    mcpAdd: (spec: ServerSpec, secrets: { key: string; value: string }[]) => invoke<void>("mcp_add", { spec, secrets }),
    mcpRemove: (name: string) => invoke<void>("mcp_remove", { name }),
    mcpPause: (name: string) => invoke<void>("mcp_pause", { name }),
    mcpResume: (name: string) => invoke<void>("mcp_resume", { name }),
    mcpLogin: (name: string) => invoke<void>("mcp_login", { name }),
    mcpLogout: (name: string) => invoke<void>("mcp_logout", { name }),
    plugins: () => invoke<Plugin[]>("plugins_list"),
    pluginSetEnabled: (id: string, enabled: boolean) => invoke<void>("plugin_set_enabled", { id, enabled }),

    chatSend: (prompt: string) => invoke<void>("chat_send", { prompt }),
    chatStop: () => invoke<void>("chat_stop"),
    chatNew: () => invoke<void>("chat_new"),

    browsers: () => invoke<BrowserInfo[]>("browsers"),
    browserState: () => invoke<BrowserState>("browser_state"),
    browserOpen: () => invoke<void>("browser_open"),
    playbooks: () => invoke<Playbook[]>("playbooks_list"),
    playbookDelete: (name: string) => invoke<void>("playbook_delete", { name }),
    doctor: () => invoke<{ checks: Check[]; report: string }>("doctor_run"),
    updateCheck: () => invoke<UpdateInfo>("update_check"),
    updateInstall: (info: UpdateInfo) => invoke<void>("update_install", { info }),
};

export function errorText(e: unknown): string {
    if (typeof e === "string") return e;
    if (e instanceof Error) return e.message;
    return String(e);
}
