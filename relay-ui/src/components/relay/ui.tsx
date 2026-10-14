// Small layout pieces shared by Relay's screens, built on Untitled UI components.
import type { FC, ReactNode } from "react";
import { AlertTriangle, CheckCircle, XCircle } from "@untitledui/icons";
import { Button } from "@/components/base/buttons/button";
import { ProgressBar } from "@/components/base/progress-indicators/progress-indicators";
import { bytes, percent } from "@/lib/format";
import type { JobState } from "@/lib/jobs";
import { cx } from "@/utils/cx";

export const PageHeader = ({ title, description, actions }: { title: string; description?: ReactNode; actions?: ReactNode }) => (
    <div className="flex items-start justify-between gap-4 border-b border-secondary pb-5">
        <div className="min-w-0">
            <h1 className="text-display-xs font-semibold text-primary">{title}</h1>
            {description && <p className="mt-1 text-md text-tertiary">{description}</p>}
        </div>
        {actions && <div className="flex shrink-0 gap-3">{actions}</div>}
    </div>
);

export const Section = ({ title, description, children }: { title: string; description?: ReactNode; children: ReactNode }) => (
    <section className="grid grid-cols-1 gap-4 border-b border-secondary py-6 lg:grid-cols-[minmax(200px,280px)_1fr] lg:gap-8">
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
