// Settings › Relay Local AI: managed local engine setup progress, models, GPU and downloads.
// Third-party engine names are never shown.
import { useEffect, useState } from "react";
import { Cpu, Download, RefreshCw, Sparkles } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { useEngines, useSetLocalModel } from "@/hooks/queries";
import { useApp } from "@/store/app";
import { api, errorText } from "@/lib/backend";
import { qk } from "@/lib/query";
import { formatBytes } from "@/lib/grouping";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ProgressRow } from "@/components/ui/progress";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { ErrorNote } from "@/components/common";
import { LoadState, PageTitle, SectionTitle } from "./parts";

export function LocalAiSection() {
  const engines = useEngines();
  const qc = useQueryClient();
  const progress = useApp((s) => s.progress["local-ai"]);
  const toast = useApp((s) => s.toast);
  const setModel = useSetLocalModel();
  const [starting, setStarting] = useState(false);
  const [pull, setPull] = useState("");

  useEffect(() => {
    if (progress) setStarting(false);
  }, [progress]);

  const header = (
    <PageTitle
      description="Relay's own AI runs privately on this PC. It plans everyday tasks without anything leaving the machine."
      action={
        <Button size="sm" variant="ghost" onClick={() => void engines.refetch()} disabled={engines.isFetching} aria-label="Check the local AI again">
          <RefreshCw className={cn(engines.isFetching && "animate-spin")} aria-hidden="true" /> Check again
        </Button>
      }
    >
      Relay Local AI
    </PageTitle>
  );

  if (!engines.data)
    return (
      <>
        {header}
        <LoadState error={engines.error} retry={() => void engines.refetch()} />
      </>
    );

  const { local, setup } = engines.data;
  const live = progress && !progress.done ? progress : null;
  const running = Boolean(live) || setup.state === "running" || starting;
  const error = (progress?.done && progress.error && progress.error !== "Cancelled" ? progress.error : null) || (setup.state === "failed" ? setup.error : null);
  const ready = local.available;

  const start = async () => {
    setStarting(true);
    try {
      await api.localAiSetup("start");
    } catch (e) {
      setStarting(false);
      toast(`Couldn't start the setup: ${errorText(e)}`, "error");
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
      toast(`Couldn't download ${m}: ${errorText(e)}`, "error");
    }
  };

  return (
    <>
      {header}
      <section className="rounded-xl border border-line bg-surface p-5" aria-label="Relay Local AI status" data-testid="local-ai-card">
        <div className="flex flex-wrap items-center gap-3">
          <span className="inline-flex size-10 items-center justify-center rounded-lg bg-brand-soft text-brand-text">
            <Cpu className="size-5" aria-hidden="true" />
          </span>
          <div className="min-w-0 flex-1">
            <div className="text-ui font-semibold">Relay Local AI</div>
            <div className="text-caption text-fg-muted">
              {local.gpu ? `${local.gpu.name} · ${(local.gpu.vram_mb / 1024).toFixed(local.gpu.vram_mb >= 10240 ? 0 : 1)} GB graphics memory` : "No dedicated graphics card found; runs on the processor"}
            </div>
          </div>
          {running ? <Badge tone="brand">Setting up</Badge> : ready ? <Badge tone="success">Ready</Badge> : error ? <Badge tone="danger">Setup failed</Badge> : <Badge>Not set up</Badge>}
        </div>

        {running && (
          <div className="mt-5" aria-live="polite">
            <ProgressRow percent={live?.percent ?? setup.percent ?? null} label={live?.label || setup.stage || "Starting…"} />
            <div className="mt-3 flex justify-end">
              <Button size="sm" variant="ghost" onClick={cancel}>
                Cancel
              </Button>
            </div>
          </div>
        )}

        {!running && error && (
          <div className="mt-4">
            <ErrorNote title="Setup didn't finish">{error}</ErrorNote>
          </div>
        )}

        {!running && !ready && (
          <div className="mt-5 rounded-lg bg-sunk px-4 py-4">
            <p className="text-ui leading-6 text-fg-2">
              Relay downloads everything it needs and picks a model that suits this PC
              {local.recommended ? (
                <>
                  {" "}
                  (<span className="font-medium text-fg">{local.recommended}</span>)
                </>
              ) : null}
              . It takes a few minutes and several gigabytes.
            </p>
            <Button variant="primary" size="lg" className="mt-3.5" onClick={() => void start()}>
              <Sparkles aria-hidden="true" /> {error ? "Try again" : "Set up Relay Local AI"}
            </Button>
          </div>
        )}
      </section>

      {ready && (
        <>
          <SectionTitle id="local-model">Model</SectionTitle>
          <RadioGroup
            aria-labelledby="local-model"
            value={local.selected ?? "__auto"}
            onValueChange={(v) => setModel.mutate(v === "__auto" ? "" : v, { onSettled: () => void qc.invalidateQueries({ queryKey: qk.engines }) })}
            className="gap-0 divide-y divide-line rounded-xl border border-line bg-surface"
          >
            <ModelRow value="__auto" title="Automatic" meta={`Relay picks${local.active ? ` · now ${local.active.name}` : local.recommended ? ` · recommended ${local.recommended}` : ""}`} />
            {local.models.map((m) => (
              <ModelRow
                key={m.name}
                value={m.name}
                title={m.name}
                meta={formatBytes(m.size_bytes)}
                tags={[m.tools && "Tools", m.vision && "Vision", m.name === local.recommended && "Recommended"].filter((x): x is string => Boolean(x))}
              />
            ))}
          </RadioGroup>
          <form
            className="mt-4 flex gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              void download();
            }}
          >
            <Input value={pull} onChange={(e) => setPull(e.target.value)} placeholder={`Download another model, e.g. ${local.recommended || "qwen3:14b"}`} aria-label="Model to download" spellCheck={false} />
            <Button type="submit" variant="secondary" disabled={!pull.trim() || running}>
              <Download aria-hidden="true" /> Download
            </Button>
          </form>
        </>
      )}
    </>
  );
}

function ModelRow({ value, title, meta, tags }: { value: string; title: string; meta: string; tags?: string[] }) {
  return (
    <label className="flex cursor-default items-center gap-3 px-4 py-3 hover:bg-hover">
      <RadioGroupItem value={value} aria-label={title} />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-ui">{title}</span>
        <span className="block text-caption text-fg-muted">{meta}</span>
      </span>
      {tags?.map((t) => (
        <Badge key={t} tone={t === "Recommended" ? "brand" : "neutral"}>
          {t}
        </Badge>
      ))}
    </label>
  );
}
