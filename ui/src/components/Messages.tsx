import { lazy, memo, Suspense, useState } from "react";
import {
  Ban,
  Check,
  ChevronRight,
  CircleCheck,
  CircleX,
  Copy,
  Cpu,
  Cloud,
  FileText,
  RotateCcw,
  ShieldCheck,
  ShieldX,
  TriangleAlert,
  Terminal,
  Settings2,
} from "lucide-react";
import type { AssistantItem, ErrorItem, NoticeItem, ToolsItem, ToolStep, UserItem } from "../lib/chat";
import { toolSteps } from "../lib/chat";
import { formatDuration } from "../lib/grouping";
import { useApp } from "../store/app";

// Markdown (and its parser) is loaded on demand so the first paint stays small.
const loadMarkdown = () => import("./Markdown");
const LazyMarkdown = lazy(() => loadMarkdown().then((m) => ({ default: m.Markdown })));
if (typeof window !== "undefined") {
  const idle = (window as Window & { requestIdleCallback?: (cb: () => void) => void }).requestIdleCallback ?? ((cb: () => void) => setTimeout(cb, 600));
  idle(() => void loadMarkdown());
}
function Markdown({ text }: { text: string }) {
  return (
    <Suspense fallback={<div className="md whitespace-pre-wrap">{text}</div>}>
      <LazyMarkdown text={text} />
    </Suspense>
  );
}
import { copyText, useNow } from "./hooks";
import { Button, cn, Spinner, Tip } from "./ui";

const baseName = (p: string) => p.split(/[\\/]/).pop() || p;

export const UserMessage = memo(function UserMessage({ item }: { item: UserItem }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="group turn flex flex-col items-end gap-1.5 py-3">
      {item.attachments.length > 0 && (
        <ul className="flex max-w-[80%] flex-wrap justify-end gap-2" aria-label="Attachments">
          {item.attachments.map((f) => (
            <li key={f} className="flex items-center gap-2 rounded-xl border border-line px-3 py-2 text-[13px]" title={f}>
              <FileText size={15} className="text-accent" aria-hidden="true" />
              <span className="max-w-[200px] truncate">{baseName(f)}</span>
            </li>
          ))}
        </ul>
      )}
      <div className="max-w-[min(80%,560px)] rounded-[22px] bg-surface px-4 py-2.5 text-[15.5px] leading-[1.6] break-words whitespace-pre-wrap">
        {item.text}
      </div>
      <div className="flex h-7 items-center opacity-0 transition-opacity duration-150 group-hover:opacity-100 focus-within:opacity-100">
        <Tip label={copied ? "Copied" : "Copy"}>
          <button
            type="button"
            aria-label="Copy message"
            onClick={async () => {
              if (await copyText(item.text)) {
                setCopied(true);
                setTimeout(() => setCopied(false), 1500);
              }
            }}
            className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted hover:bg-hover hover:text-fg"
          >
            {copied ? <Check size={15} /> : <Copy size={15} />}
          </button>
        </Tip>
      </div>
    </div>
  );
});

export const AssistantMessage = memo(function AssistantMessage({
  item,
  isLast,
  convId,
  canRetry,
}: {
  item: AssistantItem;
  isLast: boolean;
  convId: string;
  canRetry: boolean;
}) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="group turn py-3" aria-busy={item.streaming}>
      <Markdown text={item.text} />
      {item.streaming ? (
        <span className="pulse-dot mt-1" aria-hidden="true" />
      ) : (
        <div className={cn("-ml-1.5 mt-1.5 flex h-8 items-center gap-0.5 transition-opacity duration-150", isLast ? "opacity-100" : "opacity-0 group-hover:opacity-100 focus-within:opacity-100")}>
          <Tip label={copied ? "Copied" : "Copy"}>
            <button
              type="button"
              aria-label="Copy response"
              onClick={async () => {
                if (await copyText(item.text)) {
                  setCopied(true);
                  setTimeout(() => setCopied(false), 1500);
                }
              }}
              className="inline-flex h-8 w-8 items-center justify-center rounded-lg text-muted hover:bg-hover hover:text-fg"
            >
              {copied ? <Check size={16} /> : <Copy size={16} />}
            </button>
          </Tip>
          {isLast && canRetry && (
            <Tip label="Try again">
              <button
                type="button"
                aria-label="Try again"
                onClick={() => void useApp.getState().retry(convId)}
                className="inline-flex h-8 w-8 items-center justify-center rounded-lg text-muted hover:bg-hover hover:text-fg"
              >
                <RotateCcw size={16} />
              </button>
            </Tip>
          )}
          {item.engine && (
            <span className="ml-1.5 inline-flex items-center gap-1 text-[12px] text-subtle">
              {item.engine.kind === "local" ? <Cpu size={13} /> : <Cloud size={13} />}
              {item.engine.kind === "local" ? `Local AI · ${item.engine.name}` : item.engine.name}
            </span>
          )}
        </div>
      )}
    </div>
  );
});

/** Shown while a run is starting or between steps, before any text arrives. */
export function Thinking() {
  return (
    <div className="py-3" aria-label="Relay is working">
      <span className="pulse-dot" />
    </div>
  );
}

function StepIcon({ step }: { step: ToolStep }) {
  if (step.kind === "confirm") {
    if (step.state === "approved") return <ShieldCheck size={16} className="text-success" />;
    if (step.state === "declined") return <ShieldX size={16} className="text-danger" />;
    return <Spinner size={15} className="text-muted" />;
  }
  if (step.kind === "note") return <span className="mx-[5px] block h-1.5 w-1.5 rounded-full bg-subtle" />;
  switch (step.status) {
    case "running":
      return <Spinner size={15} className="text-muted" />;
    case "ok":
      return <CircleCheck size={16} className="text-success" />;
    case "failed":
      return <CircleX size={16} className="text-danger" />;
    default:
      return <Ban size={15} className="text-subtle" />;
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
  const stepsLabel = `${n} step${n === 1 ? "" : "s"}`;

  return (
    <div className="turn py-2">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        className="group/tg flex max-w-full items-center gap-1.5 rounded-md py-1 text-left text-[14.5px] text-muted hover:text-fg"
      >
        {running ? (
          <>
            <span className="shimmer font-medium">Working</span>
            <span className="text-subtle">
              · {formatDuration(elapsed)}
              {n > 0 && ` · ${stepsLabel}`}
            </span>
          </>
        ) : (
          <span>
            {item.outcome === "cancelled" ? "Stopped after" : "Worked for"} {formatDuration(elapsed)}
            <span className="text-subtle"> · {stepsLabel}</span>
            {failed && <span className="text-subtle"> · some steps failed</span>}
          </span>
        )}
        <ChevronRight size={16} className={cn("shrink-0 text-subtle transition-transform duration-150", open && "rotate-90")} />
      </button>
      {running && !open && current && (
        <div className="mt-0.5 truncate pl-0.5 text-[13.5px] text-subtle" aria-live="off">
          {current.kind === "confirm" ? `Waiting for your approval: ${current.title}` : current.summary || current.name}
        </div>
      )}
      {open && (
          <ol
            className="rise-in relative mt-2 ml-[7px] space-y-0.5 border-l border-line pb-1 pl-5"
          >
            {item.steps.map((s) => (
              <li key={s.id} className="relative py-1.5">
                <span className="absolute top-[9px] -left-[29px] flex h-[18px] w-[18px] items-center justify-center bg-bg">
                  <StepIcon step={s} />
                </span>
                {s.kind === "note" ? (
                  <p className="text-[14px] leading-6 text-muted whitespace-pre-wrap">{s.text.trim()}</p>
                ) : s.kind === "confirm" ? (
                  <div className="text-[14px] leading-6">
                    <span>{s.title}</span>
                    <span className="ml-2 text-[12.5px] text-subtle">
                      {s.state === "pending" ? "Waiting for approval" : s.state === "approved" ? "Approved" : "Declined"}
                    </span>
                    {s.detail && <div className="text-[13px] text-subtle">{s.detail}</div>}
                  </div>
                ) : (
                  <div className="text-[14px] leading-6">
                    <span className="text-fg">{s.summary || s.name}</span>
                    <span className="ml-2 inline-flex items-center gap-1 align-middle font-mono text-[11.5px] text-subtle">
                      <Terminal size={11} aria-hidden="true" />
                      {s.name}
                    </span>
                    {s.result && <div className={cn("text-[13px]", s.status === "failed" ? "text-danger" : "text-subtle")}>{s.result}</div>}
                  </div>
                )}
              </li>
            ))}
            {item.steps.length === 0 && <li className="py-1.5 text-[13.5px] text-subtle">Getting started…</li>}
          </ol>
      )}
    </div>
  );
});

export const Notice = memo(function Notice({ item }: { item: NoticeItem }) {
  return <div className="turn py-2 text-center text-[13px] text-subtle">{item.text}</div>;
});

const ERROR_HELP: Record<string, { title: string; help: string }> = {
  not_logged_in: {
    title: "Claude isn't signed in",
    help: "Open a terminal, run `claude auth login`, finish signing in, then try again.",
  },
  cli_not_installed: {
    title: "Claude isn't installed",
    help: "Relay needs the Claude CLI for this task. See Settings › AI engines for how to set it up, or switch to Local only.",
  },
  timeout: { title: "The task took too long", help: "It was stopped after running without progress. You can try again." },
  cli: { title: "Claude ran into a problem", help: "" },
  internal: { title: "Something went wrong", help: "" },
};

export const ErrorBanner = memo(function ErrorBanner({ item, convId, isLast }: { item: ErrorItem; convId: string; isLast: boolean }) {
  if (item.error === "cancelled") return <div className="turn py-2 text-center text-[13px] text-subtle">Stopped.</div>;
  const meta = ERROR_HELP[item.error] ?? ERROR_HELP.internal!;
  const parts = meta.help.split("`");
  return (
    <div role="alert" className="turn my-2 flex gap-3 rounded-2xl border border-[color-mix(in_srgb,var(--danger)_30%,transparent)] bg-danger-soft px-4 py-3">
      <TriangleAlert size={18} className="mt-0.5 shrink-0 text-danger" aria-hidden="true" />
      <div className="min-w-0 flex-1 text-[14px] leading-[1.55]">
        <div className="font-medium">{meta.title}</div>
        {item.message && <div className="text-muted break-words">{item.message}</div>}
        {meta.help && (
          <div className="mt-1 text-muted">
            {parts.map((p, i) => (i % 2 ? <code key={i} className="rounded bg-hover px-1.5 py-0.5 font-mono text-[12.5px] text-fg">{p}</code> : <span key={i}>{p}</span>))}
          </div>
        )}
        <div className="mt-2.5 flex flex-wrap gap-2">
          {isLast && (
            <Button size="sm" variant="secondary" onClick={() => void useApp.getState().retry(convId)}>
              <RotateCcw size={14} /> Try again
            </Button>
          )}
          {(item.error === "not_logged_in" || item.error === "cli_not_installed") && (
            <Button size="sm" variant="ghost" onClick={() => useApp.getState().openSettings("engines")}>
              <Settings2 size={14} /> AI engines
            </Button>
          )}
        </div>
      </div>
    </div>
  );
});
