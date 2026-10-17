// Task history (/tasks, /tasks/:id): virtualised list of workflow_list entries with outcome filters;
// selecting one opens the read-only timeline from workflow_get (approvals, errors, shared context).
import { useMemo, useRef, useState, type KeyboardEvent } from "react";
import { useParams } from "react-router";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowLeft, Cloud, Cpu, History, MessageSquare, Monitor, Search, ShieldCheck, TriangleAlert } from "lucide-react";
import { useWorkflow, useWorkflows } from "@/hooks/queries";
import { useNarrow } from "@/hooks/dom";
import { errorText } from "@/lib/backend";
import { go, paths } from "@/lib/nav";
import { cn } from "@/lib/utils";
import { formatWhen, matchesFilter, OUTCOME_META, strategyOf, workflowToTask, type OutcomeFilter } from "@/lib/workflow";
import { providerLabel, routeBadge } from "@/lib/task";
import type { WorkflowSummary } from "@/lib/types";
import { Header } from "@/components/shell/Header";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Segmented } from "@/components/ui/toggle-group";
import { Skeleton } from "@/components/ui/skeleton";
import { EmptyState, ErrorNote } from "@/components/common";
import { TaskTimeline } from "@/components/task/TaskTimeline";

const ROW_H = 60;

const FILTERS: { value: OutcomeFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "verified", label: "All checked" },
  { value: "completed", label: "Completed" },
  { value: "failed", label: "Failed" },
  { value: "stopped", label: "Stopped" },
];

export default function TasksPage() {
  const { id } = useParams();
  const narrow = useNarrow();
  const list = useWorkflows();
  const [filter, setFilter] = useState<OutcomeFilter>("all");
  const [q, setQ] = useState("");
  const rows = useMemo(
    () => (list.data ?? []).filter((w) => matchesFilter(w, filter) && (!q.trim() || w.objective.toLowerCase().includes(q.trim().toLowerCase()))),
    [list.data, filter, q],
  );
  const showList = !narrow || !id;
  const showDetail = Boolean(id);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <Header title="Task history" />
      <div className="flex min-h-0 flex-1">
        {showList && (
          <section aria-label="Tasks" className={cn("flex min-h-0 flex-col", showDetail && !narrow ? "w-[min(440px,45%)] shrink-0 border-r border-line/60" : "mx-auto w-full max-w-(--relay-content-max)")}>
            <div className="space-y-3 px-4 pt-2 pb-3">
              {!showDetail && <h1 className="font-display text-title">Task history</h1>}
              <div className="flex flex-wrap items-center gap-2">
                <Segmented<OutcomeFilter> label="Filter by outcome" size="sm" value={filter} onValueChange={setFilter} options={FILTERS} />
                <div className="relative min-w-[160px] flex-1">
                  <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-fg-muted" aria-hidden="true" />
                  <input
                    value={q}
                    onChange={(e) => setQ(e.target.value)}
                    placeholder="Filter by objective"
                    aria-label="Filter tasks by objective"
                    className="h-7 w-full rounded-md border border-line-strong bg-surface pr-2 pl-8 text-caption outline-none placeholder:text-fg-muted focus-visible:border-brand"
                  />
                </div>
              </div>
            </div>
            {list.isPending ? (
              <div className="space-y-2 px-4" aria-label="Loading tasks" role="status">
                {Array.from({ length: 7 }, (_, i) => (
                  <Skeleton key={i} className="h-[52px] rounded-lg" />
                ))}
              </div>
            ) : list.isError ? (
              <div className="px-4">
                <ErrorNote title="Couldn't load the task history" action={<Button size="sm" onClick={() => void list.refetch()}>Try again</Button>}>
                  {errorText(list.error)}
                </ErrorNote>
              </div>
            ) : (list.data ?? []).length === 0 ? (
              <div className="px-4">
                <EmptyState icon={<History />} title="No tasks yet">
                  Every task Relay does shows up here: its steps, what it shared, and which steps it could check.
                </EmptyState>
              </div>
            ) : rows.length === 0 ? (
              <div className="px-4">
                <EmptyState title="No tasks match this filter" action={<Button size="sm" onClick={() => { setFilter("all"); setQ(""); }}>Show all tasks</Button>} />
              </div>
            ) : (
              <TaskList rows={rows} selected={id ?? null} compact={showDetail && !narrow} />
            )}
          </section>
        )}
        {showDetail && <TaskDetail id={id!} narrow={narrow} />}
      </div>
    </div>
  );
}

function StrategyIcon({ strategy }: { strategy: string }) {
  const b = routeBadge(strategyOf(strategy));
  const cls = "size-3.5 shrink-0";
  if (b === "native") return <Monitor className={cls} aria-hidden="true" />;
  if (b === "local") return <Cpu className={cls} aria-hidden="true" />;
  if (b === "cloud") return <Cloud className={cls} aria-hidden="true" />;
  return <TriangleAlert className={cls} aria-hidden="true" />;
}

function TaskList({ rows, selected, compact }: { rows: WorkflowSummary[]; selected: string | null; compact: boolean }) {
  const scroller = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState(() => Math.max(0, rows.findIndex((r) => r.id === selected)));
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW_H,
    overscan: 8,
    getItemKey: (i) => rows[i]?.id ?? i,
    initialRect: { width: 600, height: 600 },
  });
  const activeIdx = Math.min(active, rows.length - 1);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    let next = activeIdx;
    if (e.key === "ArrowDown") next = Math.min(rows.length - 1, activeIdx + 1);
    else if (e.key === "ArrowUp") next = Math.max(0, activeIdx - 1);
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = rows.length - 1;
    else if (e.key === "PageDown") next = Math.min(rows.length - 1, activeIdx + 8);
    else if (e.key === "PageUp") next = Math.max(0, activeIdx - 8);
    else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      const r = rows[activeIdx];
      if (r) go(paths.tasks(r.id));
      return;
    } else return;
    e.preventDefault();
    setActive(next);
    virtualizer.scrollToIndex(next, { align: "auto" });
  };

  return (
    <div
      ref={scroller}
      role="listbox"
      tabIndex={0}
      aria-label={`${rows.length} tasks`}
      aria-activedescendant={rows[activeIdx] && virtualizer.getVirtualItems().some((v) => v.index === activeIdx) ? `task-${rows[activeIdx]!.id}` : undefined}
      onKeyDown={onKeyDown}
      className="min-h-0 flex-1 overflow-y-auto px-2 pb-4 outline-none scroll-thin focus-visible:outline-none [&:focus-visible_[data-active=true]]:outline-2 [&:focus-visible_[data-active=true]]:outline-focus"
      data-testid="task-list"
    >
      <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
        {virtualizer.getVirtualItems().map((v) => {
          const w = rows[v.index]!;
          const meta = OUTCOME_META[w.outcome];
          const allVerified = w.steps_total > 0 && w.steps_verified === w.steps_total;
          return (
            <div
              key={v.key}
              id={`task-${w.id}`}
              role="option"
              aria-selected={w.id === selected}
              data-active={v.index === activeIdx}
              onClick={() => {
                setActive(v.index);
                go(paths.tasks(w.id));
              }}
              className={cn(
                "absolute top-0 left-0 flex w-full items-center gap-3 rounded-lg px-3 outline-offset-[-2px]",
                w.id === selected ? "bg-active" : "hover:bg-hover",
              )}
              style={{ height: ROW_H - 4, transform: `translateY(${v.start}px)` }}
              data-testid="task-row"
            >
              <div className="min-w-0 flex-1">
                <div className="truncate text-ui text-fg">{w.objective}</div>
                <div className={cn("mt-0.5 flex items-center gap-1.5 truncate text-caption", w.id === selected ? "text-fg-2" : "text-fg-muted")}>
                  <StrategyIcon strategy={w.strategy} />
                  <span className="truncate">{strategyOf(w.strategy) === "native" ? "On this PC" : providerLabel(w.provider)}</span>
                  <span aria-hidden="true">·</span>
                  <span className={cn("tabular-nums", allVerified && "text-success")}>
                    {allVerified && <ShieldCheck className="mr-0.5 inline size-3 align-[-1px]" aria-hidden="true" />}
                    {w.steps_total === 0 ? "No steps" : `${w.steps_verified} of ${w.steps_total} checked`}
                  </span>
                  {!compact && (
                    <>
                      <span aria-hidden="true">·</span>
                      <span className="truncate">{formatWhen(w.created_at)}</span>
                    </>
                  )}
                </div>
              </div>
              <Badge tone={meta.tone}>{meta.label}</Badge>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function TaskDetail({ id, narrow }: { id: string; narrow: boolean }) {
  const wf = useWorkflow(id);
  const task = useMemo(() => (wf.data ? workflowToTask(wf.data) : null), [wf.data]);
  return (
    <section aria-label="Task details" className="min-h-0 flex-1 overflow-y-auto px-4 pb-10 scroll-thin" data-testid="task-detail">
      <div className="mx-auto max-w-[720px] pt-2">
        {narrow && (
          <Button variant="ghost" size="sm" className="mb-2 -ml-1" onClick={() => go(paths.tasks())}>
            <ArrowLeft aria-hidden="true" /> All tasks
          </Button>
        )}
        {wf.isPending ? (
          <div className="space-y-3" role="status" aria-label="Loading task">
            <Skeleton className="h-6 w-2/3" />
            <Skeleton className="h-64 w-full rounded-xl" />
          </div>
        ) : wf.isError ? (
          <ErrorNote title="Couldn't load this task">{errorText(wf.error)}</ErrorNote>
        ) : wf.data && task ? (
          <>
            <div className="flex flex-wrap items-center gap-2">
              <Badge tone={OUTCOME_META[wf.data.outcome].tone}>{OUTCOME_META[wf.data.outcome].label}</Badge>
              <span className="text-caption text-fg-muted">{new Date(wf.data.created_at).toLocaleString()}</span>
              <span className="flex-1" />
              {wf.data.conv_id && (
                <Button size="sm" variant="secondary" onClick={() => go(paths.chat(wf.data!.conv_id!))}>
                  <MessageSquare aria-hidden="true" /> Open chat
                </Button>
              )}
            </div>
            <TaskTimeline item={task} readOnly />
            {(wf.data.approvals.length > 0 || wf.data.errors.length > 0) && (
              <div className="mt-4 space-y-4 px-1">
                {wf.data.approvals.length > 0 && (
                  <div>
                    <h2 className="mb-1.5 text-micro font-medium tracking-[0.02em] text-fg-muted">Approvals</h2>
                    <ul className="space-y-1 text-caption text-fg-2">
                      {wf.data.approvals.map((a, i) => (
                        <li key={i}>{a}</li>
                      ))}
                    </ul>
                  </div>
                )}
                {wf.data.errors.length > 0 && (
                  <div>
                    <h2 className="mb-1.5 text-micro font-medium tracking-[0.02em] text-fg-muted">Errors</h2>
                    <ul className="space-y-1 text-caption text-danger">
                      {wf.data.errors.map((a, i) => (
                        <li key={i}>{a}</li>
                      ))}
                    </ul>
                  </div>
                )}
              </div>
            )}
          </>
        ) : null}
      </div>
    </section>
  );
}
