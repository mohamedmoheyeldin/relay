import { lazy, memo, Suspense, useState } from "react";
import { Check, Copy, FileText, RotateCcw, Settings2, TriangleAlert, UserRound } from "lucide-react";
import type { AssistantItem, ErrorItem, NoticeItem, UsedTag, UserItem } from "@/lib/chat";
import { UsedChips } from "@/components/task/UsedChips";
import { PrivacyLedger } from "@/components/task/PrivacyLedger";
import { useApp } from "@/store/app";
import { copyText } from "@/hooks/dom";
import { cn } from "@/lib/utils";
import { go, paths } from "@/lib/nav";
import { Button } from "@/components/ui/button";
import { IconButton } from "@/components/ui/icon-button";
import { Code } from "@/components/common";

// Markdown (and its parser) is loaded on demand so the first paint stays small; prefetched when idle.
const loadMarkdown = () => import("./Markdown");
const LazyMarkdown = lazy(() => loadMarkdown().then((m) => ({ default: m.Markdown })));
if (typeof window !== "undefined") {
  const idle = (window as Window & { requestIdleCallback?: (cb: () => void) => void }).requestIdleCallback ?? ((cb: () => void) => setTimeout(cb, 800));
  idle(() => void loadMarkdown());
}
function Markdown({ text, streaming }: { text: string; streaming?: boolean }) {
  return (
    <Suspense fallback={<div className="md whitespace-pre-wrap">{text}</div>}>
      <LazyMarkdown text={text} streaming={streaming} />
    </Suspense>
  );
}

const baseName = (p: string) => p.split(/[\\/]/).pop() || p;

function CopyButton({ text, label }: { text: string; label: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <IconButton
      label={copied ? "Copied" : label}
      size="icon-sm"
      onClick={async () => {
        if (await copyText(text)) {
          setCopied(true);
          setTimeout(() => setCopied(false), 1500);
        }
      }}
    >
      {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
    </IconButton>
  );
}

export const UserMessage = memo(function UserMessage({ item }: { item: UserItem }) {
  return (
    <div className="group turn flex flex-col items-end gap-1.5 pt-5 pb-1">
      {item.attachments.length > 0 && (
        <ul className="flex max-w-[85%] flex-wrap justify-end gap-2" aria-label="Attachments">
          {item.attachments.map((f) => (
            <li key={f} className="flex items-center gap-2 rounded-md border border-line bg-surface px-2.5 py-1.5 text-caption" title={f}>
              <FileText className="size-3.5 text-fg-muted" aria-hidden="true" />
              <span className="max-w-[200px] truncate">{baseName(f)}</span>
            </li>
          ))}
        </ul>
      )}
      <div className="max-w-[min(85%,560px)] rounded-xl rounded-br-md bg-sunk px-4 py-2.5 text-body break-words whitespace-pre-wrap text-fg">
        <span className="sr-only">You said: </span>
        {item.text}
      </div>
      <div className="flex h-7 items-center opacity-0 transition-opacity duration-(--relay-dur-fast) group-hover:opacity-100 focus-within:opacity-100">
        <CopyButton text={item.text} label="Copy message" />
      </div>
    </div>
  );
});

export const AssistantMessage = memo(function AssistantMessage({
  item,
  isLast,
  convId,
  canRetry,
  showTags = true,
}: {
  item: AssistantItem;
  isLast: boolean;
  convId: string;
  canRetry: boolean;
  /** Show the "what did the work" chips (off when a task card above already shows them). */
  showTags?: boolean;
}) {
  const tags: UsedTag[] = showTags ? (item.used?.length ? item.used : item.engine ? [engineTag(item.engine)] : []) : [];
  const body = <Markdown text={item.text} streaming={item.streaming} />;
  return (
    <div className="group turn py-2" aria-busy={item.streaming} data-testid="assistant-message" data-asks-user={item.asks_user ? "true" : undefined}>
      <span className="sr-only">Relay said: </span>
      {item.asks_user ? (
        <div className="rounded-xl border border-brand/35 bg-brand-soft/45 px-4 py-3" role="note" aria-label="Relay needs you to act">
          <p className="mb-1.5 inline-flex items-center gap-1.5 text-caption font-medium text-brand-text">
            <UserRound className="size-3.5" aria-hidden="true" /> Your turn
          </p>
          {body}
        </div>
      ) : (
        body
      )}
      {item.streaming && <span className="pulse-dot mt-2" aria-hidden="true" />}
      {(tags.length > 0 || (item.shared?.length ?? 0) > 0) && (
        <div className="mt-2.5 flex flex-wrap items-center gap-x-2 gap-y-1">
          <UsedChips tags={tags} label="Answered with" />
          {item.shared && item.shared.length > 0 && (
            <div className="min-w-0 basis-full sm:basis-auto">
              <PrivacyLedger shares={item.shared} staysLocal={false} compact />
            </div>
          )}
        </div>
      )}
      {!item.streaming && (
        <div
          className={cn(
            "mt-1 -ml-1.5 flex h-8 items-center gap-0.5 transition-opacity duration-(--relay-dur-fast)",
            isLast ? "opacity-100" : "opacity-0 group-hover:opacity-100 focus-within:opacity-100",
          )}
        >
          <CopyButton text={item.text} label="Copy answer" />
          {isLast && canRetry && (
            <IconButton label="Try again" size="icon-sm" onClick={() => void useApp.getState().retry(convId)}>
              <RotateCcw aria-hidden="true" />
            </IconButton>
          )}
        </div>
      )}
    </div>
  );
});

function engineTag(e: NonNullable<AssistantItem["engine"]>): UsedTag {
  return e.kind === "local" ? { kind: "local_ai", label: e.name } : { kind: "cloud_ai", label: e.name };
}

/** Shown while a run is starting or between steps, before any text arrives (no-streaming providers). */
export function Thinking() {
  return (
    <div className="flex items-center gap-2 py-3 text-caption text-fg-muted" role="status" aria-label="Relay is working">
      <span className="pulse-dot" aria-hidden="true" />
      <span className="shimmer-text">Working…</span>
    </div>
  );
}

export const Notice = memo(function Notice({ item }: { item: NoticeItem }) {
  return (
    <div className="turn flex items-center gap-3 py-2 text-caption text-fg-muted" role="note">
      <span className="h-px flex-1 bg-line" aria-hidden="true" />
      <span className="max-w-[80%] text-center">{item.text}</span>
      <span className="h-px flex-1 bg-line" aria-hidden="true" />
    </div>
  );
});

const ERROR_HELP: Record<string, { title: string; help: string }> = {
  not_logged_in: {
    title: "The cloud provider isn't signed in",
    help: "Open a terminal and run `claude auth login` (Claude Code) or `codex login` (Codex), then try again. Or switch to Local only.",
  },
  cli_not_installed: {
    title: "The cloud provider isn't installed",
    help: "Install it (see Settings › AI & Privacy for the exact command), or switch the privacy mode to Local only.",
  },
  timeout: { title: "The task took too long", help: "It was stopped after running without progress. You can try again." },
  cli: { title: "The cloud provider ran into a problem", help: "" },
  internal: { title: "Something went wrong", help: "" },
};

export const ErrorBanner = memo(function ErrorBanner({ item, convId, isLast }: { item: ErrorItem; convId: string; isLast: boolean }) {
  if (item.error === "cancelled") return <div className="turn py-2 text-center text-caption text-fg-muted">Stopped.</div>;
  const meta = ERROR_HELP[item.error] ?? ERROR_HELP.internal!;
  const parts = meta.help.split("`");
  return (
    <div role="alert" className="turn my-2 flex gap-3 rounded-xl border border-danger/25 bg-danger-soft px-4 py-3">
      <TriangleAlert className="mt-0.5 size-[18px] shrink-0 text-danger" aria-hidden="true" />
      <div className="min-w-0 flex-1 text-ui leading-[1.55]">
        <div className="font-medium text-fg">{meta.title}</div>
        {item.message && <div className="break-words text-fg-2">{item.message}</div>}
        {meta.help && <div className="mt-1 text-fg-2">{parts.map((p, i) => (i % 2 ? <Code key={i}>{p}</Code> : <span key={i}>{p}</span>))}</div>}
        <div className="mt-2.5 flex flex-wrap gap-2">
          {isLast && (
            <Button size="sm" variant="secondary" onClick={() => void useApp.getState().retry(convId)}>
              <RotateCcw aria-hidden="true" /> Try again
            </Button>
          )}
          {(item.error === "not_logged_in" || item.error === "cli_not_installed") && (
            <Button size="sm" variant="ghost" onClick={() => go(paths.settings("privacy"))}>
              <Settings2 aria-hidden="true" /> AI &amp; Privacy settings
            </Button>
          )}
        </div>
      </div>
    </div>
  );
});
