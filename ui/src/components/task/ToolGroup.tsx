// Agent mode (cloud_agent): the provider drives tools step by step. Shown as a compact,
// collapsible "Worked for 12s · 7 steps" group; raw tool names stay inside the disclosure.
import { memo, useState } from "react";
import { ChevronRight, CircleCheck, CircleX, OctagonX, ShieldCheck, ShieldX, Terminal } from "lucide-react";
import type { ToolsItem, ToolStep } from "@/lib/chat";
import { toolSteps } from "@/lib/chat";
import { formatDuration } from "@/lib/grouping";
import { cn } from "@/lib/utils";
import { useNow } from "@/hooks/dom";

function ToolIcon({ step }: { step: ToolStep }) {
  if (step.kind === "confirm") {
    if (step.state === "approved") return <ShieldCheck className="size-4 text-success" aria-hidden="true" />;
    if (step.state === "declined") return <ShieldX className="size-4 text-danger" aria-hidden="true" />;
    return <span className="pulse-dot !size-2" aria-hidden="true" />;
  }
  if (step.kind === "note") return <span className="mx-[5px] block size-1.5 rounded-full bg-fg-muted/60" aria-hidden="true" />;
  switch (step.status) {
    case "running":
      return <span className="pulse-dot !size-2" aria-hidden="true" />;
    case "ok":
      return <CircleCheck className="size-4 text-fg-muted" aria-hidden="true" />;
    case "failed":
      return <CircleX className="size-4 text-danger" aria-hidden="true" />;
    default:
      return <OctagonX className="size-4 text-fg-muted" aria-hidden="true" />;
  }
}

export const ToolGroup = memo(function ToolGroup({ item }: { item: ToolsItem }) {
  const [open, setOpen] = useState(false);
  const running = item.finished_at === null;
  const now = useNow(running, 1000);
  const n = toolSteps(item);
  const elapsed = (item.finished_at ?? now) - item.started_at;
  const failed = item.steps.some((s) => s.kind === "tool" && s.status === "failed");
  const current = [...item.steps].reverse().find((s) => s.kind === "tool" || s.kind === "confirm");
  const stepsLabel = `${n} ${n === 1 ? "step" : "steps"}`;
  const id = `tools-${item.id}`;

  return (
    <div className="turn py-1.5" data-testid="tool-group">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        aria-controls={id}
        className="-ml-1.5 flex max-w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-left text-ui text-fg-2 transition-colors hover:bg-hover hover:text-fg"
      >
        {running ? (
          <>
            <span className="shimmer-text font-medium">Working</span>
            <span className="text-fg-muted">
              · {formatDuration(elapsed)}
              {n > 0 && ` · ${stepsLabel}`}
            </span>
          </>
        ) : (
          <span>
            {item.outcome === "cancelled" ? "Stopped after" : "Worked for"} {formatDuration(elapsed)}
            <span className="text-fg-muted"> · {stepsLabel}</span>
            {failed && <span className="text-danger"> · a step failed</span>}
          </span>
        )}
        <ChevronRight aria-hidden="true" className={cn("size-4 shrink-0 text-fg-muted transition-transform duration-(--relay-dur-fast)", open && "rotate-90")} />
      </button>
      {running && !open && current && (
        <div className="mt-0.5 truncate pl-0.5 text-caption text-fg-muted">
          {current.kind === "confirm" ? `Waiting for your approval: ${current.title}` : current.kind === "tool" ? current.summary || current.name : ""}
        </div>
      )}
      {open && (
        <ol id={id} className="rise-in relative mt-2 ml-[7px] space-y-0.5 border-l border-line pb-1 pl-5" aria-label="Tool steps">
          {item.steps.map((s) => (
            <li key={s.id} className="relative py-1.5">
              <span className="absolute top-[9px] -left-[29px] flex size-[18px] items-center justify-center bg-canvas">
                <ToolIcon step={s} />
              </span>
              {s.kind === "note" ? (
                <p className="text-ui leading-6 whitespace-pre-wrap text-fg-2">{s.text.trim()}</p>
              ) : s.kind === "confirm" ? (
                <div className="text-ui leading-6">
                  <span>{s.title}</span>
                  <span className="ml-2 text-caption text-fg-muted">{s.state === "pending" ? "Waiting for approval" : s.state === "approved" ? "Approved" : "Declined"}</span>
                  {s.detail && <div className="text-caption text-fg-muted">{s.detail}</div>}
                </div>
              ) : (
                <div className="text-ui leading-6">
                  <span className="text-fg">{s.summary || s.name}</span>
                  <span className="ml-2 inline-flex items-center gap-1 align-middle font-mono text-[11.5px] text-fg-muted">
                    <Terminal className="size-3" aria-hidden="true" />
                    {s.name}
                  </span>
                  {s.result && <div className={cn("text-caption", s.status === "failed" ? "text-danger" : "text-fg-muted")}>{s.result}</div>}
                </div>
              )}
            </li>
          ))}
          {item.steps.length === 0 && <li className="py-1.5 text-caption text-fg-muted">Getting started…</li>}
        </ol>
      )}
    </div>
  );
});
