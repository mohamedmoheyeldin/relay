// Checklist rows the wizard runs on its own: GPU and the suggested local model,
// the browser lane, and the rest of the self-test.
import { useCallback, useEffect, useRef, useState } from "react";
import { Download01, Globe02, RefreshCw01 } from "@untitledui/icons";
import { Button } from "@/components/base/buttons/button";
import { Select } from "@/components/base/select/select";
import { CheckRow, type CheckStatus, JobProgress, Notice } from "@/components/relay/ui";
import { type BrowserState, type Check, type Settings, api, errorText } from "@/lib/api";
import { useClaude } from "@/lib/claude";
import { bytes, gb } from "@/lib/format";
import { useJobs } from "@/lib/jobs";
import { downloadRecommended, downloadable, isInstalled, modelJob, runtimeJob, useLocalAi } from "./local-ai";

/** `autoDownload` starts the suggested download on its own, once (the wizard). */
export const LocalAiChecks = ({ autoDownload = false }: { autoDownload?: boolean }) => {
    const { state, refresh, error } = useLocalAi();
    const jobs = useJobs();
    const [busy, setBusy] = useState(false);
    const [failed, setFailed] = useState<string | null>(null);
    const started = useRef(false);

    if (error) return <CheckRow status="warn" title="Your GPU" detail={error} />;
    const gpu: CheckStatus = !state ? "running" : state.gpu ? "ok" : "warn";
    const picks = state?.recommendation.picks ?? [];
    const available = state ? downloadable(state) : [];
    const pending = state ? available.filter((p) => !isInstalled(state, p.model_id)) : [];
    const ready = !!state && !!state.runtime && available.length > 0 && pending.length === 0;
    const size = pending.reduce(
        (sum, p) => sum + (state?.models.find((m) => m.id === p.model_id)?.quants.find((q) => q.quant === p.quant)?.download_bytes ?? 0),
        0,
    );
    const label = (id: string, quant: string) => {
        const m = state?.models.find((x) => x.id === id);
        return m ? `${m.name} (${quant})` : id;
    };
    const names = (available.length ? available : picks).map((p) => label(p.model_id, p.quant)).join(" and ");
    const later = picks.filter((p) => !available.includes(p)).map((p) => state?.models.find((x) => x.id === p.model_id)?.name ?? p.model_id);
    const what = pending.length ? `About ${bytes(size)} to download, plus the runtime if needed.` : "Needs the llama.cpp runtime for your GPU (a small download).";
    const model: CheckStatus = !state ? "waiting" : busy ? "running" : ready ? "ok" : "warn";
    const download = async () => {
        if (!state) return;
        setBusy(true);
        setFailed(null);
        try {
            await downloadRecommended(state);
        } catch (e) {
            setFailed(errorText(e));
        }
        await refresh();
        setBusy(false);
    };
    const running = [runtimeJob, ...picks.map((p) => modelJob(p.model_id))].filter((id) => jobs[id]?.state === "running");
    // The local model is what runs tasks when Claude can't, so setup gets it without a click.
    const auto = autoDownload && !!state && !state.error && !ready && available.length > 0 && running.length === 0;
    useEffect(() => {
        if (!auto || started.current) return;
        started.current = true;
        download();
    }, [auto]);

    return (
        <>
            <CheckRow
                status={gpu}
                title="Your GPU"
                detail={
                    !state
                        ? "Checking your graphics card…"
                        : state.gpu
                          ? `${state.gpu.name} · ${gb(state.gpu.vram_total_mb)} video memory, ${gb(state.gpu.vram_free_mb)} free`
                          : "No NVIDIA GPU found. Relay still works: Claude reads the screen itself."
                }
            />
            <CheckRow
                status={model}
                title={picks.length ? `Suggested model: ${names}` : "Suggested model"}
                detail={
                    !state
                        ? "After the GPU check."
                        : state.error
                          ? state.error
                          : ready
                            ? "Downloaded and in use."
                            : picks.length === 0
                              ? state.recommendation.reason
                              : available.length === 0
                                ? "Its download source comes with the benchmark. Relay works without it."
                                : `${what}${later.length ? ` ${later.join(" and ")} comes later, with the benchmark.` : ""}`
                }
                action={
                    state &&
                    !ready &&
                    available.length > 0 &&
                    !state.error && (
                        <Button size="sm" iconLeading={Download01} onClick={download} isLoading={busy} showTextWhileLoading>
                            {busy ? "Downloading" : pending.length ? "Download" : "Get runtime"}
                        </Button>
                    )
                }
            >
                {(running.length > 0 || failed) && (
                    <div className="flex flex-col gap-3">
                        {running.map((id) => (
                            <JobProgress key={id} job={jobs[id]} onCancel={() => api.cancelJob(id)} />
                        ))}
                        {failed && <Notice tone="error">{failed}</Notice>}
                    </div>
                )}
            </CheckRow>
        </>
    );
};

export function useBrowserLane() {
    const [state, setState] = useState<BrowserState | null>(null);
    const refresh = useCallback(async () => setState(await api.browserState()), []);
    useEffect(() => {
        refresh();
    }, [refresh]);
    return { state, refresh };
}

export const BrowserCheck = ({ settings, save }: { settings: Settings; save: (s: Settings) => Promise<void> }) => {
    const { state, refresh } = useBrowserLane();
    const [error, setError] = useState<string | null>(null);
    const chromium = state?.browsers.filter((b) => b.chromium) ?? [];
    const status: CheckStatus = !state ? "running" : !state.enabled ? "warn" : state.problem ? "warn" : "ok";
    const choose = async (name: string) => {
        await save({ ...settings, browser: { ...settings.browser, preferred: name } });
        await refresh();
    };
    return (
        <CheckRow
            status={status}
            title={state?.browser ? `Browser: ${state.browser.name}` : "Browser"}
            detail={
                !state
                    ? "Looking for your browsers…"
                    : !state.enabled
                      ? "The browser lane is off. Turn it on in Settings > Browser."
                      : (state.problem ??
                        "Claude browses in its own profile of this browser. Sign in to your sites there once and they stay signed in.")
            }
            action={
                state?.browser &&
                state.enabled && (
                    <Button
                        size="sm"
                        color="secondary"
                        iconLeading={Globe02}
                        onClick={() => api.browserOpen().catch((e) => setError(errorText(e)))}
                    >
                        Open to sign in
                    </Button>
                )
            }
        >
            {(chromium.length > 1 || error) && (
                <div className="flex max-w-sm flex-col gap-3">
                    {chromium.length > 1 && (
                        <Select
                            size="sm"
                            aria-label="Browser"
                            selectedKey={settings.browser.preferred}
                            onSelectionChange={(k) => choose(String(k))}
                            items={[{ id: "auto", label: "Pick for me" }, ...chromium.map((b) => ({ id: b.name, label: b.name }))]}
                        >
                            {(item) => <Select.Item id={item.id} label={item.label} />}
                        </Select>
                    )}
                    {error && <Notice tone="error">{error}</Notice>}
                </div>
            )}
        </CheckRow>
    );
};

// Shown elsewhere in the wizard, so left out of its self-test list.
const SHOWN_ELSEWHERE = new Set(["Relay", "Claude Code installed", "Claude Code signed in", "GPU", "Browser lane", "Settings file"]);

export const SelfTest = () => {
    const { check } = useClaude();
    const [checks, setChecks] = useState<Check[] | null>(null);
    const [running, setRunning] = useState(false);
    const run = useCallback(async () => {
        setRunning(true);
        try {
            setChecks((await api.doctor()).checks.filter((c) => !SHOWN_ELSEWHERE.has(c.name)));
        } finally {
            setRunning(false);
        }
    }, []);
    // Run once the Claude check has settled: it registers relay-screen first.
    const settled = check.status === "ok" || check.status === "error";
    useEffect(() => {
        run();
    }, [run, settled]);
    if (!checks) return <CheckRow status="running" title="Relay's own parts" detail="Running the self-test…" />;
    return (
        <>
            {checks.map((c, i) => (
                <CheckRow
                    key={c.name}
                    status={running ? "running" : c.ok ? "ok" : "warn"}
                    title={c.name}
                    detail={c.detail}
                    action={
                        i === checks.length - 1 && (
                            <Button size="sm" color="secondary" iconLeading={RefreshCw01} onClick={run} isDisabled={running}>
                                Run again
                            </Button>
                        )
                    }
                />
            ))}
        </>
    );
};
