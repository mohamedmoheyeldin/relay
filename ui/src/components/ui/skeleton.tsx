import * as React from "react";
import { cn } from "@/lib/utils";

/** Placeholder block while data loads (opacity pulse; static under reduced motion). */
function Skeleton({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="skeleton" aria-hidden="true" className={cn("skeleton h-4", className)} {...props} />;
}

/** A few text-like skeleton lines with varied widths. */
function SkeletonLines({ n = 3, className }: { n?: number; className?: string }) {
  return (
    <div className={cn("space-y-2.5", className)} aria-hidden="true">
      {Array.from({ length: n }, (_, i) => (
        <Skeleton key={i} style={{ width: `${58 + ((i * 29) % 38)}%` }} />
      ))}
    </div>
  );
}

export { Skeleton, SkeletonLines };
