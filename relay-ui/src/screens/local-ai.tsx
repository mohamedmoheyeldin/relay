// Settings > Local AI: the model picker (D-046), and the wizard's "recommended" step.
import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { CpuChip01, Download01, Folder, Plus, Stars01, Trash01 } from "@untitledui/icons";
import { Badge } from "@/components/base/badges/badges";
import { Button } from "@/components/base/buttons/button";
import { Input } from "@/components/base/input/input";
import { Select } from "@/components/base/select/select";
import { Card, JobProgress, KeyValue, Notice, Section } from "@/components/relay/ui";
import { type Fit, type LocalAiState, type ModelView, type Settings, api, errorText } from "@/lib/api";
import { bytes, gb, roleLabel } from "@/lib/format";
import { clearJob, useJobs, waitForJob } from "@/lib/jobs";

export function useLocalAi() {
    const [state, setState] = useState<LocalAiState | null>(null);
    const [error, setError] = useState<string | null>(null);
    const refresh = useCallback(async () => {
        try {
            setState(await api.localAi());
        } catch (e) {
            setError(errorText(e));
        }
    }, []);
    useEffect(() => {
        refresh();
    }, [refresh]);
    return { state, refresh, error };
}

const fitBadge = (fit: Fit) => {
    switch (fit) {
        case "fits":
            return <Badge color="success" size="sm">Fits</Badge>;
        case "tight":
            return <Badge color="warning" size="sm">Tight: not with the other model</Badge>;
        case "wont_fit":
            return <Badge color="error" size="sm">Won't fit</Badge>;
        default:
            return <Badge color="gray" size="sm">No NVIDIA GPU</Badge>;
    }
};

export const GpuCard = ({ state }: { state: LocalAiState }) => (
    <Card className="flex flex-col gap-2">
        <div className="flex items-center gap-2 text-md font-semibold text-primary">
            <CpuChip01 className="size-5 text-fg-quaternary" />
            {state.gpu ? state.gpu.name : "No NVIDIA GPU found"}
        </div>
        {state.gpu && (
            <>
                <KeyValue label="Video memory" value={`${gb(state.gpu.vram_total_mb)} (${gb(state.gpu.vram_free_mb)} free now)`} />
                <KeyValue label="Driver" value={state.gpu.driver} />
            </>
        )}
        <p className="text-sm text-tertiary">{state.recommendation.reason}</p>
    </Card>
);

/** Downloads the runtime (if needed) and every recommended model, in parallel. */
export async function downloadRecommended(state: LocalAiState) {
    const waits: Promise<void>[] = [];
    if (!state.runtime.installed) {
        clearJob("runtime");
        await api.runtimeInstall();
        waits.push(waitForJob("runtime"));
    }
    for (const p of state.recommendation.picks) {
        const installed = state.installed.some((i) => i.model_id === p.model_id && i.quant === p.quant);
        if (installed) {
            await api.modelActivate(p.role, p.model_id, p.quant);
            continue;
        }
        const id = `model:${p.model_id}:${p.quant}`;
        clearJob(id);
        await api.modelInstall(p.model_id, p.quant);
        waits.push(waitForJob(id));
    }
    await Promise.all(waits);
    // Make the recommendation the active choice even if other models were set before.
    for (const p of state.recommendation.picks) await api.modelActivate(p.role, p.model_id, p.quant);
}

export const RecommendedDownloads = ({ state, onDone }: { state: LocalAiState; onDone: () => void }) => {
    const jobs = useJobs();
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const picks = state.recommendation.picks;
    const allInstalled =
        state.runtime.installed && picks.every((p) => state.installed.some((i) => i.model_id === p.model_id && i.quant === p.quant));
    const total =
        (state.runtime.installed ? 0 : state.runtime_download_bytes) +
        picks
            .filter((p) => !state.installed.some((i) => i.model_id === p.model_id && i.quant === p.quant))
            .reduce((sum, p) => sum + (state.models.find((m) => m.id === p.model_id)?.quants.find((q) => q.quant === p.quant)?.download_bytes ?? 0), 0);

    if (picks.length === 0) return null;
    const start = async () => {
        setBusy(true);
        setError(null);
        try {
            await downloadRecommended(state);
            onDone();
        } catch (e) {
            setError(errorText(e));
        }
        setBusy(false);
    };
    return (
        <Card className="flex flex-col gap-3">
            <div className="flex items-center gap-2 text-md font-semibold text-primary">
                <Stars01 className="size-5 text-fg-brand-primary" />
                Recommended for this PC
            </div>
            {picks.map((p) => {
                const m = state.models.find((x) => x.id === p.model_id);
                return (
                    <div key={p.model_id} className="flex flex-col gap-2">
                        <div className="flex items-center justify-between text-sm">
                            <span className="font-medium text-secondary">
                                {m?.name} <span className="text-tertiary">· {p.quant}</span>
                            </span>
                            <Badge size="sm" color="brand">
                                {roleLabel[p.role]}
                            </Badge>
                        </div>
                        <JobProgress job={jobs[`model:${p.model_id}:${p.quant}`]} onCancel={() => api.cancelJob(`model:${p.model_id}:${p.quant}`)} />
                    </div>
                );
            })}
            {!state.runtime.installed && (
                <div className="flex flex-col gap-2">
                    <div className="text-sm font-medium text-secondary">
                        Local AI runtime <span className="text-tertiary">· llama.cpp {state.runtime.build} ({state.runtime_flavor === "cuda" ? "CUDA" : "Vulkan"})</span>
                    </div>
                    <JobProgress job={jobs.runtime} onCancel={() => api.cancelJob("runtime")} />
                </div>
            )}
            {allInstalled ? (
                <Notice tone="success">Everything is downloaded and in use.</Notice>
            ) : (
                <div className="flex items-center gap-3">
                    <Button iconLeading={Download01} onClick={start} isLoading={busy} showTextWhileLoading>
                        Download ({bytes(total)})
                    </Button>
                    <span className="text-sm text-tertiary">Downloads resume if they are interrupted.</span>
                </div>
            )}
            {error && <Notice tone="error">{error}</Notice>}
        </Card>
    );
};

const ModelCard = ({ model, state, onChange }: { model: ModelView; state: LocalAiState; onChange: () => void }) => {
    const jobs = useJobs();
    const [quant, setQuant] = useState(model.recommended_quant ?? model.default_quant);
    const [error, setError] = useState<string | null>(null);
    const q = model.quants.find((x) => x.quant === quant) ?? model.quants[0];
    const jobId = `model:${model.id}:${q.quant}`;
    const job = jobs[jobId];

    const act = async (f: () => Promise<unknown>) => {
        setError(null);
        try {
            await f();
        } catch (e) {
            setError(errorText(e));
        }
        onChange();
    };
    const download = () =>
        act(async () => {
            clearJob(jobId);
            await api.modelInstall(model.id, q.quant);
            await waitForJob(jobId);
        });
    const isActive = (role: string) =>
        (role === "grounding" ? state.grounding : state.describe)?.model_id === model.id &&
        (role === "grounding" ? state.grounding : state.describe)?.quant === q.quant;
    const roles = model.role === "both" ? ["grounding", "describe"] : [model.role];

    return (
        <Card className="flex flex-col gap-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex flex-wrap items-center gap-2">
                    <span className="text-md font-semibold text-primary">{model.name}</span>
                    <Badge size="sm" color="gray">
                        {roleLabel[model.role]}
                    </Badge>
                    {model.recommended_quant && (
                        <Badge size="sm" color="brand">
                            Recommended
                        </Badge>
                    )}
                    {model.active_as.map((r) => (
                        <Badge key={r} size="sm" color="success">
                            In use: {roleLabel[r]}
                        </Badge>
                    ))}
                </div>
                {fitBadge(q.fit)}
            </div>
            <p className="text-sm text-tertiary">{model.summary}</p>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-[180px_1fr] sm:items-end">
                <Select
                    label="Quality"
                    size="sm"
                    selectedKey={q.quant}
                    onSelectionChange={(k) => setQuant(String(k))}
                    items={model.quants.map((x) => ({ id: x.quant, label: x.quant, supportingText: x.installed ? "downloaded" : undefined }))}
                >
                    {(item) => <Select.Item id={item.id} label={item.label} supportingText={item.supportingText} />}
                </Select>
                <div className="flex flex-col gap-1 text-sm text-tertiary">
                    <span>
                        Download {bytes(q.download_bytes)} · needs about {bytes(q.vram_bytes)} of VRAM
                    </span>
                    <span>
                        Benchmark on your screens:{" "}
                        {model.benchmark ? `${model.benchmark.score.toFixed(1)} · ${model.benchmark.latency_ms} ms` : "not run yet"}
                    </span>
                </div>
            </div>
            <JobProgress job={job} onCancel={() => api.cancelJob(jobId)} />
            <div className="flex flex-wrap gap-2">
                {!q.installed && (
                    <Button size="sm" iconLeading={Download01} onClick={download} isDisabled={job?.state === "running" || q.fit === "wont_fit"}>
                        Download
                    </Button>
                )}
                {q.installed &&
                    roles.map((r) => (
                        <Button
                            key={r}
                            size="sm"
                            color={isActive(r) ? "secondary" : "primary"}
                            isDisabled={isActive(r)}
                            onClick={() => act(() => api.modelActivate(r, model.id, q.quant))}
                        >
                            {isActive(r) ? `Using for ${roleLabel[r].toLowerCase()}` : `Use for ${roleLabel[r].toLowerCase()}`}
                        </Button>
                    ))}
                {q.installed && (
                    <Button size="sm" color="tertiary-destructive" iconLeading={Trash01} onClick={() => act(() => api.modelDelete(model.id, q.quant))}>
                        Delete
                    </Button>
                )}
                <Button size="sm" color="link-gray" isDisabled>
                    Run benchmark (coming with the benchmark tool)
                </Button>
            </div>
            {error && <Notice tone="error">{error}</Notice>}
        </Card>
    );
};

const CustomModels = ({ state, onChange }: { state: LocalAiState; onChange: () => void }) => {
    const jobs = useJobs();
    const [name, setName] = useState("");
    const [role, setRole] = useState("grounding");
    const [modelUrl, setModelUrl] = useState("");
    const [mmprojUrl, setMmprojUrl] = useState("");
    const [error, setError] = useState<string | null>(null);

    const run = async (f: () => Promise<unknown>) => {
        setError(null);
        try {
            await f();
        } catch (e) {
            setError(errorText(e));
        }
        onChange();
    };
    const pick = async (title: string) => {
        const f = await openDialog({ title, filters: [{ name: "GGUF", extensions: ["gguf"] }] });
        return typeof f === "string" ? f : null;
    };
    const fromDisk = () =>
        run(async () => {
            const m = await pick("Choose the model .gguf");
            if (!m) return;
            const p = await pick("Choose the vision projector (mmproj) .gguf");
            if (!p) return;
            await api.customAddDisk(name || "Custom model", role, m, p);
        });
    const fromHf = () =>
        run(async () => {
            clearJob("custom");
            await api.customAddHf(name || "Custom model", role, modelUrl, mmprojUrl);
            await waitForJob("custom");
        });

    return (
        <div className="flex flex-col gap-4">
            {state.custom.map((c) => (
                <Card key={c.id} className="flex flex-wrap items-center justify-between gap-3">
                    <div className="min-w-0">
                        <div className="text-sm font-semibold text-primary">{c.name}</div>
                        <div className="truncate text-xs text-tertiary">{c.model_path}</div>
                    </div>
                    <div className="flex gap-2">
                        {(c.role === "both" ? ["grounding", "describe"] : [c.role]).map((r) => (
                            <Button key={r} size="sm" color="secondary" onClick={() => run(() => api.modelActivate(r, c.id, "custom"))}>
                                Use for {roleLabel[r].toLowerCase()}
                            </Button>
                        ))}
                        <Button size="sm" color="tertiary-destructive" iconLeading={Trash01} onClick={() => run(() => api.customRemove(c.id))}>
                            Remove
                        </Button>
                    </div>
                </Card>
            ))}
            <Card className="flex flex-col gap-3">
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                    <Input label="Name" placeholder="My model" value={name} onChange={setName} size="sm" />
                    <Select
                        label="Role"
                        size="sm"
                        selectedKey={role}
                        onSelectionChange={(k) => setRole(String(k))}
                        items={[
                            { id: "grounding", label: roleLabel.grounding },
                            { id: "describe", label: roleLabel.describe },
                            { id: "both", label: roleLabel.both },
                        ]}
                    >
                        {(item) => <Select.Item id={item.id} label={item.label} />}
                    </Select>
                </div>
                <Input label="Model file link (Hugging Face)" placeholder="https://huggingface.co/…/resolve/main/model.gguf" value={modelUrl} onChange={setModelUrl} size="sm" />
                <Input label="Vision projector link (mmproj)" placeholder="https://huggingface.co/…/resolve/main/mmproj.gguf" value={mmprojUrl} onChange={setMmprojUrl} size="sm" />
                <JobProgress job={jobs.custom} onCancel={() => api.cancelJob("custom")} />
                <div className="flex flex-wrap gap-2">
                    <Button size="sm" iconLeading={Download01} onClick={fromHf} isDisabled={!modelUrl || !mmprojUrl || jobs.custom?.state === "running"}>
                        Download from Hugging Face
                    </Button>
                    <Button size="sm" color="secondary" iconLeading={Plus} onClick={fromDisk}>
                        Add files from this PC
                    </Button>
                </div>
                {error && <Notice tone="error">{error}</Notice>}
            </Card>
        </div>
    );
};

export const LocalAiPage = ({ settings, save }: { settings: Settings; save: (s: Settings) => Promise<void> }) => {
    const { state, refresh, error } = useLocalAi();
    const jobs = useJobs();
    if (error) return <Notice tone="error">{error}</Notice>;
    if (!state) return <Card>Checking your GPU and models…</Card>;

    const chooseFolder = async () => {
        const dir = await openDialog({ directory: true, title: "Folder for local models" });
        if (typeof dir === "string") {
            await save({ ...settings, local_ai: { ...settings.local_ai, models_dir: dir } });
            refresh();
        }
    };

    return (
        <>
            <Section title="This PC" description="Relay checks your GPU so it only suggests models that fit in video memory.">
                <GpuCard state={state} />
                <RecommendedDownloads state={state} onDone={refresh} />
            </Section>
            <Section title="Models" description="Pick which model finds things on screen and which one checks them. Switching takes effect on the next task.">
                {state.models.map((m) => (
                    <ModelCard key={m.id} model={m} state={state} onChange={refresh} />
                ))}
            </Section>
            <Section title="Custom models" description="Add any GGUF vision model with its projector, from Hugging Face or from disk.">
                <CustomModels state={state} onChange={refresh} />
            </Section>
            <Section title="Runtime" description="llama.cpp runs the models on your GPU. Relay downloads a pinned, checked build.">
                <Card className="flex flex-col gap-2">
                    <KeyValue label="Build" value={`llama.cpp ${state.runtime.build}`} />
                    <KeyValue label="Status" value={state.runtime.installed ? `Installed (${state.runtime.flavor})` : "Not downloaded"} />
                    {!state.runtime.installed && (
                        <div>
                            <Button
                                size="sm"
                                iconLeading={Download01}
                                isDisabled={jobs.runtime?.state === "running"}
                                onClick={async () => {
                                    clearJob("runtime");
                                    await api.runtimeInstall();
                                    await waitForJob("runtime").catch(() => {});
                                    refresh();
                                }}
                            >
                                Download runtime ({bytes(state.runtime_download_bytes)})
                            </Button>
                        </div>
                    )}
                    <JobProgress job={jobs.runtime} onCancel={() => api.cancelJob("runtime")} />
                </Card>
            </Section>
            <Section title="Storage and memory" description="Where models are kept, and when they leave video memory.">
                <Card className="flex flex-col gap-3">
                    <KeyValue label="Models folder" value={state.models_dir} />
                    <KeyValue label="Space used" value={bytes(state.models_dir_bytes)} />
                    <div className="flex flex-wrap gap-2">
                        <Button size="sm" color="secondary" iconLeading={Folder} onClick={() => api.openPath(state.models_dir)}>
                            Open folder
                        </Button>
                        <Button size="sm" color="secondary" onClick={chooseFolder}>
                            Change folder
                        </Button>
                        {settings.local_ai.models_dir && (
                            <Button
                                size="sm"
                                color="link-gray"
                                onClick={async () => {
                                    await save({ ...settings, local_ai: { ...settings.local_ai, models_dir: null } });
                                    refresh();
                                }}
                            >
                                Use the default folder
                            </Button>
                        )}
                    </div>
                    <p className="text-xs text-tertiary">Changing the folder doesn't move models already downloaded.</p>
                </Card>
                <Select
                    label="Unload models after a task"
                    size="sm"
                    selectedKey={String(settings.local_ai.idle_unload_secs)}
                    onSelectionChange={(k) => save({ ...settings, local_ai: { ...settings.local_ai, idle_unload_secs: Number(k) } })}
                    items={[
                        { id: "30", label: "30 seconds" },
                        { id: "60", label: "60 seconds (recommended)" },
                        { id: "300", label: "5 minutes" },
                        { id: "900", label: "15 minutes" },
                    ]}
                >
                    {(item) => <Select.Item id={item.id} label={item.label} />}
                </Select>
            </Section>
        </>
    );
};
