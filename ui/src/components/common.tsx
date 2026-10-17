import { memo, type ReactNode } from "react";
import { Folder } from "lucide-react";
import { cn } from "@/lib/utils";
import { folderColor } from "@/lib/theme";
import { initials } from "@/lib/grouping";

/** Relay's own mark: two arcs passing a point between them (a relay hand-off). Original artwork. */
export const RelayMark = memo(function RelayMark({ size = 22, className, title }: { size?: number; className?: string; title?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" role={title ? "img" : undefined} aria-hidden={title ? undefined : true} className={className}>
      {title && <title>{title}</title>}
      <g fill="none" stroke="var(--relay-accent)" strokeWidth="2.4" strokeLinecap="round">
        <path d="M4.5 15.5a7.5 7.5 0 0 1 7.5-7.5" />
        <path d="M19.5 8.5a7.5 7.5 0 0 1-7.5 7.5" />
      </g>
      <circle cx="12" cy="12" r="2.1" fill="var(--relay-accent)" />
    </svg>
  );
});

export function FolderIcon({ color, size = 16, className }: { color: string; size?: number; className?: string }) {
  return <Folder size={size} strokeWidth={1.8} style={{ color: folderColor(color) }} aria-hidden="true" className={cn("shrink-0", className)} />;
}

export function Avatar({ name, size = 28, className }: { name: string; size?: number; className?: string }) {
  return (
    <span
      aria-hidden="true"
      className={cn("inline-flex shrink-0 items-center justify-center rounded-full bg-brand-soft font-semibold text-brand-text select-none", className)}
      style={{ width: size, height: size, fontSize: Math.round(size * 0.38) }}
    >
      {initials(name)}
    </span>
  );
}

/** A small coloured dot plus (always) a text label elsewhere; colour is never the only signal. */
export function StatusDot({ tone, className }: { tone: "ok" | "warn" | "bad" | "idle" | "busy"; className?: string }) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "inline-block size-2 shrink-0 rounded-full",
        tone === "ok" && "bg-success",
        tone === "warn" && "bg-warning",
        tone === "bad" && "bg-danger",
        tone === "idle" && "bg-fg-muted/60",
        tone === "busy" && "bg-brand pulse-dot size-2",
        className,
      )}
    />
  );
}

export function EmptyState({ icon, title, children, action, className }: { icon?: ReactNode; title: string; children?: ReactNode; action?: ReactNode; className?: string }) {
  return (
    <div className={cn("flex flex-col items-center rounded-xl border border-dashed border-line px-6 py-12 text-center", className)}>
      {icon && <div className="mb-3 text-fg-muted [&_svg]:size-6">{icon}</div>}
      <p className="text-ui font-medium text-fg">{title}</p>
      {children && <p className="mt-1 max-w-sm text-caption leading-5 text-fg-muted">{children}</p>}
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}

export function ErrorNote({ title, children, action }: { title: string; children?: ReactNode; action?: ReactNode }) {
  return (
    <div role="alert" className="rounded-lg border border-danger/25 bg-danger-soft px-4 py-3 text-ui">
      <div className="font-medium text-fg">{title}</div>
      {children && <div className="mt-0.5 text-caption leading-5 text-fg-2">{children}</div>}
      {action && <div className="mt-2.5">{action}</div>}
    </div>
  );
}

/** Inline code (commands the user should type, paths). */
export function Code({ children }: { children: ReactNode }) {
  return <code className="rounded-xs bg-sunk px-1.5 py-0.5 font-mono text-[12.5px] text-fg">{children}</code>;
}
