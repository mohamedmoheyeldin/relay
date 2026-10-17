import * as React from "react";
import { Progress as ProgressPrimitive } from "radix-ui";
import { cn } from "@/lib/utils";

/** Determinate bar (scaleX transform) or indeterminate sweep when `value` is null. */
function Progress({ className, value, ...props }: React.ComponentProps<typeof ProgressPrimitive.Root>) {
  const v = value === null || value === undefined ? null : Math.max(0, Math.min(100, value));
  return (
    <ProgressPrimitive.Root data-slot="progress" value={v} className={cn("relative h-1.5 w-full overflow-hidden rounded-full bg-sunk", className)} {...props}>
      {v === null ? (
        <div className="h-full w-1/3 animate-[relay-indeterminate_1.3s_var(--relay-ease-in-out)_infinite] rounded-full bg-brand" />
      ) : (
        <ProgressPrimitive.Indicator
          data-slot="progress-indicator"
          className="h-full w-full origin-left rounded-full bg-brand transition-transform duration-(--relay-dur-panel) ease-relay"
          style={{ transform: `scaleX(${v / 100})` }}
        />
      )}
    </ProgressPrimitive.Root>
  );
}

/** Labelled progress row used by Local AI setup and the installer. */
function ProgressRow({ percent, label }: { percent: number | null; label: string }) {
  const p = percent === null ? null : Math.round(Math.max(0, Math.min(100, percent)));
  return (
    <div>
      <div className="mb-2 flex items-center justify-between gap-3 text-caption">
        <span className="truncate text-fg-2">{label}</span>
        {p !== null && <span className="shrink-0 text-fg-muted tabular-nums">{p}%</span>}
      </div>
      <Progress value={p} aria-label={label} />
    </div>
  );
}

export { Progress, ProgressRow };
