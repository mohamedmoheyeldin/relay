import { useEffect, useState } from "react";
import { Check, Cloud, Cpu, Download, RefreshCw, Route, Sparkles } from "lucide-react";
import { useApp } from "../store/app";
import { api } from "../lib/backend";
import { formatBytes } from "../lib/grouping";
import type { RouteMode } from "../lib/types";
import { Button, cn, Spinner } from "../components/ui";
import { fieldClass } from "../components/Dialogs";
import { Card, PageTitle, Pill, ProgressBar, Row, SubTitle } from "./parts";

const ROUTES: { id: RouteMode; title: string; desc: string; icon: typeof Route }[] = [
  { id: "auto", title: "Auto", desc: "Relay Local AI handles PC chores; Claude takes web, coding, writing and anything hard.", icon: Route },
  { id: "local_only", title: "Local only", desc: "Everything stays on this PC. Works offline; best for simple tasks.", icon: Cpu },
  { id: "cloud_only", title: "Claude only", desc: "Every task goes to Claude. Most capable, needs internet.", icon: Cloud },
];

export function EnginesSection() {
  const engines = useApp((s) => s.engines);
  const enginesError = useApp((s) => s.enginesError);
  const status = useApp((s) => s.status);
  const { loadEngines, setRouteMode } = useApp.getState();
  const mode = engines?.route_mode ?? status?.route_mode;
  const [refreshing, setRefreshing] = useState(false);

  useEffect(() => {
    void loadEngines();
  }, [loadEngines]);

  const refresh = async () => {
    setRefreshing(true);
    await loadEngines();
    setRefreshing(false);
  };

  return (
    <>
      <div className="flex items-start justify-between gap-4">
        <PageTitle>AI engines</PageTitle>
        <Button size="sm" variant="ghost" onClick={() => void refresh()} disabled={refreshing} className="mt-1">
          {refreshing ? <Spinner size={14} /> : <RefreshCw size={14} />} Check again
        </Button>
      </div>

      <SubTitle>Routing</SubTitle>
      <div className="grid gap-3 sm:grid-cols-3" role="radiogroup" aria-label="Routing">
        {ROUTES.map((r) => {
          const on = mode === r.id;
          return (
            <button
              key={r.id}
              type="button"
              role="radio"
              aria-checked={on}
              onClick={() => setRouteMode(r.id)}
              className={cn(
                "relative flex flex-col items-start gap-1.5 rounded-2xl border p-4 text-left transition-colors duration-150",
                on ? "border-accent bg-accent-soft" : "border-line hover:bg-hover",
              )}
            >
              <r.icon size={18} className={on ? "text-accent" : "text-muted"} />
              <span className="text-[14.5px] font-medium">{r.title}</span>
              <span className="text-[12.5px] leading-[18px] text-subtle">{r.desc}</span>
              {on && <Check size={16} className="absolute top-3.5 right-3.5 text-accent" />}
            </button>
          );
        })}
      </div>

      {enginesError && !engines && (
        <div className="mt-6 rounded-2xl bg-danger-soft px-4 py-3 text-[14px]">Couldn't read the engines: {enginesError}</div>
      )}

      <SubTitle>Relay Local AI</SubTitle>
      {engines ? <LocalCard /> : !enginesError && <div className="skeleton h-40 rounded-2xl" />}

      <SubTitle>Claude</SubTitle>
      {engines ? <ClaudeCard /> : !enginesError && <div className="skeleton h-28 rounded-2xl" />}
    </>
  );
}

function LocalCard() {
  const engines = useApp((s) => s.engines)!;
  const progress = useApp((s) => s.progress["local-ai"]);
  const { setLocalModel, toast, loadEngines } = useApp.getState();
  const [starting, setStarting] = useState(false);
  const [pull, setPull] = useState("");
  const local = engines.local;
  const setup = engines.setup;

  const live = progress && !progress.done ? progress : null;
  const running = Boolean(live) || setup.state === "running" || starting;
  const error = (progress?.done && progress.error) || (setup.state === "failed" ? setup.error : null);
  const ready = local.available;

  useEffect(() => {
    if (progress) setStarting(false);
  }, [progress]);

  const start = async () => {
    setStarting(true);
    try {
      await api.localAiSetup("start");
    } catch (e) {
      setStarting(false);
      toast(`Couldn't start the setup: ${e instanceof Error ? e.message : e}`, "error");
    }
  };
  const cancel = () => api.localAiSetup("cancel").catch(() => undefined);
  const download = async () => {
    const m = pull.trim();
    if (!m) return;
    setStarting(true);
    try {
      await api.pullModel(m);
      setPull("");
    } catch (e) {
      setStarting(false);
      toast(`Couldn't download ${m}: ${e instanceof Error ? e.message : e}`, "error");
    }
  };

  return (
    <div className="rounded-2xl border border-line p-4 sm:p-5">
      <div className="flex flex-wrap items-center gap-3">
        <span className="inline-flex h-10 w-10 items-center justify-center rounded-xl bg-accent-soft text-accent">
          <Cpu size={20} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="text-[15px] font-medium">Relay Local AI</div>
          <div className="text-[13px] text-subtle">
            {local.gpu ? `${local.gpu.name} · ${(local.gpu.vram_mb / 1024).toFixed(local.gpu.vram_mb >= 10240 ? 0 : 1)} GB VRAM` : "No dedicated GPU found; runs on the processor"}
          </div>
        </div>
        {running ? <Pill tone="accent">Setting up</Pill> : ready ? <Pill tone="ok">Ready</Pill> : error ? <Pill tone="bad">Setup failed</Pill> : <Pill>Not set up</Pill>}
      </div>

      {running && (
        <div className="mt-5">
          <ProgressBar percent={live?.percent ?? setup.percent ?? null} label={live?.label || setup.stage || "Starting…"} />
          <div className="mt-3 flex justify-end">
            <Button size="sm" variant="ghost" onClick={cancel}>
              Cancel
            </Button>
          </div>
        </div>
      )}

      {!running && error && <p className="mt-4 rounded-xl bg-danger-soft px-3 py-2 text-[13.5px]">{error}</p>}

      {!running && !ready && (
        <div className="mt-5 rounded-xl bg-hover px-4 py-4">
          <p className="text-[14px] leading-6">
            Run simple PC tasks right here, privately and offline. Relay downloads everything it needs and picks a model that suits this PC
            {local.recommended ? (
              <>
                {" "}
                (<span className="font-medium">{local.recommended}</span>)
              </>
            ) : null}
            .
          </p>
          <Button variant="primary" className="mt-3.5 h-10 px-5" onClick={() => void start()}>
            <Sparkles size={16} /> {error ? "Try again" : "Set up Relay Local AI"}
          </Button>
        </div>
      )}

      {ready && (
        <>
          <div className="mt-5 mb-2 text-[13px] font-medium text-muted">Model</div>
          <div className="divide-y divide-[var(--border)] rounded-xl border border-line" role="radiogroup" aria-label="Local model">
            <ModelRow
              selected={!local.selected}
              onSelect={() => setLocalModel("")}
              title="Automatic"
              meta={`Relay picks${local.active ? ` · now ${local.active.name}` : local.recommended ? ` · recommended ${local.recommended}` : ""}`}
            />
            {local.models.map((m) => (
              <ModelRow
                key={m.name}
                selected={local.selected === m.name}
                onSelect={() => setLocalModel(m.name)}
                title={m.name}
                meta={formatBytes(m.size_bytes)}
                tags={[m.tools && "Tools", m.vision && "Vision", m.name === local.recommended && "Recommended"].filter(Boolean) as string[]}
              />
            ))}
          </div>
          <form
            className="mt-4 flex gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              void download();
            }}
          >
            <input
              value={pull}
              onChange={(e) => setPull(e.target.value)}
              placeholder={`Download another model, e.g. ${local.recommended || "qwen3:14b"}`}
              aria-label="Model to download"
              className={cn(fieldClass, "h-9 flex-1 rounded-full py-0")}
              spellCheck={false}
            />
            <Button type="submit" size="md" variant="secondary" disabled={!pull.trim()}>
              <Download size={15} /> Download
            </Button>
          </form>
        </>
      )}
      {!running && (
        <button type="button" className="mt-4 text-[12.5px] text-subtle underline-offset-2 hover:underline" onClick={() => void loadEngines()}>
          Refresh status
        </button>
      )}
    </div>
  );
}

function ModelRow({ selected, onSelect, title, meta, tags }: { selected: boolean; onSelect: () => void; title: string; meta: string; tags?: string[] }) {
  return (
    <button type="button" role="radio" aria-checked={selected} onClick={onSelect} className="flex w-full items-center gap-3 px-3.5 py-3 text-left hover:bg-hover">
      <span className={cn("flex h-[18px] w-[18px] shrink-0 items-center justify-center rounded-full border-2", selected ? "border-accent" : "border-line-strong")}>
        {selected && <span className="h-2 w-2 rounded-full bg-accent" />}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[14px]">{title}</span>
        <span className="block text-[12.5px] text-subtle">{meta}</span>
      </span>
      {tags?.map((t) => (
        <Pill key={t} tone={t === "Recommended" ? "accent" : "neutral"}>
          {t}
        </Pill>
      ))}
    </button>
  );
}

function ClaudeCard() {
  const cloud = useApp((s) => s.engines!.cloud);
  return (
    <Card>
      <Row label="Installed" desc={cloud.installed ? <span className="break-all font-mono text-[12px]">{cloud.path}</span> : "The Claude CLI was not found on this PC."}>
        {cloud.installed ? <Pill tone="ok">Installed</Pill> : <Pill tone="bad">Not installed</Pill>}
      </Row>
      <Row
        label="Signed in"
        desc={
          cloud.logged_in ? (
            cloud.detail || "Ready to take tasks."
          ) : (
            <>
              Open a terminal and run <code className="rounded bg-hover px-1.5 py-0.5 font-mono text-[12px] text-fg">claude auth login</code>, then press Check again.
            </>
          )
        }
      >
        {cloud.logged_in ? <Pill tone="ok">Signed in</Pill> : <Pill tone="bad">Not signed in</Pill>}
      </Row>
      {!cloud.installed && (
        <Row
          label="How to install"
          desc={
            <>
              Install Claude Code from its official site, then run <code className="rounded bg-hover px-1.5 py-0.5 font-mono text-[12px] text-fg">claude auth login</code>. Until then, use Local only.
            </>
          }
        />
      )}
    </Card>
  );
}
