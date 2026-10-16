// Types from docs/ui-contract.md. Keep in sync with the backend.

export type Risk = "reversible" | "destructive";
export type ErrorKind = "cli_not_installed" | "not_logged_in" | "timeout" | "cancelled" | "cli" | "internal";
export type RouteMode = "auto" | "local_only" | "cloud_only";
export type RunOutcome = "completed" | "cancelled" | "timed_out" | "failed";

export type StatusPayload = {
  provider: string;
  ready: boolean;
  detail: string;
  busy: boolean;
  kill_hotkey: string;
  local_model: string | null;
  route_mode: RouteMode;
};

export type UiEvent =
  | { type: "run_started"; run_id: number; conv_id: string }
  | { type: "assistant_delta"; run_id: number; text: string }
  | { type: "assistant_message"; run_id: number; text: string }
  | { type: "tool_started"; run_id: number; tool_id: string; name: string; summary: string }
  | { type: "tool_finished"; run_id: number; tool_id: string; ok: boolean; summary: string }
  | { type: "confirm"; id: number; run_id: number; title: string; detail: string; risk: Risk }
  | { type: "confirm_resolved"; id: number; approved: boolean }
  | { type: "run_finished"; run_id: number; outcome: RunOutcome }
  | { type: "error"; run_id: number | null; kind: ErrorKind; message: string }
  | ({ type: "status" } & StatusPayload)
  | { type: "engine"; run_id: number; kind: "local" | "cloud"; name: string }
  | { type: "notice"; text: string }
  | { type: "progress"; id: string; label: string; percent: number | null; done: boolean; error: string | null };

export type ConvSummary = {
  id: string;
  title: string;
  project_id: string | null;
  pinned: boolean;
  archived: boolean;
  created_at: number;
  updated_at: number;
};

export type Conversation = ConvSummary & { items: unknown[] };

export type ProjectColor = "green" | "red" | "blue" | "yellow" | "purple" | "orange" | "pink" | "gray";

export type Project = {
  id: string;
  name: string;
  color: ProjectColor | string;
  instructions: string;
  created_at: number;
  updated_at: number;
};

export type ModelInfo = { name: string; size_bytes: number; vision: boolean; tools: boolean };

export type Engines = {
  route_mode: RouteMode;
  local: {
    available: boolean;
    managed: boolean;
    base_url: string;
    models: ModelInfo[];
    selected: string | null;
    active: ModelInfo | null;
    recommended: string;
    gpu: { name: string; vram_mb: number } | null;
  };
  cloud: { name: string; installed: boolean; logged_in: boolean; detail: string; path: string };
  setup: { state: "idle" | "running" | "ready" | "failed"; stage: string; percent: number | null; error: string | null };
};

export type ApprovalMode = "off" | "destructive" | "always";

export type Settings = {
  approval_mode: ApprovalMode;
  user_name: string;
  custom_instructions: string;
  auto_setup: boolean;
  hotkey: string;
  version: string;
  paths: { data: string; audit: string; logs: string; mcp: string };
};

export type SearchResult = {
  conversations: (ConvSummary & { snippet: string })[];
  projects: Project[];
};

export type McpConfig = { path: string; servers: Record<string, unknown>; error: string | null };

export type Progress = { id: string; label: string; percent: number | null; done: boolean; error: string | null };

export type LinkKind = "data" | "audit" | "logs" | "mcp";

export type InstallerMode = "app" | "install" | "uninstall";
export type InstallerInfo = {
  mode: InstallerMode;
  version: string;
  install_dir: string;
  installed_version: string | null;
  data_dir: string;
  data_bytes: number;
};
