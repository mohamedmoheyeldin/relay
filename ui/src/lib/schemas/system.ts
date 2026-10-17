// Settings, Relay Local AI engines, platform, installer and MCP config (docs/ui-contract.md).
import * as z from "zod/mini";

export const ApprovalModeSchema = z.enum(["off", "destructive", "always"]);

export const SettingsSchema = z.object({
  approval_mode: ApprovalModeSchema,
  user_name: z.string(),
  custom_instructions: z.string(),
  auto_setup: z.boolean(),
  hotkey: z.string(),
  version: z.string(),
  paths: z.object({ data: z.string(), audit: z.string(), logs: z.string(), mcp: z.string() }),
});

export const ModelInfoSchema = z.object({ name: z.string(), size_bytes: z.number(), vision: z.boolean(), tools: z.boolean() });

export const EnginesSchema = z.object({
  route_mode: z.string(),
  local: z.object({
    available: z.boolean(),
    managed: z.boolean(),
    base_url: z.string(),
    models: z.array(ModelInfoSchema),
    selected: z.nullable(z.string()),
    active: z.nullable(ModelInfoSchema),
    recommended: z.string(),
    gpu: z.nullable(z.object({ name: z.string(), vram_mb: z.number() })),
  }),
  cloud: z.object({ name: z.string(), installed: z.boolean(), logged_in: z.boolean(), detail: z.string(), path: z.string() }),
  setup: z.object({
    state: z.enum(["idle", "running", "ready", "failed"]),
    stage: z.string(),
    percent: z.nullable(z.number()),
    error: z.nullable(z.string()),
  }),
});

export const PlatformSchema = z.object({
  os: z.enum(["windows", "macos", "linux"]),
  materials: z.enum(["mica", "vibrancy", "none"]),
  title_bar: z.enum(["native", "overlay", "custom"]),
  version: z.string(),
});

export const InstallerModeSchema = z.enum(["app", "install", "uninstall"]);
export const InstallerInfoSchema = z.object({
  mode: InstallerModeSchema,
  version: z.string(),
  install_dir: z.string(),
  installed_version: z.nullable(z.string()),
  data_dir: z.string(),
  data_bytes: z.number(),
});

export const McpConfigSchema = z.object({
  path: z.string(),
  servers: z.record(z.string(), z.unknown()),
  error: z.nullable(z.string()),
});

export const RunStartSchema = z.object({ run_id: z.number() });

/** `system_check`: is everything Relay can use installed and ready? Slow (probes the CLIs).
 *  Known ids: local_ai, claude, codex, node, browser (others are shown as they come). An action
 *  either opens a web page (`kind: "url"`, via open_url) or runs a known command
 *  (`kind: "command"`, value "local_ai_setup" = local_ai_setup {action: "start"}). */
export const SystemCheckStatusSchema = z.enum(["ok", "warn", "missing"]);
export const SystemCheckActionSchema = z.object({ kind: z.enum(["url", "command"]), value: z.string(), label: z.string() });
export const SystemCheckItemSchema = z.object({
  id: z.string(),
  label: z.string(),
  status: SystemCheckStatusSchema,
  detail: z.string(),
  action: z.nullable(SystemCheckActionSchema),
});
export const SystemCheckSchema = z.object({ items: z.array(SystemCheckItemSchema), ok: z.number(), total: z.number() });
export type SystemCheckStatus = z.infer<typeof SystemCheckStatusSchema>;
export type SystemCheckAction = z.infer<typeof SystemCheckActionSchema>;
export type SystemCheckItem = z.infer<typeof SystemCheckItemSchema>;
export type SystemCheck = z.infer<typeof SystemCheckSchema>;

/** `get_appearance` / `set_appearance`: an object the backend stores without interpreting it (at
 *  most 64 keys). The UI reads it leniently (lib/theme.ts `normalizeAppearance`). */
export const APPEARANCE_MAX_KEYS = 64;
export const AppearanceRecordSchema = z.record(z.string(), z.unknown());
/** A `set_appearance` patch: keys replace, `null` removes a key. */
export type AppearancePatch = Record<string, unknown>;

export type ApprovalMode = z.infer<typeof ApprovalModeSchema>;
export type Settings = z.infer<typeof SettingsSchema>;
export type SettingsPatch = Partial<Pick<Settings, "approval_mode" | "user_name" | "custom_instructions" | "auto_setup">>;
export type ModelInfo = z.infer<typeof ModelInfoSchema>;
export type Engines = z.infer<typeof EnginesSchema>;
export type Platform = z.infer<typeof PlatformSchema>;
export type InstallerMode = z.infer<typeof InstallerModeSchema>;
export type InstallerInfo = z.infer<typeof InstallerInfoSchema>;
export type McpConfig = z.infer<typeof McpConfigSchema>;
export type LinkKind = "data" | "audit" | "logs" | "mcp";
