import type { ReactNode } from "react";
import { cn } from "../components/ui";

export function PageTitle({ children }: { children: ReactNode }) {
  return <h1 className="mb-8 text-[26px] leading-9 font-normal">{children}</h1>;
}

export function SubTitle({ children, className }: { children: ReactNode; className?: string }) {
  return <h2 className={cn("mt-8 mb-3 text-[15px] font-semibold first:mt-0", className)}>{children}</h2>;
}

export function Card({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn("divide-y divide-[var(--border)] rounded-2xl border border-line px-4", className)}>{children}</div>;
}

export function Row({ label, desc, children, stack, htmlFor }: { label: ReactNode; desc?: ReactNode; children?: ReactNode; stack?: boolean; htmlFor?: string }) {
  return (
    <div className={cn("flex gap-4 py-3.5", stack ? "flex-col items-stretch" : "items-center")}>
      <div className="min-w-0 flex-1">
        {htmlFor ? (
          <label htmlFor={htmlFor} className="block text-[14px] font-medium">
            {label}
          </label>
        ) : (
          <div className="text-[14px] font-medium">{label}</div>
        )}
        {desc && <div className="mt-0.5 text-[13px] leading-5 text-subtle">{desc}</div>}
      </div>
      {children !== undefined && <div className={cn("flex shrink-0 items-center gap-2", stack && "w-full")}>{children}</div>}
    </div>
  );
}

export function Segmented<T extends string>({ value, options, onChange, label }: { value: T | undefined; options: { id: T; label: string }[]; onChange: (v: T) => void; label: string }) {
  return (
    <div role="radiogroup" aria-label={label} className="inline-flex rounded-full bg-hover p-0.5">
      {options.map((o) => (
        <button
          key={o.id}
          type="button"
          role="radio"
          aria-checked={value === o.id}
          onClick={() => onChange(o.id)}
          className={cn(
            "h-8 rounded-full px-3.5 text-[13px] whitespace-nowrap transition-colors duration-150",
            value === o.id ? "bg-elevated text-fg shadow-sm" : "text-muted hover:text-fg",
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Pill({ tone = "neutral", children }: { tone?: "ok" | "bad" | "neutral" | "accent"; children: ReactNode }) {
  return (
    <span
      className={cn(
        "inline-flex h-6 items-center gap-1.5 rounded-full px-2.5 text-[12px] font-medium whitespace-nowrap",
        tone === "ok" && "bg-[color-mix(in_srgb,var(--success)_15%,transparent)] text-success",
        tone === "bad" && "bg-danger-soft text-danger",
        tone === "neutral" && "bg-hover text-muted",
        tone === "accent" && "bg-accent-soft text-accent",
      )}
    >
      {children}
    </span>
  );
}

export function ProgressBar({ percent, label }: { percent: number | null; label: string }) {
  const p = percent === null ? null : Math.max(0, Math.min(100, percent));
  return (
    <div>
      <div className="mb-1.5 flex items-center justify-between gap-3 text-[13px]">
        <span className="truncate text-muted">{label}</span>
        {p !== null && <span className="shrink-0 tabular-nums text-subtle">{Math.round(p)}%</span>}
      </div>
      <div
        className="h-1.5 overflow-hidden rounded-full bg-hover"
        role="progressbar"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={p ?? undefined}
      >
        {p === null ? (
          <div className="h-full w-1/3 animate-[relay-indeterminate_1.2s_ease-in-out_infinite] rounded-full bg-accent" />
        ) : (
          <div className="h-full origin-left rounded-full bg-accent transition-transform duration-300 ease-out" style={{ transform: `scaleX(${p / 100})` }} />
        )}
      </div>
    </div>
  );
}
