// The structured task timeline: how Relay routed the task, what it shared, the AI's proposed plan,
// each step's live state (pending, running, verified, done, failed, skipped) and the verdict.
// AI output is a proposal; only "verified" means Relay checked the effect afterwards.
import { memo, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  Ban,
  ChevronDown,
  ChevronRight,
  CircleCheck,
  CircleX,
  Clock,
  Cloud,
  Cpu,
  Hand,
  Monitor,
  OctagonX,
  RotateCcw,
  ShieldAlert,
  ShieldCheck,
  Sparkles,
  Square,
  TriangleAlert,
  UserRound,
} from "lucide-react";
import type { TaskConfirm, TaskItem, TaskStep } from "@/lib/chat";
import {
  collapsedStepIndexes,
  COLLAPSE_STEPS_AT,
  DESTRUCTIVE_BADGE,
  planSourceLabel,
  providerLabel,
  ROUTE_BADGE_LABEL,
  routeBadge,
  STEP_STATE,
  stepBadges,
  stepStateLabel,
  usedTagsOf,
  verdictHint,
  verdictOf,
  verdictText,
  type StepBadge,
  type Verdict,
} from "@/lib/task";
import { UsedChips, ViaChip } from "./UsedChips";
import { formatDuration } from "@/lib/grouping";
import { cn } from "@/lib/utils";
import { useNow, formatClock } from "@/hooks/dom";
import { useRovingList } from "@/hooks/roving";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tip } from "@/components/ui/tooltip";
import { StepIcon } from "./StepIcon";
import { PrivacyLedger } from "./PrivacyLedger";
import { CONFIRM_TIMEOUT_S } from "@/store/app";
import { useAppearance } from "@/store/appearance";
import type { TaskView } from "@/lib/theme";

type Props = {
  item: TaskItem;
  /** The run is in progress in this window (shows Stop and live timers). */
  live?: boolean;
  onStop?: () => void;
  onRetry?: () => void;
  /** History view: no live controls. */
  readOnly?: boolean;
};

export const TaskTimeline = memo(function TaskTimeline({ item, live = false, onStop, onRetry, readOnly = false }: Props) {
  const running = item.outcome === null;
  const now = useNow(running && live, 1000);
  const elapsed = (item.finished_at ?? (running ? now : item.started_at)) - item.started_at;
  const verdict = verdictOf(item);
  const badge = item.route ? routeBadge(item.route.strategy) : null;
  const staysLocal = badge === "native" || badge === "local";
  const used = useMemo(() => usedTagsOf(item), [item]);
  // Settings > Appearance > Default task view. Changing it re-mounts the parts it controls, so it
  // applies at once to every task on screen.
  const view = useAppearance((s) => s.appearance.taskView);

  return (
    <section
      aria-label={`Task: ${item.plan?.objective ?? item.route?.reason ?? "Relay task"}`}
      aria-busy={running}
      className="turn my-3 overflow-hidden rounded-xl border border-line/70 bg-surface shadow-card"
      data-testid="task-timeline"
      data-verdict={verdict.kind}
      data-view={view}
    >
      <header className="flex items-start gap-3 px-4 pt-3.5 pb-3">
        <RouteBadge badge={badge} />
        <div className="min-w-0 flex-1">
          <p className="text-ui leading-5 text-fg">{item.route?.reason ?? "Deciding how to do this…"}</p>
          {item.previous_routes.length > 0 && (
            <p className="mt-0.5 text-caption text-fg-muted">
              Switched from {item.previous_routes.map((r) => providerLabel(r.provider)).join(", ")} to {providerLabel(item.route?.provider)}
            </p>
          )}
          <UsedChips tags={used} className="mt-2" />
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <span className="inline-flex items-center gap-1 text-caption text-fg-muted tabular-nums" aria-label={`Elapsed ${formatDuration(elapsed)}`}>
            <Clock className="size-3.5" aria-hidden="true" />
            {formatDuration(elapsed)}
          </span>
          {live && running && !readOnly && onStop && (
            <Button size="sm" variant="secondary" onClick={onStop} aria-label="Stop the task">
              <Square className="size-3 fill-current" aria-hidden="true" /> Stop
            </Button>
          )}
        </div>
      </header>

      <div className="border-t border-line/60">
        <PrivacyLedger shares={item.shared} staysLocal={staysLocal} />
      </div>

      {view === "simple" ? (
        <PendingConfirms item={item} live={live && !readOnly} />
      ) : item.plan ? (
        <PlanSection key={`plan-${view}`} item={item} live={live && !readOnly} view={view} />
      ) : running ? (
        <div className="border-t border-line/60 px-4 py-3 text-caption text-fg-muted">
          <span className="shimmer-text">Working out the steps…</span>
        </div>
      ) : null}

      <VerdictFooter key={`verdict-${view}`} verdict={verdict} item={item} onRetry={readOnly ? undefined : onRetry} view={view} />
    </section>
  );
});

function RouteBadge({ badge }: { badge: ReturnType<typeof routeBadge> | null }) {
  const icon =
    badge === "native" ? <Monitor aria-hidden="true" /> : badge === "local" ? <Cpu aria-hidden="true" /> : badge === "cloud" ? <Cloud aria-hidden="true" /> : <Ban aria-hidden="true" />;
  const tone = badge === "cloud" ? "info" : badge === "unavailable" ? "danger" : badge ? "success" : "neutral";
  return (
    <Badge tone={tone} className="mt-px h-6 px-2.5 text-caption [&>svg]:size-3.5" data-testid="route-badge">
      {badge ? icon : <Sparkles aria-hidden="true" />}
      {badge ? ROUTE_BADGE_LABEL[badge] : "Routing"}
    </Badge>
  );
}

/** Simple view: no step list, but an approval Relay is waiting for stays visible. */
function PendingConfirms({ item, live }: { item: TaskItem; live: boolean }) {
  const pending = item.confirms.filter((c) => c.state === "pending");
  if (!pending.length) return null;
  return (
    <div className="border-t border-line/60 pt-2 pb-0.5">
      {pending.map((c) => (
        <ConfirmLine key={c.confirm_id} c={c} live={live} />
      ))}
    </div>
  );
}

function PlanSection({ item, live, view }: { item: TaskItem; live: boolean; view: TaskView }) {
  const plan = item.plan!;
  const listRef = useRef<HTMLOListElement>(null);
  const onKeyDown = useRovingList(listRef);
  const long = item.steps.length > COLLAPSE_STEPS_AT;
  // Web tasks and finished long runs start collapsed; a plan that is about to run stays open so the
  // user can read every step before approving anything.
  const [expanded, setExpanded] = useState<boolean | null>(null);
  const collapsed = long && view !== "verbose" && !(expanded ?? !(item.intent === "web" || item.outcome !== null));
  const visible = collapsed ? collapsedStepIndexes(item.steps) : item.steps.map((_, i) => i);
  const hidden = item.steps.length - visible.length;
  const listId = `plan-steps-${item.id}`;
  return (
    <div className="border-t border-line/60 px-2 pt-3 pb-2">
      <div className="mb-2 flex flex-wrap items-baseline gap-x-2 gap-y-0.5 px-2">
        <h3 className="text-ui font-medium text-fg">{item.intent === "web" ? "Steps" : "Plan"}</h3>
        <span className="text-caption text-fg-muted" data-testid="plan-source">
          Proposed by {planSourceLabel(plan.source)}
          {plan.source !== "native" && <> · Relay checks each step as it goes</>}
        </span>
      </div>
      <p className="mb-2 px-2 font-serif text-[17px] leading-6 text-fg">{plan.objective}</p>
      {plan.notes && <p className="mb-2 px-2 text-caption text-fg-2">{plan.notes}</p>}
      {long && (
        <button
          type="button"
          aria-expanded={!collapsed}
          aria-controls={listId}
          onClick={() => setExpanded(collapsed)}
          className="mb-1 flex h-8 w-full items-center gap-2 rounded-lg px-2 text-left text-caption text-fg-2 transition-colors hover:bg-hover hover:text-fg"
          data-testid="steps-toggle"
        >
          <ChevronDown aria-hidden="true" className={cn("size-4 text-fg-muted transition-transform duration-(--relay-dur-fast)", !collapsed && "rotate-180")} />
          {collapsed ? `Show all ${item.steps.length} steps (${hidden} more)` : "Show only the latest step"}
        </button>
      )}
      <ol id={listId} ref={listRef} onKeyDown={onKeyDown} aria-label={collapsed ? `Plan steps, ${visible.length} of ${item.steps.length} shown` : "Plan steps"} className="space-y-px">
        {visible.map((i) => {
          const s = item.steps[i]!;
          return <StepRow key={s.id} step={s} index={i} confirms={item.confirms.filter((c) => c.step_id === s.id)} live={live} defaultOpen={view === "verbose"} />;
        })}
      </ol>
      {item.confirms
        .filter((c) => !c.step_id || !item.steps.some((s) => s.id === c.step_id))
        .map((c) => (
          <ConfirmLine key={c.confirm_id} c={c} live={live} />
        ))}
    </div>
  );
}

const StepRow = memo(function StepRow({
  step,
  index,
  confirms,
  live,
  defaultOpen = false,
}: {
  step: TaskStep;
  index: number;
  confirms: TaskConfirm[];
  live: boolean;
  defaultOpen?: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen || step.status === "failed");
  // A step that fails opens its details so the reason is visible without a click.
  useEffect(() => {
    if (step.status === "failed") setOpen(true);
  }, [step.status]);
  const meta = STEP_STATE[step.status];
  const id = `step-${step.id}-${index}`;
  const running = step.status === "running";
  const badges = stepBadges(step);
  return (
    <li className={cn("rounded-lg", running && "shimmer-row", step.status === "failed" && "bg-danger-soft/50")} data-testid="plan-step" data-status={step.status}>
      <button
        type="button"
        data-roving
        data-roving-active={running ? "true" : undefined}
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen((o) => !o)}
        className="flex min-h-(--relay-step-min) w-full items-center gap-3 rounded-lg px-2 py-(--relay-step-pad) text-left transition-colors hover:bg-hover"
      >
        <Tip label={meta.description} side="left">
          <span className="inline-flex">
            <StepIcon state={step.status} />
          </span>
        </Tip>
        <span className="w-4 shrink-0 text-right text-caption text-fg-muted tabular-nums" aria-hidden="true">
          {index + 1}
        </span>
        <span className={cn("min-w-0 flex-1 text-ui leading-5", step.status === "pending" || step.status === "skipped" ? "text-fg-2" : "text-fg")}>
          {step.title}
          {(step.via || badges.length > 0) && (
            <span className="ml-2 inline-flex flex-wrap gap-1 align-[1px]">
              <ViaChip via={step.via} server={step.server} />
              {badges.map((b) => (
                <RiskBadge key={b.key} badge={b} />
              ))}
            </span>
          )}
        </span>
        <span
          className={cn(
            "shrink-0 text-caption",
            step.status === "verified" && "font-medium text-success",
            step.status === "failed" && "font-medium text-danger",
            step.status === "running" && "text-brand-text",
            (step.status === "done" || step.status === "pending" || step.status === "skipped" || step.status === "stopped") && "text-fg-muted",
          )}
        >
          <span className="sr-only">Step {index + 1}: </span>
          {stepStateLabel(step)}
        </span>
        <ChevronRight aria-hidden="true" className={cn("size-4 shrink-0 text-fg-muted transition-transform duration-(--relay-dur-fast)", open && "rotate-90")} />
      </button>
      {confirms.map((c) => (
        <ConfirmLine key={c.confirm_id} c={c} live={live} />
      ))}
      {open && (
        <div id={id} className="rise-in pr-3 pb-3 pl-[60px] text-caption leading-5">
          {step.summary ? (
            <p className={cn(step.status === "failed" ? "text-danger" : "text-fg-2")}>{step.summary}</p>
          ) : (
            <p className="text-fg-muted">{meta.description}</p>
          )}
          <dl className="mt-1.5 flex flex-wrap gap-x-4 gap-y-0.5 text-fg-muted">
            <div className="flex gap-1">
              <dt>Tool</dt>
              <dd className="font-mono text-[12px] text-fg-2">{step.tool || "—"}</dd>
            </div>
            {step.attempts > 1 && (
              <div className="flex gap-1">
                <dt>Attempts</dt>
                <dd className="text-fg-2">{step.attempts}</dd>
              </div>
            )}
            {step.started_at && step.finished_at && (
              <div className="flex gap-1">
                <dt>Took</dt>
                <dd className="text-fg-2">{formatDuration(step.finished_at - step.started_at)}</dd>
              </div>
            )}
          </dl>
        </div>
      )}
    </li>
  );
});

function RiskBadge({ badge }: { badge: StepBadge }) {
  return (
    <Badge tone={badge.tone} className="h-[18px] px-1.5" title={badge.description} data-testid="risk-badge" data-risk={badge.key}>
      {badge.key === "destructive" ? <TriangleAlert aria-hidden="true" /> : <ShieldAlert aria-hidden="true" />}
      {badge.label}
    </Badge>
  );
}

function ConfirmLine({ c, live }: { c: TaskConfirm; live: boolean }) {
  const pending = c.state === "pending";
  const now = useNow(pending && live, 1000);
  const left = Math.max(0, CONFIRM_TIMEOUT_S * 1000 - (now - c.at));
  return (
    <div
      className={cn(
        "mx-2 mb-1.5 flex items-center gap-2 rounded-md px-3 py-2 text-caption",
        pending ? "bg-warning-soft text-fg" : "bg-hover text-fg-2",
      )}
      data-testid="confirm-line"
      data-state={c.state}
    >
      {pending ? (
        <ShieldAlert className="size-4 shrink-0 text-warning" aria-hidden="true" />
      ) : c.state === "approved" ? (
        <ShieldCheck className="size-4 shrink-0 text-success" aria-hidden="true" />
      ) : (
        <OctagonX className="size-4 shrink-0 text-fg-muted" aria-hidden="true" />
      )}
      <span className="min-w-0 flex-1">
        {pending ? "Waiting for your OK: " : c.state === "approved" ? "You said yes: " : "You said no: "}
        <span className="text-fg">{c.title}</span>
      </span>
      {c.risk === "destructive" && <RiskBadge badge={DESTRUCTIVE_BADGE} />}
      {pending && live && <span className="shrink-0 tabular-nums text-fg-muted">{formatClock(left)}</span>}
    </div>
  );
}

function VerdictFooter({ verdict, item, onRetry, view }: { verdict: Verdict; item: TaskItem; onRetry?: () => void; view: TaskView }) {
  const [showLog, setShowLog] = useState(view === "verbose");
  if (verdict.kind === "running") {
    return (
      <footer className="flex items-center gap-2 border-t border-line/60 px-4 py-2.5 text-caption text-fg-muted" aria-live="off">
        <span className="shimmer-text">Working</span>
        {verdict.total > 0 && <span>· {verdictText(verdict)}</span>}
      </footer>
    );
  }
  const tone: Record<Verdict["kind"], string> = {
    running: "text-fg-muted",
    verified: "text-success",
    unverified: "text-fg-2",
    failed: "text-danger",
    stopped: "text-fg-2",
    declined: "text-warning",
    timed_out: "text-warning",
    waiting: "text-brand-text",
    answered: "text-fg-2",
  };
  const icon: Record<Verdict["kind"], ReactNode> = {
    running: null,
    verified: <ShieldCheck className="size-4" aria-hidden="true" />,
    unverified: <CircleCheck className="size-4" aria-hidden="true" />,
    failed: <CircleX className="size-4" aria-hidden="true" />,
    stopped: <OctagonX className="size-4" aria-hidden="true" />,
    declined: <Hand className="size-4" aria-hidden="true" />,
    timed_out: <Clock className="size-4" aria-hidden="true" />,
    waiting: <UserRound className="size-4" aria-hidden="true" />,
    answered: <CircleCheck className="size-4" aria-hidden="true" />,
  };
  const hint = verdictHint(verdict);
  return (
    <footer className="border-t border-line/60">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2 px-4 py-2.5">
        <p className={cn("inline-flex items-center gap-1.5 text-ui font-medium", tone[verdict.kind])} data-testid="task-verdict">
          {icon[verdict.kind]}
          {verdictText(verdict)}
        </p>
        {hint && <span className="text-caption text-fg-muted">{hint}</span>}
        <span className="flex-1" />
        {onRetry && (verdict.kind === "failed" || verdict.kind === "stopped" || verdict.kind === "timed_out") && (
          <Button size="sm" variant="secondary" onClick={onRetry}>
            <RotateCcw aria-hidden="true" /> Retry task
          </Button>
        )}
        <Button size="sm" variant="ghost" aria-expanded={showLog} onClick={() => setShowLog((v) => !v)}>
          Details
          <ChevronRight aria-hidden="true" className={cn("transition-transform duration-(--relay-dur-fast)", showLog && "rotate-90")} />
        </Button>
      </div>
      {showLog && (
        <pre className="rise-in mx-4 mb-3 max-h-64 overflow-auto rounded-md bg-sunk p-3 font-mono text-[12px] leading-5 whitespace-pre-wrap text-fg-2 scroll-thin" aria-label="Raw task log">
          {rawLog(item)}
        </pre>
      )}
    </footer>
  );
}

/** Plain-text log of everything the timeline knows (the "raw output" view). */
export function rawLog(t: TaskItem): string {
  const lines: string[] = [];
  const ts = (n: number | null) => (n ? new Date(n).toLocaleTimeString() : "--");
  for (const r of t.previous_routes) lines.push(`route   ${r.strategy}${r.provider ? ` (${r.provider})` : ""}: ${r.reason}  [replaced]`);
  if (t.route) lines.push(`route   ${t.route.strategy}${t.route.provider ? ` (${t.route.provider})` : ""}: ${t.route.reason}`);
  if (t.engine) lines.push(`engine  ${t.engine.kind}: ${t.engine.name}`);
  for (const u of t.used ?? []) lines.push(`used    ${u.kind}: ${u.label}${u.detail ? ` (${u.detail})` : ""}`);
  for (const s of t.shared) lines.push(`shared  ${s.provider}: ${s.categories.map((c) => `${c.category}=${c.items}`).join(", ")} (${s.bytes} bytes)`);
  if (t.plan) lines.push(`plan    source=${t.plan.source} objective="${t.plan.objective}"${t.plan.notes ? ` notes="${t.plan.notes}"` : ""}`);
  t.steps.forEach((s, i) =>
    lines.push(`step ${i + 1}  [${s.status}] ${s.id} ${s.tool} risk=${s.risk}${s.via ? ` via=${s.via}${s.server ? `:${s.server}` : ""}` : ""}${s.sensitive_data ? " sensitive_data" : ""} attempts=${s.attempts} ${ts(s.started_at)}–${ts(s.finished_at)}${s.summary ? `\n        ${s.summary}` : ""}`),
  );
  for (const c of t.confirms) lines.push(`confirm #${c.confirm_id} ${c.state} (${c.risk}) step=${c.step_id ?? "-"}: ${c.title}`);
  lines.push(`outcome ${t.outcome ?? "running"}  started ${ts(t.started_at)}  finished ${ts(t.finished_at)}`);
  return lines.join("\n");
}
