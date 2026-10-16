// Backend access. Inside the Tauri window this calls the real commands; in a plain browser
// (`pnpm dev`) it falls back to an in-memory mock so the UI can be seen and tested without Rust.
import type {
  InstallerInfo,
  ApprovalMode,
  ConvSummary,
  Conversation,
  Engines,
  LinkKind,
  McpConfig,
  Project,
  RouteMode,
  SearchResult,
  Settings,
  StatusPayload,
  UiEvent,
} from "./types";

export interface Transport {
  readonly kind: "tauri" | "mock";
  invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  listen(handler: (ev: UiEvent) => void): Promise<() => void>;
}

export function isTauri(): boolean {
  if (typeof window === "undefined") return false;
  const w = window as unknown as Record<string, unknown>;
  return "__TAURI_INTERNALS__" in w || "__TAURI__" in w;
}

/** Commands return JSON values; tolerate a backend that sends JSON-encoded strings. */
function decode<T>(v: unknown): T {
  if (typeof v === "string") {
    const t = v.trimStart();
    if (t.startsWith("{") || t.startsWith("[")) {
      try {
        return JSON.parse(v) as T;
      } catch {
        /* fall through: a plain string */
      }
    }
  }
  return v as T;
}

function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
}

async function tauriTransport(): Promise<Transport> {
  const [{ invoke }, { listen }] = await Promise.all([import("@tauri-apps/api/core"), import("@tauri-apps/api/event")]);
  return {
    kind: "tauri",
    async invoke<T>(cmd: string, args?: Record<string, unknown>) {
      try {
        return decode<T>(await invoke(cmd, args));
      } catch (e) {
        throw new Error(errorText(e));
      }
    },
    async listen(handler) {
      return listen<unknown>("relay-event", (e) => {
        const ev = decode<UiEvent>(e.payload);
        if (ev && typeof ev === "object" && "type" in ev) handler(ev);
      });
    },
  };
}

let transportP: Promise<Transport> | null = null;
export function transport(): Promise<Transport> {
  transportP ??= isTauri() ? tauriTransport() : import("./mock").then((m) => m.createMockTransport());
  return transportP;
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return (await transport()).invoke<T>(cmd, args);
}

/** Typed command wrappers (camelCase args; see docs/ui-contract.md). */
export const api = {
  sendMessage: (convId: string, text: string, attachments?: string[]) =>
    call<{ run_id: number }>("send_message", { convId, text, attachments: attachments?.length ? attachments : undefined }),
  stopTask: () => call<void>("stop_task"),
  answerConfirm: (id: number, approved: boolean) => call<boolean>("answer_confirm", { id, approved }),
  getStatus: () => call<StatusPayload>("get_status"),
  convList: (archived?: boolean) => call<ConvSummary[]>("conv_list", archived ? { archived } : {}),
  convGet: (id: string) => call<Conversation>("conv_get", { id }),
  convCreate: (projectId?: string | null) => call<ConvSummary>("conv_create", projectId ? { projectId } : {}),
  convSave: (id: string, items: unknown[], searchText: string, title?: string) =>
    call<void>("conv_save", { id, items, searchText, ...(title !== undefined ? { title } : {}) }),
  convUpdate: (id: string, patch: { title?: string; pinned?: boolean; archived?: boolean }) =>
    call<ConvSummary>("conv_update", { id, ...patch }),
  /** Move a chat into a project, or out of one with `null`. */
  convMove: (id: string, projectId: string | null) => call<ConvSummary>("conv_move", { id, projectId }),
  convDelete: (id: string) => call<void>("conv_delete", { id }),
  search: (query: string) => call<SearchResult>("search", { query }),
  projectList: () => call<Project[]>("project_list"),
  projectCreate: (name: string, color: string, instructions: string) =>
    call<Project>("project_create", { name, color, instructions }),
  projectUpdate: (id: string, patch: { name?: string; color?: string; instructions?: string }) =>
    call<Project>("project_update", { id, ...patch }),
  projectDelete: (id: string) => call<void>("project_delete", { id }),
  getEngines: () => call<Engines>("get_engines"),
  setRouteMode: (mode: RouteMode) => call<void>("set_route_mode", { mode }),
  setLocalModel: (model: string) => call<void>("set_local_model", { model }),
  localAiSetup: (action: "start" | "cancel") => call<void>("local_ai_setup", { action }),
  pullModel: (model: string) => call<void>("pull_model", { model }),
  getSettings: () => call<Settings>("get_settings"),
  setSettings: (patch: Partial<{ approval_mode: ApprovalMode; user_name: string; custom_instructions: string; auto_setup: boolean }>) =>
    call<Settings>("set_settings", patch),
  mcpGet: () => call<McpConfig>("mcp_get"),
  mcpSet: (servers: Record<string, unknown>) => call<void>("mcp_set", { servers }),
  pickFiles: () => call<string[]>("pick_files"),
  openLink: (kind: LinkKind) => call<void>("open_link", { kind }),
  // Setup / uninstall (the release exe is its own installer)
  installerInfo: () => call<InstallerInfo>("installer_info"),
  installerInstall: (desktopShortcut: boolean, startWithWindows: boolean) =>
    call<void>("installer_install", { desktopShortcut, startWithWindows }),
  installerLaunch: () => call<void>("installer_launch"),
  uninstallRun: (deleteData: boolean) => call<void>("uninstall_run", { deleteData }),
  appQuit: () => call<void>("app_quit"),
};

export async function listenEvents(handler: (ev: UiEvent) => void): Promise<() => void> {
  return (await transport()).listen(handler);
}
