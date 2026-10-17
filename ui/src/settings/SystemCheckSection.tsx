// Settings > System check: what Relay can use on this PC (Relay Local AI, the cloud CLIs, Node.js,
// browser control), each with its status in words and, when the backend offers one, a fix.
import { CircleCheck, CircleX, ExternalLink, Play, RefreshCw, TriangleAlert } from "lucide-react";
import { useSystemCheck } from "@/hooks/queries";
import { isRunnable, recheck, runSystemAction } from "@/lib/systemCheck";
import type { SystemCheckItem, SystemCheckStatus } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Card, LoadState, PageTitle } from "./parts";

const STATUS: Record<SystemCheckStatus, { text: string; className: string; icon: typeof CircleCheck }> = {
  ok: { text: "Ready", className: "text-success", icon: CircleCheck },
  warn: { text: "Needs attention", className: "text-warning", icon: TriangleAlert },
  missing: { text: "Missing", className: "text-danger", icon: CircleX },
};

export function SystemCheckSection() {
  const q = useSystemCheck();
  const checking = q.isFetching;
  const data = q.data;
  return (
    <>
      <PageTitle
        description="What Relay can use on this PC, and how to fix anything that is missing."
        action={
          <Button size="sm" variant="secondary" onClick={() => void recheck()} disabled={checking}>
            <RefreshCw className={cn(checking && "animate-spin")} aria-hidden="true" /> {checking ? "Checking…" : "Check again"}
          </Button>
        }
      >
        System check
      </PageTitle>
      {!data && q.error ? (
        <LoadState error={q.error} retry={() => void recheck()} />
      ) : (
        <>
          <p className="mb-3 px-1 text-caption text-fg-muted" aria-live="polite" data-testid="system-check-summary">
            {checking || !data ? "Checking this PC…" : `${data.ok} of ${data.total} ready`}
          </p>
          <Card>
            {checking || !data
              ? Array.from({ length: data?.items.length || 5 }, (_, i) => (
                  <div key={i} className="flex items-center gap-3 py-3.5" role="status" aria-label="Checking">
                    <Skeleton className="size-5 rounded-full" />
                    <div className="flex-1 space-y-1.5">
                      <Skeleton className="h-4 w-32" />
                      <Skeleton className="h-3 w-64 max-w-full" />
                    </div>
                  </div>
                ))
              : data.items.map((item) => <CheckRow key={item.id} item={item} />)}
          </Card>
        </>
      )}
    </>
  );
}

function CheckRow({ item }: { item: SystemCheckItem }) {
  const s = STATUS[item.status];
  const action = item.action && isRunnable(item.action) ? item.action : null;
  return (
    <div className="flex flex-wrap items-start gap-x-3 gap-y-2 py-(--relay-setting-pad)" data-testid="system-check-item" data-id={item.id} data-status={item.status}>
      <s.icon className={cn("mt-0.5 size-5 shrink-0", s.className)} aria-hidden="true" />
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-baseline gap-x-2">
          <span className="text-ui font-medium text-fg">{item.label}</span>
          <span className={cn("text-caption font-medium", s.className)}>{s.text}</span>
        </div>
        {item.detail && <p className="mt-0.5 text-caption leading-5 text-fg-muted">{item.detail}</p>}
      </div>
      {action && (
        <Button size="sm" variant={item.status === "ok" ? "ghost" : "secondary"} onClick={() => void runSystemAction(action)} className="shrink-0">
          {action.kind === "url" ? <ExternalLink aria-hidden="true" /> : <Play aria-hidden="true" />} {action.label}
        </Button>
      )}
    </div>
  );
}
