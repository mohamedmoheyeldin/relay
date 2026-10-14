// First-run setup (D-044): GPU check, Claude Code, local model, browser, self-test.
import { useEffect, useState } from "react";
import { ArrowRight, CheckCircle, CpuChip01, Globe01, Stars01, XCircle, Zap } from "@untitledui/icons";
import { Button } from "@/components/base/buttons/button";
import { Select } from "@/components/base/select/select";
import { FeaturedIcon } from "@/components/foundations/featured-icon/featured-icon";
import { Card, Notice } from "@/components/relay/ui";
import { type Check, type Settings, api } from "@/lib/api";
import { cx } from "@/utils/cx";
import { ClaudeSetup } from "./claude-setup";
import { GpuCard, RecommendedDownloads, useLocalAi } from "./local-ai";

const STEPS = [
    { id: "welcome", label: "Welcome", icon: Zap },
    { id: "claude", label: "Claude Code", icon: Stars01 },
    { id: "model", label: "Local AI", icon: CpuChip01 },
    { id: "browser", label: "Browser", icon: Globe01 },
    { id: "test", label: "Self-test", icon: CheckCircle },
] as const;

const ModelStep = () => {
    const { state, refresh, error } = useLocalAi();
    if (error) return <Notice tone="error">{error}</Notice>;
    if (!state) return <Card>Checking your GPU…</Card>;
    return (
        <div className="flex flex-col gap-4">
            <GpuCard state={state} />
            <RecommendedDownloads state={state} onDone={refresh} />
            <p className="text-sm text-tertiary">
                You can keep going while the models download, or pick other models later in Settings &gt; Local AI. Nothing downloads until you
                press Download.
            </p>
        </div>
    );
};

const BrowserStep = ({ settings, save }: { settings: Settings; save: (s: Settings) => Promise<void> }) => {
    const [browsers, setBrowsers] = useState<{ name: string; path: string }[]>([]);
    useEffect(() => {
        api.browsers().then(setBrowsers);
    }, []);
    return (
        <div className="flex flex-col gap-4">
            <Card className="flex flex-col gap-3">
                <Select
                    label="Browser for web tasks"
                    size="sm"
                    selectedKey={settings.browser.preferred}
                    onSelectionChange={(k) => save({ ...settings, browser: { preferred: String(k) } })}
                    items={[{ id: "auto", label: "Windows default browser" }, ...browsers.map((b) => ({ id: b.name, label: b.name }))]}
                >
                    {(item) => <Select.Item id={item.id} label={item.label} />}
                </Select>
                <p className="text-sm text-tertiary">
                    Relay's browser extension, which lets it work inside your signed-in profile, comes in a later version. Until then Relay works in
                    your browser through the screen, and you can skip this step.
                </p>
            </Card>
        </div>
    );
};

const TestStep = ({ onResult }: { onResult: (ok: boolean) => void }) => {
    const [checks, setChecks] = useState<Check[] | null>(null);
    const [verify, setVerify] = useState<{ ok: boolean; text: string } | null>(null);
    const run = async () => {
        setChecks(null);
        setVerify(null);
        const r = await api.doctor();
        setChecks(r.checks);
        try {
            const text = await api.claudeVerify();
            setVerify({ ok: true, text });
        } catch (e) {
            setVerify({ ok: false, text: String(e) });
        }
    };
    useEffect(() => {
        run();
    }, []);
    useEffect(() => {
        if (verify) onResult(verify.ok);
    }, [verify, onResult]);
    const row = (name: string, ok: boolean | null, detail: string) => (
        <div key={name} className="flex items-start justify-between gap-3 border-b border-secondary py-3 last:border-b-0">
            <span className="flex items-center gap-2 text-sm font-medium text-secondary">
                {ok === null ? (
                    <span className="size-4 animate-pulse rounded-full bg-quaternary" />
                ) : ok ? (
                    <CheckCircle className="size-4 text-fg-success-primary" />
                ) : (
                    <XCircle className="size-4 text-fg-warning-primary" />
                )}
                {name}
            </span>
            <span className="text-right text-sm break-all text-tertiary">{detail}</span>
        </div>
    );
    return (
        <div className="flex flex-col gap-4">
            <Card className="py-1">
                {checks === null && row("Checking Relay", null, "")}
                {checks?.map((c) => row(c.name, c.ok, c.detail))}
                {checks && row("Claude answers a test message", verify ? verify.ok : null, verify ? (verify.ok ? `"${verify.text}"` : verify.text) : "asking Claude…")}
            </Card>
            <div>
                <Button size="sm" color="secondary" onClick={run}>
                    Run again
                </Button>
            </div>
            <p className="text-sm text-tertiary">Items in yellow can be fixed later from Settings. Local models are optional: without them Claude reads the screen itself.</p>
        </div>
    );
};

export const Wizard = ({ settings, save, onFinish }: { settings: Settings; save: (s: Settings) => Promise<void>; onFinish: () => void }) => {
    const [step, setStep] = useState(0);
    const [claudeReady, setClaudeReady] = useState(false);
    const [, setTestOk] = useState(false);
    const current = STEPS[step];
    const last = step === STEPS.length - 1;

    const next = async () => {
        if (last) {
            await api.setupComplete();
            onFinish();
        } else setStep(step + 1);
    };

    return (
        <div className="flex h-full min-h-0">
            <aside className="w-64 shrink-0 border-r border-secondary bg-secondary_alt px-6 py-8">
                <div className="mb-8 text-lg font-semibold text-primary">Set up Relay</div>
                <ol className="flex flex-col gap-1">
                    {STEPS.map((s, i) => (
                        <li
                            key={s.id}
                            className={cx(
                                "flex items-center gap-3 rounded-md px-3 py-2 text-sm font-semibold",
                                i === step ? "bg-primary text-primary shadow-xs ring-1 ring-secondary" : i < step ? "text-tertiary" : "text-quaternary",
                            )}
                        >
                            {i < step ? <CheckCircle className="size-5 text-fg-success-primary" /> : <s.icon className="size-5" />}
                            {s.label}
                        </li>
                    ))}
                </ol>
            </aside>
            <main className="min-w-0 flex-1 overflow-y-auto px-10 py-10">
                <div className="mx-auto flex max-w-2xl flex-col gap-6">
                    <div className="flex items-center gap-4">
                        <FeaturedIcon icon={current.icon} color="brand" theme="light" size="lg" />
                        <div>
                            <h1 className="text-display-xs font-semibold text-primary">
                                {current.id === "welcome" && "Welcome to Relay"}
                                {current.id === "claude" && "Connect Claude"}
                                {current.id === "model" && "Local AI"}
                                {current.id === "browser" && "Your browser"}
                                {current.id === "test" && "Self-test"}
                            </h1>
                            <p className="text-md text-tertiary">
                                {current.id === "welcome" && "Local hands, cloud brain: Claude thinks, Relay acts on your PC."}
                                {current.id === "claude" && "Relay uses Claude Code with your own Claude subscription."}
                                {current.id === "model" && "A small vision model on your GPU helps Relay find and check things on screen quickly."}
                                {current.id === "browser" && "Pick the browser Relay should use."}
                                {current.id === "test" && "A quick check that everything is in place."}
                            </p>
                        </div>
                    </div>

                    {current.id === "welcome" && (
                        <Card className="flex flex-col gap-3 text-md text-secondary">
                            <p>This takes a few minutes. Relay will:</p>
                            <ul className="ml-5 list-disc text-tertiary">
                                <li>install Claude Code if you don't have it, and let you sign in;</li>
                                <li>check your GPU and suggest a local model that fits;</li>
                                <li>run a short self-test.</li>
                            </ul>
                            <p className="text-sm text-tertiary">Relay asks before it clicks or types, and moving the mouse to the top-left corner stops it at once.</p>
                        </Card>
                    )}
                    {current.id === "claude" && <ClaudeSetup onReady={setClaudeReady} />}
                    {current.id === "model" && <ModelStep />}
                    {current.id === "browser" && <BrowserStep settings={settings} save={save} />}
                    {current.id === "test" && <TestStep onResult={setTestOk} />}

                    <div className="flex items-center justify-between border-t border-secondary pt-6">
                        <Button color="link-gray" onClick={() => setStep(Math.max(0, step - 1))} isDisabled={step === 0}>
                            Back
                        </Button>
                        <div className="flex items-center gap-3">
                            {current.id === "claude" && !claudeReady && (
                                <Button color="link-gray" onClick={next}>
                                    Skip for now
                                </Button>
                            )}
                            <Button iconTrailing={ArrowRight} onClick={next} isDisabled={current.id === "claude" && !claudeReady}>
                                {last ? "Finish" : "Continue"}
                            </Button>
                        </div>
                    </div>
                </div>
            </main>
        </div>
    );
};
