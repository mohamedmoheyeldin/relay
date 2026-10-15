// Small layout pieces shared by Relay's screens, built on Untitled UI components.
import type { CSSProperties, FC, ReactNode } from "react";
import { AlertCircle, AlertTriangle, CheckCircle, XCircle } from "@untitledui/icons";
import { Button } from "@/components/base/buttons/button";
import { ProgressBar } from "@/components/base/progress-indicators/progress-indicators";
import { bytes, percent } from "@/lib/format";
import type { JobState } from "@/lib/jobs";
import { cx } from "@/utils/cx";

/** A small circular spinner in the brand colour. */
export const Spinner = ({ className }: { className?: string }) => (
    <svg viewBox="0 0 20 20" className={cx("size-4 shrink-0 animate-spin", className)} aria-hidden>
        <circle cx="10" cy="10" r="8" fill="none" strokeWidth="2.5" className="stroke-bg-quaternary" />
        <path d="M18 10a8 8 0 0 0-8-8" fill="none" strokeWidth="2.5" strokeLinecap="round" className="stroke-fg-brand-primary" />
    </svg>
);

/** Grey placeholder bars while something loads. */
export const Skeleton = ({ className, style }: { className?: string; style?: CSSProperties }) => (
    <div className={cx("animate-pulse rounded-md bg-quaternary", className)} style={style} />
);

export type CheckStatus = "waiting" | "running" | "ok" | "warn" | "error";

const checkIcon = (status: CheckStatus) => {
    switch (status) {
        case "running":
            return <Spinner className="size-5" />;
        case "ok":
            return <CheckCircle className="size-5 shrink-0 text-fg-success-primary animate-in zoom-in-50 duration-300" />;
        case "warn":
            return <AlertCircle className="size-5 shrink-0 text-fg-warning-primary animate-in zoom-in-50 duration-300" />;
        case "error":
            return <XCircle className="size-5 shrink-0 text-fg-error-primary animate-in zoom-in-50 duration-300" />;
        default:
            return <span className="m-0.5 size-4 shrink-0 rounded-full border-2 border-secondary" />;
    }
};

/** One line of a checklist: status icon, title, detail, and an action or more below. */
export const CheckRow = ({
    status,
    title,
    detail,
    action,
    children,
}: {
    status: CheckStatus;
    title: ReactNode;
    detail?: ReactNode;
    action?: ReactNode;
    children?: ReactNode;
}) => (
    <div className="flex flex-col gap-3 border-b border-secondary px-4 py-3.5 last:border-b-0 sm:px-5">
        <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
            <div className="flex min-w-0 flex-1 items-center gap-3">
                {checkIcon(status)}
                <div className="min-w-0">
                    <div className={cx("text-sm font-semibold", status === "waiting" ? "text-quaternary" : "text-primary")}>{title}</div>
                    {detail && <div className="text-sm break-words text-tertiary">{detail}</div>}
                </div>
            </div>
            {action && <div className="flex shrink-0 flex-wrap gap-2 pl-8 sm:pl-0">{action}</div>}
        </div>
        {children && <div className="pl-8 animate-in fade-in slide-in-from-top-1 duration-200">{children}</div>}
    </div>
);

export const PageHeader = ({ title, description, actions }: { title: string; description?: ReactNode; actions?: ReactNode }) => (
    <div className="flex flex-wrap items-start justify-between gap-4 border-b border-secondary pb-5">
        <div className="min-w-0">
            <h1 className="text-display-xs font-semibold text-primary">{title}</h1>
            {description && <p className="mt-1 text-md text-tertiary">{description}</p>}
        </div>
        {actions && <div className="flex shrink-0 flex-wrap gap-3">{actions}</div>}
    </div>
);

export const Section = ({ title, description, children }: { title: string; description?: ReactNode; children: ReactNode }) => (
    <section className="grid grid-cols-1 gap-4 border-b border-secondary py-6 last:border-b-0 @3xl:grid-cols-[minmax(180px,240px)_1fr] @3xl:gap-8">
        <div>
            <h2 className="text-sm font-semibold text-secondary">{title}</h2>
            {description && <p className="mt-1 text-sm text-tertiary">{description}</p>}
        </div>
        <div className="flex min-w-0 flex-col gap-4">{children}</div>
    </section>
);

export const Card = ({ children, className }: { children: ReactNode; className?: string }) => (
    <div className={cx("rounded-xl bg-primary p-4 shadow-xs ring-1 ring-secondary ring-inset", className)}>{children}</div>
);

export const KeyValue = ({ label, value }: { label: string; value: ReactNode }) => (
    <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 text-sm">
        <span className="text-tertiary">{label}</span>
        <span className="min-w-0 font-medium break-all text-secondary">{value}</span>
    </div>
);

export const Notice = ({ tone = "warning", children }: { tone?: "warning" | "error" | "success"; children: ReactNode }) => {
    const Icon: FC<{ className?: string }> = tone === "success" ? CheckCircle : tone === "error" ? XCircle : AlertTriangle;
    return (
        <div
            className={cx(
                "flex gap-3 rounded-lg p-3 text-sm ring-1 ring-inset",
                tone === "success" && "bg-success-primary text-success-primary ring-secondary",
                tone === "warning" && "bg-warning-primary text-warning-primary ring-secondary",
                tone === "error" && "bg-error-primary text-error-primary ring-error_subtle",
            )}
        >
            <Icon className="mt-0.5 size-4 shrink-0" />
            <div className="min-w-0 break-words">{children}</div>
        </div>
    );
};

export const JobProgress = ({ job, onCancel, label }: { job?: JobState; onCancel?: () => void; label?: string }) => {
    if (!job) return null;
    if (job.state === "error") return <Notice tone="error">{job.error}</Notice>;
    if (job.state === "done") return null;
    return (
        <div className="flex flex-col gap-2">
            <div className="flex items-center justify-between text-sm">
                <span className="text-secondary">
                    {label ?? job.stage ?? "Working"}
                    {job.total > 0 && (
                        <span className="text-tertiary">
                            {" "}
                            · {bytes(job.done)} of {bytes(job.total)}
                        </span>
                    )}
                </span>
                {onCancel && (
                    <Button size="sm" color="link-gray" onClick={onCancel}>
                        Cancel
                    </Button>
                )}
            </div>
            <ProgressBar value={percent(job.done, job.total)} labelPosition="right" />
        </div>
    );
};

export const LogLines = ({ lines }: { lines: string[] }) =>
    lines.length === 0 ? null : (
        <pre className="max-h-48 overflow-auto rounded-lg bg-secondary p-3 font-mono text-xs whitespace-pre-wrap text-secondary">
            {lines.slice(-60).join("\n")}
        </pre>
    );
