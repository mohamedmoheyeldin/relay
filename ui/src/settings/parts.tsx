import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import { Skeleton } from "@/components/ui/skeleton";
import { ErrorNote } from "@/components/common";
import { Button } from "@/components/ui/button";
import { errorText } from "@/lib/backend";

export function PageTitle({ children, description, action }: { children: ReactNode; description?: ReactNode; action?: ReactNode }) {
  return (
    <div className="mb-8 flex items-start gap-4">
      <div className="min-w-0 flex-1">
        <h1 className="font-display text-title text-fg">{children}</h1>
        {description && <p className="mt-1.5 text-ui leading-6 text-fg-2">{description}</p>}
      </div>
      {action && <div className="shrink-0 pt-1">{action}</div>}
    </div>
  );
}

export function SectionTitle({ children, description, className, id }: { children: ReactNode; description?: ReactNode; className?: string; id?: string }) {
  return (
    <div className={cn("mt-10 mb-3 first:mt-0", className)}>
      <h2 id={id} className="text-heading font-semibold text-fg">
        {children}
      </h2>
      {description && <p className="mt-0.5 text-caption leading-5 text-fg-muted">{description}</p>}
    </div>
  );
}

export function Card({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn("divide-y divide-line/70 rounded-xl border border-line/70 bg-surface px-4", className)}>{children}</div>;
}

export function Row({
  label,
  desc,
  children,
  stack,
  htmlFor,
  id,
}: {
  label: ReactNode;
  desc?: ReactNode;
  children?: ReactNode;
  stack?: boolean;
  htmlFor?: string;
  id?: string;
}) {
  return (
    <div className={cn("flex gap-4 py-(--relay-setting-pad)", stack ? "flex-col items-stretch" : "items-center")}>
      <div className="min-w-0 flex-1">
        {htmlFor ? (
          <label htmlFor={htmlFor} id={id} className="block text-ui font-medium text-fg">
            {label}
          </label>
        ) : (
          <div id={id} className="text-ui font-medium text-fg">
            {label}
          </div>
        )}
        {desc && <div className="mt-0.5 text-caption leading-5 text-fg-muted">{desc}</div>}
      </div>
      {children !== undefined && <div className={cn("flex shrink-0 items-center gap-2", stack && "w-full")}>{children}</div>}
    </div>
  );
}

export function LoadState({ error, retry, rows = 3 }: { error?: unknown; retry?: () => void; rows?: number }) {
  if (error)
    return (
      <ErrorNote title="Couldn't load these settings" action={retry && <Button size="sm" onClick={retry}>Try again</Button>}>
        {errorText(error)}
      </ErrorNote>
    );
  return (
    <div className="space-y-3" role="status" aria-label="Loading">
      {Array.from({ length: rows }, (_, i) => (
        <Skeleton key={i} className="h-14 rounded-xl" />
      ))}
    </div>
  );
}
