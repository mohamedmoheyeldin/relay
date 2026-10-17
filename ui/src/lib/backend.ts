// Backend access. Inside the Tauri window this calls the real commands; in a plain browser
// (`pnpm dev`) it falls back to an in-memory mock so the UI can be seen and tested without Rust.
// Every payload that crosses the boundary is validated with the Zod schemas in ./schemas:
// visibly in development (console error + toast), softly in production (warning, data kept).
import * as z from "zod/mini";
import {
  ConvSummarySchema,
  ConversationSchema,
  EnginesSchema,
  EVENT_TYPES,
  HybridSchema,
  InstallerInfoSchema,
  McpConfigSchema,
  PlatformSchema,
  ProjectSchema,
  ProviderInfoSchema,
  RunStartSchema,
  SearchResultSchema,
  SettingsSchema,
  StatusPayloadSchema,
  UiEventSchema,
  WorkflowSchema,
  WorkflowSummarySchema,
  type HybridPatch,
  type LinkKind,
  type SettingsPatch,
  type UiEvent,
} from "./schemas";

export interface Transport {
  readonly kind: "tauri" | "mock";
  invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown>;
  listen(handler: (raw: unknown) => void): Promise<() => void>;
}

export function isTauri(): boolean {
  if (typeof window === "undefined") return false;
  const w = window as unknown as Record<string, unknown>;
  return "__TAURI_INTERNALS__" in w || "__TAURI__" in w;
}

/** Commands return JSON values; tolerate a backend that sends JSON-encoded strings. */
export function decode(v: unknown): unknown {
  if (typeof v === "string") {
    const t = v.trimStart();
    if (t.startsWith("{") || t.startsWith("[")) {
      try {
        return JSON.parse(v) as unknown;
      } catch {
        /* a plain string */
      }
    }
  }
  return v;
}

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
}

// ---- contract violations ---------------------------------------------------------------------

export type ContractViolation = { where: "command" | "event"; name: string; message: string };
const violationListeners = new Set<(v: ContractViolation) => void>();
export function onContractViolation(fn: (v: ContractViolation) => void): () => void {
  violationListeners.add(fn);
  return () => violationListeners.delete(fn);
}
const DEV = import.meta.env.DEV;

function issuesText(error: z.core.$ZodError): string {
  return error.issues
    .slice(0, 3)
    .map((i) => `${i.path.join(".") || "(root)"}: ${i.message}`)
    .join("; ");
}

function report(v: ContractViolation) {
  if (DEV) console.error(`[relay] contract violation in ${v.where} ${v.name}: ${v.message}`);
  else console.warn(`[relay] contract violation in ${v.where} ${v.name}: ${v.message}`);
  violationListeners.forEach((fn) => fn(v));
}

/** Validates a command result. Dev: throws so the problem is visible. Production: warns and keeps the data. */
export function checkResult<S extends z.ZodMiniType>(cmd: string, schema: S, value: unknown): z.infer<S> {
  const r = schema.safeParse(value);
  if (r.success) return r.data as z.infer<S>;
  const message = issuesText(r.error);
  report({ where: "command", name: cmd, message });
  if (DEV) throw new Error(`The backend returned an unexpected ${cmd} result (${message}).`);
  return value as z.infer<S>;
}

/** Validates an event. Invalid events are reported; in production an event with a known type is
 *  still delivered (the reducers are defensive), in development it is dropped. */
export function checkEvent(raw: unknown): UiEvent | null {
  const value = decode(raw);
  const r = UiEventSchema.safeParse(value);
  if (r.success) return r.data;
  const type = value && typeof value === "object" && "type" in value ? String((value as { type: unknown }).type) : "(no type)";
  report({ where: "event", name: type, message: issuesText(r.error) });
  if (!DEV && (EVENT_TYPES as readonly string[]).includes(type)) return value as UiEvent;
  return null;
}

// ---- transports ------------------------------------------------------------------------------

async function tauriTransport(): Promise<Transport> {
  const [{ invoke }, { listen }] = await Promise.all([import("@tauri-apps/api/core"), import("@tauri-apps/api/event")]);
  return {
    kind: "tauri",
    async invoke(cmd, args) {
      try {
        return decode(await invoke(cmd, args));
      } catch (e) {
        throw new Error(errorText(e));
      }
    },
    async listen(handler) {
      return listen<unknown>("relay-event", (e) => handler(e.payload));
    },
  };
}

let transportP: Promise<Transport> | null = null;
export function transport(): Promise<Transport> {
  transportP ??= isTauri() ? tauriTransport() : import("./mock").then((m) => m.createMockTransport());
  return transportP;
}

/** Tests can inject a transport (for example the mock with a fixed scenario). */
export function setTransport(t: Transport | null) {
  transportP = t ? Promise.resolve(t) : null;
}

async function call(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  return (await transport()).invoke(cmd, args);
}
async function callAs<S extends z.ZodMiniType>(cmd: string, schema: S, args?: Record<string, unknown>): Promise<z.infer<S>> {
  return checkResult(cmd, schema, await call(cmd, args));
}

const ConvList = z.array(ConvSummarySchema);
const ProjectList = z.array(ProjectSchema);
const ProviderList = z.array(ProviderInfoSchema);
const WorkflowList = z.array(WorkflowSummarySchema);
const StringList = z.array(z.string());
const Bool = z.boolean();

/** Typed command wrappers (camelCase args; see docs/ui-contract.md). */
export const api = {
  sendMessage: (convId: string, text: string, attachments?: string[]) =>
    callAs("send_message", RunStartSchema, { convId, text, attachments: attachments?.length ? attachments : undefined }),
  stopTask: () => call("stop_task").then(() => undefined),
  answerConfirm: (id: number, approved: boolean) => callAs("answer_confirm", Bool, { id, approved }),
  getStatus: () => callAs("get_status", StatusPayloadSchema),
  getPlatform: () => callAs("get_platform", PlatformSchema),

  convList: (archived = false) => callAs("conv_list", ConvList, archived ? { archived } : {}),
  convGet: (id: string) => callAs("conv_get", ConversationSchema, { id }),
  convCreate: (projectId?: string | null) => callAs("conv_create", ConvSummarySchema, projectId ? { projectId } : {}),
  convSave: (id: string, items: unknown[], searchText: string, title?: string) =>
    call("conv_save", { id, items, searchText, ...(title !== undefined ? { title } : {}) }).then(() => undefined),
  convUpdate: (id: string, patch: { title?: string; pinned?: boolean; archived?: boolean }) =>
    callAs("conv_update", ConvSummarySchema, { id, ...patch }),
  /** Move a chat into a project, or out of one with `null`. */
  convMove: (id: string, projectId: string | null) => callAs("conv_move", ConvSummarySchema, { id, projectId }),
  convDelete: (id: string) => call("conv_delete", { id }).then(() => undefined),
  search: (query: string) => callAs("search", SearchResultSchema, { query }),

  projectList: () => callAs("project_list", ProjectList),
  projectCreate: (name: string, color: string, instructions: string) => callAs("project_create", ProjectSchema, { name, color, instructions }),
  projectUpdate: (id: string, patch: { name?: string; color?: string; instructions?: string }) =>
    callAs("project_update", ProjectSchema, { id, ...patch }),
  projectDelete: (id: string) => call("project_delete", { id }).then(() => undefined),

  getHybrid: () => callAs("get_hybrid", HybridSchema),
  setHybrid: (patch: HybridPatch) => callAs("set_hybrid", HybridSchema, patch as Record<string, unknown>),
  refreshProviders: () => callAs("refresh_providers", ProviderList),
  workflowList: (opts: { convId?: string; limit?: number } = {}) => callAs("workflow_list", WorkflowList, opts),
  workflowGet: (id: string) => callAs("workflow_get", WorkflowSchema, { id }),

  getEngines: () => callAs("get_engines", EnginesSchema),
  setLocalModel: (model: string) => call("set_local_model", { model }).then(() => undefined),
  localAiSetup: (action: "start" | "cancel") => call("local_ai_setup", { action }).then(() => undefined),
  pullModel: (model: string) => call("pull_model", { model }).then(() => undefined),

  getSettings: () => callAs("get_settings", SettingsSchema),
  setSettings: (patch: SettingsPatch) => callAs("set_settings", SettingsSchema, patch),
  mcpGet: () => callAs("mcp_get", McpConfigSchema),
  mcpSet: (servers: Record<string, unknown>) => call("mcp_set", { servers }).then(() => undefined),
  pickFiles: () => callAs("pick_files", StringList),
  openLink: (kind: LinkKind) => call("open_link", { kind }).then(() => undefined),
  openUrl: (url: string) => call("open_url", { url }).then(() => undefined),

  installerInfo: () => callAs("installer_info", InstallerInfoSchema),
  installerInstall: (desktopShortcut: boolean, startWithWindows: boolean) =>
    call("installer_install", { desktopShortcut, startWithWindows }).then(() => undefined),
  installerLaunch: () => call("installer_launch").then(() => undefined),
  uninstallRun: (deleteData: boolean) => call("uninstall_run", { deleteData }).then(() => undefined),
  appQuit: () => call("app_quit").then(() => undefined),
};

/** Subscribes to validated `relay-event` payloads. */
export async function listenEvents(handler: (ev: UiEvent) => void): Promise<() => void> {
  return (await transport()).listen((raw) => {
    const ev = checkEvent(raw);
    if (ev) handler(ev);
  });
}
