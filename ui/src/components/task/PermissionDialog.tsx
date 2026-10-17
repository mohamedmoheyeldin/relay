// Approval request from a running task (`confirm` event): a blocking dialog with a countdown.
// Declining, pressing Esc or letting it time out means the step is not executed.
import { useEffect, useRef } from "react";
import { ShieldAlert, TriangleAlert } from "lucide-react";
import { CONFIRM_TIMEOUT_S, useApp, type ConfirmReq } from "@/store/app";
import { useNow, formatClock } from "@/hooks/dom";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { AlertDialog, AlertDialogContent, AlertDialogDescription, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { Progress } from "@/components/ui/progress";
import { Badge } from "@/components/ui/badge";
import { DESTRUCTIVE_BADGE, SENSITIVE_BADGE } from "@/lib/task";

export function PermissionDialog() {
  const req = useApp((s) => s.confirms[0] ?? null);
  const more = useApp((s) => Math.max(0, s.confirms.length - 1));
  const answer = useApp((s) => s.answerConfirm);
  return <PermissionDialogView req={req} more={more} onAnswer={answer} />;
}

export function PermissionDialogView({ req, more, onAnswer }: { req: ConfirmReq | null; more: number; onAnswer: (id: number, approved: boolean) => void }) {
  const now = useNow(req !== null, 1000);
  const leftMs = req ? Math.max(0, CONFIRM_TIMEOUT_S * 1000 - (now - req.at)) : 0;
  const destructive = req?.risk === "destructive";
  const declineRef = useRef<HTMLButtonElement>(null);
  const approveRef = useRef<HTMLButtonElement>(null);
  // Safer default focus: Decline for destructive actions, Approve for reversible ones.
  useEffect(() => {
    if (!req) return;
    const t = setTimeout(() => (destructive ? declineRef : approveRef).current?.focus(), 30);
    return () => clearTimeout(t);
  }, [req, destructive]);

  return (
    <AlertDialog open={req !== null}>
      <AlertDialogContent
        data-testid="permission-dialog"
        className={cn("w-[min(480px,calc(100vw-24px))] gap-0 p-0", destructive && "border-danger/40")}
        onEscapeKeyDown={(e) => {
          e.preventDefault();
          if (req) onAnswer(req.id, false);
        }}
      >
        <div className="flex gap-3.5 px-6 pt-6">
          <span
            className={cn("inline-flex size-10 shrink-0 items-center justify-center rounded-full", destructive ? "bg-danger-soft text-danger" : "bg-warning-soft text-warning")}
            aria-hidden="true"
          >
            {destructive ? <TriangleAlert className="size-5" /> : <ShieldAlert className="size-5" />}
          </span>
          <div className="min-w-0 flex-1">
            <p className="flex flex-wrap items-center gap-1.5">
              <span className={cn("text-caption font-medium", destructive ? "text-danger" : "text-warning")}>
                {destructive ? `${DESTRUCTIVE_BADGE.label} · needs your OK` : "Needs your OK"}
              </span>
              {req?.sensitive && (
                <Badge tone="warning" title={SENSITIVE_BADGE.description} data-testid="sensitive-badge">
                  <ShieldAlert aria-hidden="true" />
                  {SENSITIVE_BADGE.label}
                </Badge>
              )}
            </p>
            <AlertDialogTitle className="mt-1 font-sans text-heading font-semibold break-words">{req?.title}</AlertDialogTitle>
            <AlertDialogDescription asChild>
              <div className="mt-2 text-ui text-fg-2">
                {req?.detail ? (
                  <pre className="max-h-48 overflow-y-auto rounded-md bg-sunk px-3 py-2 font-mono text-[12.5px] leading-5 break-words whitespace-pre-wrap text-fg-2 scroll-thin">
                    {req.detail}
                  </pre>
                ) : (
                  <p>Relay will only go on if you say yes.</p>
                )}
              </div>
            </AlertDialogDescription>
          </div>
        </div>
        <div className="mt-5 px-6">
          <div className="mb-1.5 flex items-center justify-between text-caption text-fg-muted">
            <span>{destructive ? "Decline to keep everything as it is." : "Decline to skip this step."}</span>
            <span className="tabular-nums" aria-label={`${Math.ceil(leftMs / 1000)} seconds left`}>
              Declines in {formatClock(leftMs)}
            </span>
          </div>
          <Progress value={(leftMs / (CONFIRM_TIMEOUT_S * 1000)) * 100} aria-label="Time left to answer" className="h-1" />
        </div>
        <div className="mt-5 flex items-center gap-2 border-t border-line px-6 py-4">
          {more > 0 && <span className="text-caption text-fg-muted">{more} more waiting</span>}
          <span className="flex-1" />
          <Button ref={declineRef} variant="secondary" size="lg" onClick={() => req && onAnswer(req.id, false)}>
            Decline
          </Button>
          <Button ref={approveRef} variant={destructive ? "danger" : "primary"} size="lg" onClick={() => req && onAnswer(req.id, true)}>
            Approve
          </Button>
        </div>
      </AlertDialogContent>
    </AlertDialog>
  );
}
