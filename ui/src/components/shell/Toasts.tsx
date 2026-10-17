import { CircleAlert, X } from "lucide-react";
import { useApp } from "@/store/app";
import { cn } from "@/lib/utils";

/** Bottom-centre pills. Errors are role=alert; info toasts are polite status messages. */
export function Toasts() {
  const toasts = useApp((s) => s.toasts);
  const dismiss = useApp((s) => s.dismissToast);
  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-28 z-(--relay-z-toast) flex flex-col items-center gap-2 px-4" aria-live="polite">
      {toasts.map((t) => (
        <div
          key={t.id}
          role={t.tone === "error" ? "alert" : "status"}
          className={cn(
            "toast-in pointer-events-auto flex max-w-[min(520px,100%)] items-center gap-2.5 rounded-lg border px-3.5 py-2 text-ui shadow-pop",
            t.tone === "error" ? "border-danger/30 bg-danger-soft text-fg" : "border-line bg-raised text-fg",
          )}
        >
          {t.tone === "error" && <CircleAlert className="size-4 shrink-0 text-danger" aria-hidden="true" />}
          <span className="min-w-0 flex-1 break-words">{t.text}</span>
          {t.action && (
            <button type="button" className="shrink-0 font-medium text-brand-text hover:underline" onClick={t.action.run}>
              {t.action.label}
            </button>
          )}
          <button
            type="button"
            aria-label="Dismiss"
            onClick={() => dismiss(t.id)}
            className="-mr-1 inline-flex size-6 shrink-0 items-center justify-center rounded-sm text-fg-muted hover:bg-hover hover:text-fg"
          >
            <X className="size-3.5" aria-hidden="true" />
          </button>
        </div>
      ))}
    </div>
  );
}

/** Polite live region for task progress and results (screen readers). */
export function LiveRegion() {
  const live = useApp((s) => s.live);
  return (
    <div className="sr-only" aria-live="polite" role="status" data-testid="live-region">
      {live}
    </div>
  );
}
