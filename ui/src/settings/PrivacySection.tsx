// Settings › AI & Privacy: privacy mode, cloud providers (status, model, reasoning), what context
// may be shared, confirmation level, allowed/protected folders and the tool allowlist.
import { useEffect, useId, useState } from "react";
import { Cloud, FolderLock, FolderOpen, Globe, Plus, RefreshCw, Route, ShieldCheck, Trash2, TriangleAlert } from "lucide-react";
import { useHybrid, useRefreshProviders, useSetHybrid } from "@/hooks/queries";
import type { BrowserInfo, ConfirmLevel, Hybrid, ProviderInfo, Share } from "@/lib/types";
import { DESTRUCTIVE_BADGE } from "@/lib/task";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { RadioCard, RadioGroup } from "@/components/ui/radio-group";
import { Segmented } from "@/components/ui/toggle-group";
import { Switch } from "@/components/ui/switch";
import { Skeleton } from "@/components/ui/skeleton";
import { IconButton } from "@/components/ui/icon-button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { copyText } from "@/hooks/dom";
import { PRIVACY_OPTIONS, providerProblem } from "@/components/chat/ContextControls";
import { Card, LoadState, PageTitle, Row, SectionTitle } from "./parts";

const SHARE_ROWS: { key: keyof Share; label: string; desc: string }[] = [
  { key: "window_titles", label: "Window titles", desc: "Names of open windows, so the provider knows which apps are on screen." },
  { key: "ui_tree", label: "UI controls", desc: "Names and roles of buttons, fields and menus in the window Relay works in." },
  { key: "file_names", label: "File and folder names", desc: "Names and sizes of files involved in the task, never their contents." },
  { key: "file_contents", label: "File contents", desc: "Text from the files involved. Off by default; turn on only when a task needs it." },
  { key: "screenshots", label: "Screenshots", desc: "Images of the screen. Off by default; Relay prefers UI controls instead." },
];

const INSTALL: Record<ProviderInfo["id"], { install: string; login: string }> = {
  claude: { install: "Install Claude Code from its official site (needs a Claude subscription or API access).", login: "claude auth login" },
  codex: { install: "npm install -g @openai/codex", login: "codex login" },
};

export function PrivacySection() {
  const hybrid = useHybrid();
  const set = useSetHybrid();
  const refresh = useRefreshProviders();
  if (!hybrid.data) return <LoadStateWithTitle error={hybrid.error} retry={() => void hybrid.refetch()} />;
  const h = hybrid.data;

  return (
    <>
      <PageTitle description="Decide what may leave this PC, which cloud provider plans tasks, and what Relay may do without asking.">AI &amp; Privacy</PageTitle>

      <SectionTitle id="privacy-mode">Privacy mode</SectionTitle>
      <RadioGroup aria-labelledby="privacy-mode" value={h.privacy_mode} onValueChange={(v) => set.mutate({ privacyMode: v as Hybrid["privacy_mode"] })} className="grid gap-3 sm:grid-cols-3">
        {PRIVACY_OPTIONS.map((o) => (
          <RadioCard key={o.value} value={o.value} aria-label={o.label}>
            <span className="flex items-center gap-2 text-ui font-medium text-fg">
              {o.value === "local_only" ? <ShieldCheck className="size-4 text-success" aria-hidden="true" /> : o.value === "hybrid_auto" ? <Route className="size-4 text-brand-text" aria-hidden="true" /> : <Cloud className="size-4 text-info" aria-hidden="true" />}
              {o.label}
            </span>
            <span className="text-caption leading-5 text-fg-2">{o.explain}</span>
          </RadioCard>
        ))}
      </RadioGroup>

      <SectionTitle
        description="Cloud providers only plan. Relay runs every step on this PC, checks the result, and asks before risky steps."
      >
        Cloud providers
      </SectionTitle>
      <div className="mb-3 flex items-center justify-between gap-3">
        <div className="flex items-center gap-3">
          <span className="text-ui text-fg-2" id="default-provider">
            Default provider
          </span>
          <Segmented
            label="Default provider"
            size="sm"
            value={h.default_provider}
            onValueChange={(v) => set.mutate({ defaultProvider: v })}
            options={h.providers.map((p) => ({ value: p.id, label: p.label }))}
          />
        </div>
        <Button size="sm" variant="secondary" onClick={() => refresh.mutate()} disabled={refresh.isPending} aria-label="Refresh provider status">
          <RefreshCw className={cn(refresh.isPending && "animate-spin")} aria-hidden="true" /> {refresh.isPending ? "Checking…" : "Refresh"}
        </Button>
      </div>
      <div className="grid gap-3 lg:grid-cols-2" aria-busy={refresh.isPending}>
        {h.providers.map((p) => (refresh.isPending ? <Skeleton key={p.id} className="h-[260px] rounded-xl" /> : <ProviderCard key={p.id} p={p} isDefault={p.id === h.default_provider} />))}
      </div>

      {h.browser && <BrowserControl browser={h.browser} onChange={(v) => set.mutate({ browserEnabled: v })} />}

      <SectionTitle description="Only what you switch on here can go into a cloud request. Each task shows exactly what was shared.">What may be shared</SectionTitle>
      <Card>
        {SHARE_ROWS.map((r) => (
          <ShareRow key={r.key} row={r} checked={h.share[r.key]} onChange={(v) => set.mutate({ share: { [r.key]: v } })} />
        ))}
      </Card>

      <SectionTitle>Ask before acting</SectionTitle>
      <Card>
        <Row label="Confirmation level" desc="Unanswered requests are declined after 2 minutes. Declined steps are never run." stack>
          <Segmented<ConfirmLevel>
            label="Confirmation level"
            value={h.confirm_level}
            onValueChange={(v) => set.mutate({ confirmLevel: v })}
            options={[
              { value: "off", label: "Never ask" },
              { value: "destructive", label: "Destructive steps" },
              { value: "always", label: "Every step that changes something" },
            ]}
            className="max-w-full flex-wrap"
          />
        </Row>
      </Card>

      <SectionTitle description="Plans may only touch files inside these folders. Anything else is refused before it runs.">Allowed folders</SectionTitle>
      <AllowedFolders roots={h.allowed_roots} onChange={(allowedRoots) => set.mutate({ allowedRoots })} />
      <div className="mt-4 rounded-xl border border-line bg-sunk/60 px-4 py-3">
        <div className="mb-2 flex items-center gap-2 text-ui font-medium text-fg">
          <FolderLock className="size-4 text-fg-muted" aria-hidden="true" /> Protected folders
          <span className="text-caption font-normal text-fg-muted">(always refused, cannot be changed)</span>
        </div>
        <ul className="flex flex-wrap gap-1.5">
          {h.protected_roots.map((r) => (
            <li key={r} className="rounded-sm border border-line bg-surface px-2 py-0.5 font-mono text-[12px] text-fg-2">
              {r}
            </li>
          ))}
        </ul>
      </div>

      <SectionTitle description="Switch off any tool you never want a plan to use. Steps that need a switched-off tool are refused.">Tools</SectionTitle>
      <ToolTable hybrid={h} onToggle={(id, on) => set.mutate({ disabledTools: on ? h.disabled_tools.filter((x) => x !== id) : [...h.disabled_tools, id] })} />
    </>
  );
}

function LoadStateWithTitle({ error, retry }: { error: unknown; retry: () => void }) {
  return (
    <>
      <PageTitle>AI &amp; Privacy</PageTitle>
      <LoadState error={error} retry={retry} rows={4} />
    </>
  );
}

function BrowserControl({ browser, onChange }: { browser: BrowserInfo; onChange: (v: boolean) => void }) {
  const id = useId();
  return (
    <>
      <SectionTitle description="Web tasks use a separate browser window that Relay controls. Your own browser, its tabs and logins are not touched.">Web tasks</SectionTitle>
      <Card>
        <Row
          label={
            <span className="inline-flex items-center gap-2">
              <Globe className="size-4 text-violet" aria-hidden="true" /> Browser control (Playwright)
            </span>
          }
          htmlFor={browser.available ? id : undefined}
          desc={
            browser.available ? (
              "Lets Relay open web pages, click and fill in forms to finish web tasks. It asks before anything that changes or deletes data."
            ) : (
              <span className="flex items-start gap-1.5 text-warning" data-testid="browser-unavailable">
                <TriangleAlert className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
                <span>
                  Not available on this PC{browser.detail ? `: ${browser.detail.replace(/\.$/, "")}` : ""}.
                </span>
              </span>
            )
          }
        >
          <Switch
            id={id}
            checked={browser.available && browser.enabled}
            disabled={!browser.available}
            onCheckedChange={onChange}
            aria-label="Browser control (Playwright)"
            data-testid="browser-switch"
          />
        </Row>
      </Card>
    </>
  );
}

function ShareRow({ row, checked, onChange }: { row: (typeof SHARE_ROWS)[number]; checked: boolean; onChange: (v: boolean) => void }) {
  const id = useId();
  return (
    <Row label={row.label} desc={row.desc} htmlFor={id}>
      <Switch id={id} checked={checked} onCheckedChange={onChange} />
    </Row>
  );
}

function ProviderCard({ p, isDefault }: { p: ProviderInfo; isDefault: boolean }) {
  const set = useSetHybrid();
  const problem = providerProblem(p);
  const [model, setModel] = useState(p.model ?? "");
  useEffect(() => setModel(p.model ?? ""), [p.model]);
  const modelId = useId();
  const custom = Boolean(model) && !p.caps.models.includes(model);
  const [customMode, setCustomMode] = useState(false);
  const commitModel = (v: string) => {
    const next = v.trim() || null;
    if (next !== p.model) set.mutate({ provider: { id: p.id, model: next } });
  };
  return (
    <section aria-label={`${p.label} status`} className="flex flex-col rounded-xl border border-line bg-surface p-4" data-testid={`provider-card-${p.id}`}>
      <div className="flex items-center gap-2">
        <Cloud className="size-4 text-fg-2" aria-hidden="true" />
        <h3 className="text-ui font-semibold text-fg">{p.label}</h3>
        {isDefault && <Badge tone="brand">Default</Badge>}
        <span className="flex-1" />
        {problem ? <Badge tone="warning">{problem}</Badge> : <Badge tone="success">Ready</Badge>}
      </div>
      <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-caption">
        <dt className="text-fg-muted">Installed</dt>
        <dd className="text-fg-2">{p.installed ? `Yes${p.version ? `, version ${p.version}` : ""}` : "No"}</dd>
        <dt className="text-fg-muted">Signed in</dt>
        <dd className="text-fg-2">{p.logged_in ? `Yes${p.auth_method ? ` (${p.auth_method})` : ""}` : "No"}</dd>
        <dt className="text-fg-muted">Capabilities</dt>
        <dd className="text-fg-2">
          {[p.caps.streaming_deltas ? "Live text" : "No live text", p.caps.structured_output && "Structured plans", p.caps.mcp_tools && "Tools", p.caps.session_resume && "Resume"].filter(Boolean).join(" · ")}
        </dd>
      </dl>
      {problem && (
        <div className="mt-3 rounded-md bg-warning-soft px-3 py-2.5 text-caption leading-5 text-fg" role="note">
          <p className="flex items-start gap-1.5">
            <TriangleAlert className="mt-0.5 size-3.5 shrink-0 text-warning" aria-hidden="true" />
            <span>{!p.installed ? (p.id === "claude" ? INSTALL.claude.install : "Install it from a terminal:") : "Sign in from a terminal:"}</span>
          </p>
          <ol className="mt-1.5 space-y-1 pl-5">
            {!p.installed && p.id === "codex" && (
              <li>
                <Command text={INSTALL.codex.install} />
              </li>
            )}
            <li>
              <Command text={INSTALL[p.id].login} />
            </li>
            <li className="text-fg-2">Then press Refresh.</li>
          </ol>
          {p.detail && !p.detail.toLowerCase().startsWith(problem.toLowerCase()) && <p className="mt-1 text-fg-2">{p.detail}</p>}
        </div>
      )}
      <div className="mt-4 grid gap-3 sm:grid-cols-2">
        <div className="space-y-1.5">
          <label htmlFor={modelId} className="block text-caption font-medium text-fg-2">
            Model
          </label>
          <Select
            value={customMode || custom ? "__custom" : model || "__default"}
            onValueChange={(v) => {
              if (v === "__custom") {
                setCustomMode(true);
                return;
              }
              setCustomMode(false);
              const next = v === "__default" ? "" : v;
              setModel(next);
              commitModel(next);
            }}
            disabled={!p.installed}
          >
            <SelectTrigger id={modelId} className="w-full min-w-0" aria-label={`${p.label} model`}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="__default">CLI default</SelectItem>
              {p.caps.models
                .filter((m) => m !== "default")
                .map((m) => (
                  <SelectItem key={m} value={m}>
                    {m}
                  </SelectItem>
                ))}
              <SelectItem value="__custom">Other model…</SelectItem>
            </SelectContent>
          </Select>
          {(customMode || custom) && (
            <Input
              value={model}
              autoFocus={customMode}
              onChange={(e) => setModel(e.target.value)}
              onBlur={(e) => commitModel(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              placeholder="Model name, e.g. sonnet-4"
              aria-label={`${p.label} model name`}
              className="h-8 text-caption"
              spellCheck={false}
            />
          )}
        </div>
        <div className="space-y-1.5">
          <span className="block text-caption font-medium text-fg-2" id={`${modelId}-r`}>
            Reasoning
          </span>
          {p.caps.reasoning_levels.length ? (
            <Select
              value={p.reasoning ?? "__default"}
              onValueChange={(v) => set.mutate({ provider: { id: p.id, reasoning: v === "__default" ? null : v } })}
              disabled={!p.installed}
            >
              <SelectTrigger className="w-full min-w-0" aria-label={`${p.label} reasoning level`}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="__default">CLI default</SelectItem>
                {p.caps.reasoning_levels
                  .filter((r) => r !== "default")
                  .map((r) => (
                    <SelectItem key={r} value={r}>
                      {r}
                    </SelectItem>
                  ))}
              </SelectContent>
            </Select>
          ) : (
            <p className="text-caption text-fg-muted">This provider has no reasoning levels.</p>
          )}
        </div>
      </div>
    </section>
  );
}

function Command({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <span className="inline-flex max-w-full items-center gap-1.5">
      <code className="min-w-0 rounded-xs bg-surface px-1.5 py-0.5 font-mono text-[12px] break-words text-fg">{text}</code>
      <button
        type="button"
        className="shrink-0 rounded-xs px-1 text-micro text-brand-text hover:underline"
        aria-label={`Copy ${text}`}
        onClick={async () => {
          if (await copyText(text)) {
            setCopied(true);
            setTimeout(() => setCopied(false), 1400);
          }
        }}
      >
        {copied ? "Copied" : "Copy"}
      </button>
    </span>
  );
}

function AllowedFolders({ roots, onChange }: { roots: string[]; onChange: (r: string[]) => void }) {
  const [value, setValue] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const inputId = useId();
  const add = () => {
    const v = value.trim().replace(/[\\/]+$/, "");
    if (!v) return;
    if (!/^([a-zA-Z]:\\|\/|~\/|\\\\)/.test(v)) return setErr("Type a full path, for example C:\\Users\\you\\Documents.");
    if (roots.some((r) => r.toLowerCase() === v.toLowerCase())) return setErr("That folder is already allowed.");
    onChange([...roots, v]);
    setValue("");
    setErr(null);
  };
  return (
    <Card>
      {roots.length === 0 && <p className="py-3.5 text-caption text-fg-muted">No folders yet: plans cannot touch any files.</p>}
      {roots.map((r) => (
        <div key={r} className="flex items-center gap-3 py-2.5">
          <FolderOpen className="size-4 shrink-0 text-fg-muted" aria-hidden="true" />
          <span className="min-w-0 flex-1 truncate font-mono text-[12.5px] text-fg" title={r}>
            {r}
          </span>
          <IconButton label={`Remove ${r}`} size="icon-sm" variant="danger-ghost" onClick={() => onChange(roots.filter((x) => x !== r))}>
            <Trash2 aria-hidden="true" />
          </IconButton>
        </div>
      ))}
      <form
        className="flex flex-col gap-1.5 py-3"
        onSubmit={(e) => {
          e.preventDefault();
          add();
        }}
      >
        <label htmlFor={inputId} className="sr-only">
          Folder to allow
        </label>
        <div className="flex gap-2">
          <Input
            id={inputId}
            value={value}
            onChange={(e) => {
              setValue(e.target.value);
              setErr(null);
            }}
            placeholder="C:\Users\you\Projects"
            aria-invalid={Boolean(err)}
            aria-describedby={err ? `${inputId}-err` : undefined}
            className="font-mono text-[12.5px]"
            spellCheck={false}
          />
          <Button type="submit" variant="secondary" disabled={!value.trim()}>
            <Plus aria-hidden="true" /> Add
          </Button>
        </div>
        {err && (
          <p id={`${inputId}-err`} role="alert" className="text-caption text-danger">
            {err}
          </p>
        )}
      </form>
    </Card>
  );
}

function ToolTable({ hybrid, onToggle }: { hybrid: Hybrid; onToggle: (id: string, on: boolean) => void }) {
  return (
    <div className="overflow-hidden rounded-xl border border-line bg-surface">
      <table className="w-full text-ui">
        <caption className="sr-only">Tools a plan may use</caption>
        <thead>
          <tr className="border-b border-line bg-sunk/60 text-left text-caption text-fg-muted">
            <th scope="col" className="px-4 py-2 font-medium">
              Tool
            </th>
            <th scope="col" className="w-20 px-4 py-2 text-right font-medium">
              Allowed
            </th>
          </tr>
        </thead>
        <tbody>
          {hybrid.tools.map((t) => {
            const on = !hybrid.disabled_tools.includes(t.id);
            return (
              <tr key={t.id} className="border-b border-line last:border-0">
                <th scope="row" className="px-4 py-2.5 text-left font-normal">
                  <span className="text-fg">{t.label}</span>
                  <span className="ml-2 font-mono text-[11.5px] text-fg-muted">{t.id}</span>
                  {t.risk === "destructive" && (
                    <Badge tone="danger" className="ml-2 align-[1px]" title={DESTRUCTIVE_BADGE.description}>
                      <TriangleAlert aria-hidden="true" />
                      {DESTRUCTIVE_BADGE.label}
                    </Badge>
                  )}
                </th>
                <td className="px-4 py-2.5 text-right">
                  <Switch checked={on} onCheckedChange={(v) => onToggle(t.id, v)} aria-label={`Allow ${t.label}`} />
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
