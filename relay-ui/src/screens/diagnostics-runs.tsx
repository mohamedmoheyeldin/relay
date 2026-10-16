// The full test run (D-096): real tasks in Notepad, File Explorer, the browser, a web
// form, every monitor, the router, Cloud AI and the local AI alone, recorded under
// Downloads\Relay-diagnostics. Shown on the Developer page of Settings.
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Copy01, File06, Folder, PlayCircle, StopCircle } from "@untitledui/icons";
import { Badge } from "@/components/base/badges/badges";
import { Button } from "@/components/base/buttons/button";
import { Card, Notice, Section, Spinner } from "@/components/relay/ui";
import { type DiagnosticsResult, type DiagnosticsSummary, api, errorText } from "@/lib/api";
import { useJobs } from "@/lib/jobs";

interface Row {
    id: string;
    title: string;
    area?: string;
    result?: DiagnosticsResult;
    live?: string;
}

const statusBadge = (row: Row, running: boolean) => {
    const status = row.result?.status;
    if (status === "passed") return <Badge color="success" size="sm">Passed</Badge>;
    if (status === "failed") return <Badge color="error" size="sm">Failed</Badge>;
    if (status === "skipped") return <Badge color="gray" size="sm">Skipped</Badge>;
    if (running && row.live !== undefined) return <Spinner className="size-4" />;
    return <Badge color="gray" size="sm">Waiting</Badge>;
};

const timing = (r: DiagnosticsResult) => {
    const parts = [`${r.seconds} s`];
    if (r.steps) parts.push(`${r.steps} steps`);
    if (r.model) parts.push(r.model);
    const think = r.timings?.think_ms_total;
    if (typeof think === "number" && think > 0) parts.push(`model ${(think / 1000).toFixed(1)} s`);
    const cloud = r.timings?.cloud_ms;
    if (typeof cloud === "number") parts.push(`Cloud AI ${(cloud / 1000).toFixed(1)} s`);
    return parts.join(" · ");
};

const fromSummary = (s: DiagnosticsSummary): Row[] => s.results.map((r) => ({ id: r.id, title: r.title, area: r.area, result: r }));

/** Rows under their area (the app, real tasks, Cloud AI, system), in run order. */
const byArea = (rows: Row[]) => {
    const groups: { area: string; rows: Row[] }[] = [];
    for (const r of rows) {
        const area = r.area ?? "Tests";
        const last = groups[groups.length - 1];
        if (last?.area === area) last.rows.push(r);
        else groups.push({ area, rows: [r] });
    }
    return groups;
};

const tally = (rows: Row[]) => {
    const n = (status: string) => rows.filter((r) => r.result?.status === status).length;
    const parts = [`${n("passed")} passed`];
    if (n("failed")) parts.push(`${n("failed")} failed`);
    if (n("skipped")) parts.push(`${n("skipped")} skipped`);
    const waiting = rows.length - n("passed") - n("failed") - n("skipped");
    if (waiting) parts.push(`${waiting} to go`);
    return parts.join(" · ");
};

export const DiagnosticsRuns = () => {
    const jobs = useJobs();
    const job = jobs["diagnostics"];
    const running = job?.state === "running";
    const [rows, setRows] = useState<Row[]>([]);
    const [summary, setSummary] = useState<DiagnosticsSummary | null>(null);
    const [error, setError] = useState("");
    const [copied, setCopied] = useState(false);

    useEffect(() => {
        api.diagnosticsLatest().then((s) => {
            if (s) {
                setSummary(s);
                setRows((current) => (current.length ? current : fromSummary(s)));
            }
        });
        const sub = listen<Record<string, unknown>>("diagnostics", ({ payload: e }) => {
            const kind = e.event as string;
            if (kind === "start") {
                const list = (e.scenarios as { id: string; title: string; area?: string }[]) ?? [];
                setRows(list.map((s) => ({ id: s.id, title: s.title, area: s.area })));
                setSummary(null);
            } else if (kind === "scenario") {
                setRows((rs) => rs.map((r) => (r.id === e.scenario ? { ...r, live: "Starting…" } : r)));
            } else if (kind === "progress") {
                const text = (e.say as string) || (e.kind === "tool_use" ? `Cloud AI called ${e.tool}` : "");
                if (text) setRows((rs) => rs.map((r) => (r.id === e.scenario ? { ...r, live: text } : r)));
            } else if (kind === "result") {
                const result = e as unknown as DiagnosticsResult;
                setRows((rs) => rs.map((r) => (r.id === result.id ? { ...r, result, live: undefined } : r)));
            } else if (kind === "summary") {
                setSummary(e as unknown as DiagnosticsSummary);
            }
        });
        return () => {
            sub.then((f) => f());
        };
    }, []);

    const start = async () => {
        setError("");
        try {
            await api.diagnosticsRun();
        } catch (e) {
            setError(errorText(e));
        }
    };

    const copy = async () => {
        if (!summary) return;
        const lines = [
            `Relay diagnostics ${summary.started}: ${summary.passed} passed, ${summary.failed} failed, ${summary.skipped} skipped in ${summary.seconds} s.`,
            ...summary.problems.map((p) => `- ${p}`),
            `Full results: ${summary.folder}\\summary.json`,
        ];
        await navigator.clipboard.writeText(lines.join("\n"));
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
    };

    return (
        <Section
            title="Full test"
            description="Runs everything in one go: Relay's own app, real tasks in Notepad, File Explorer, Brave and web forms, Cloud AI and the local AI on its own, every monitor, and RAM and VRAM. It records screenshots, timings and every step in Downloads\Relay-diagnostics. It opens windows and moves the mouse while it runs; press Ctrl+Alt+Pause to stop it."
        >
            {error && <Notice tone="error">{error}</Notice>}
            {job?.state === "error" && job.error && job.error !== "Stopped." && <Notice tone="error">{job.error}</Notice>}
            {summary && !running && (
                <Notice tone={summary.failed || summary.resources?.over_budget.length ? "warning" : "success"}>
                    {summary.passed} passed, {summary.failed} failed, {summary.skipped} skipped in {Math.round(summary.seconds)} s ({summary.started}).
                    {summary.resources && ` Relay used up to ${(summary.resources.peak_relay_ram_mb / 1024).toFixed(1)} GB of RAM.`}
                    {summary.resources?.over_budget.map((line) => (
                        <span key={line} className="block">
                            {line}
                        </span>
                    ))}
                </Notice>
            )}
            {byArea(rows).map((group) => (
                <div key={group.area} className="flex flex-col gap-2">
                    <div className="flex items-baseline justify-between gap-3 px-1">
                        <p className="text-sm font-semibold text-primary">{group.area}</p>
                        <p className="text-xs text-tertiary">{tally(group.rows)}</p>
                    </div>
                        <Card className="py-1">
                            {group.rows.map((r) => (
                                <div key={r.id} className="flex items-start justify-between gap-3 border-b border-secondary py-3 last:border-b-0">
                                    <div className="min-w-0">
                                        <p className="text-sm font-medium text-secondary">{r.title}</p>
                                        {r.result ? (
                                            <>
                                                <p className="text-xs text-tertiary">{timing(r.result)}</p>
                                                {r.result.status !== "passed" && (
                                                    <p className="text-xs break-words text-error-primary">
                                                        {r.result.failed_checks.length ? r.result.failed_checks.join("; ") : r.result.message}
                                                    </p>
                                                )}
                                            </>
                                        ) : (
                                            r.live && running && <p className="truncate text-xs text-tertiary">{r.live}</p>
                                        )}
                                    </div>
                                    <div className="shrink-0">{statusBadge(r, running)}</div>
                                </div>
                            ))}
                        </Card>
                </div>
            ))}
            <div className="flex flex-wrap gap-2">
                {running ? (
                    <Button size="sm" color="secondary" iconLeading={StopCircle} onClick={() => api.diagnosticsStop()}>
                        Stop the test
                    </Button>
                ) : (
                    <Button size="sm" iconLeading={PlayCircle} onClick={start}>
                        Run full test
                    </Button>
                )}
                {summary && (
                    <>
                        <Button size="sm" color="secondary" iconLeading={File06} onClick={() => api.openPath(summary.report)}>
                            Open report
                        </Button>
                        <Button size="sm" color="secondary" iconLeading={Folder} onClick={() => api.openPath(summary.folder)}>
                            Open results folder
                        </Button>
                        <Button size="sm" color="secondary" iconLeading={Copy01} onClick={copy}>
                            {copied ? "Copied" : "Copy for Cloud AI"}
                        </Button>
                    </>
                )}
            </div>
        </Section>
    );
};
