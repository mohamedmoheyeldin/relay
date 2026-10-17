import { Check, CircleCheck, CircleDashed, CircleMinus, CircleX, OctagonX } from "lucide-react";
import type { TaskStepState } from "@/lib/chat";
import { cn } from "@/lib/utils";

/** Icon per step state. Each state also has a text label next to it (never colour alone). */
export function StepIcon({ state, className }: { state: TaskStepState; className?: string }) {
  const base = cn("size-[18px] shrink-0", className);
  switch (state) {
    case "pending":
      return <CircleDashed aria-hidden="true" className={cn(base, "text-fg-muted/70")} strokeWidth={1.75} />;
    case "running":
      return (
        <span aria-hidden="true" className={cn(base, "relative inline-flex items-center justify-center")}>
          <span className="absolute inset-[3px] rounded-full border-[1.75px] border-brand/35" />
          <span className="pulse-dot !size-[7px]" />
        </span>
      );
    case "verified":
      return <CircleCheck aria-hidden="true" className={cn(base, "fill-success text-surface")} strokeWidth={2.25} />;
    case "done":
      return (
        <span aria-hidden="true" className={cn(base, "inline-flex items-center justify-center rounded-full border-[1.75px] border-line-strong/80")}>
          <Check className="size-[11px] text-fg-2" strokeWidth={3} />
        </span>
      );
    case "failed":
      return <CircleX aria-hidden="true" className={cn(base, "fill-danger text-surface")} strokeWidth={2.25} />;
    case "skipped":
      return <CircleMinus aria-hidden="true" className={cn(base, "text-fg-muted")} strokeWidth={1.75} />;
    case "stopped":
      return <OctagonX aria-hidden="true" className={cn(base, "text-fg-muted")} strokeWidth={1.75} />;
  }
}
