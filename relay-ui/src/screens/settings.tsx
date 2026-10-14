// Settings (D-048): every option Relay has, no config files or terminal needed.
import { type FC, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
    Activity,
    BookOpen01,
    Copy01,
    CpuChip01,
    Download01,
    Folder,
    Globe01,
    Link01,
    Lock01,
    RefreshCw01,
    Settings01,
    Shield01,
    Stars01,
    Trash01,
} from "@untitledui/icons";
import { Badge } from "@/components/base/badges/badges";
import { Button } from "@/components/base/buttons/button";
import { Input } from "@/components/base/input/input";
import { Select } from "@/components/base/select/select";
import { Toggle } from "@/components/base/toggle/toggle";
import { Card, JobProgress, KeyValue, Notice, PageHeader, Section } from "@/components/relay/ui";
import { type AppInfo, type Check, type Playbook, type Settings, type UpdateInfo, api, errorText } from "@/lib/api";
import { bytes } from "@/lib/format";
import { clearJob, useJobs } from "@/lib/jobs";
import { cx } from "@/utils/cx";
import { ClaudeSetup } from "./claude-setup";
import { ConnectionsPage } from "./connections";
import { LocalAiPage } from "./local-ai";

type Save = (s: Settings) => Promise<void>;
interface PageProps {
    settings: Settings;
    save: Save;
    info: AppInfo;
    rerunSetup: () => void;
}

const GeneralPage = ({ settings, save, rerunSetup }: PageProps) => (
    <>
        <Section title="Startup" description="Relay lives in the tray so it is ready when you need it.">
            <Toggle
                label="Start Relay when I sign in to Windows"
                hint="Relay starts quietly in the tray."
                isSelected={settings.general.start_at_login}
                onChange={(v) => save({ ...settings, general: { ...settings.general, start_at_login: v } })}
            />
        </Section>
        <Section title="Closing the window" description="What the close button does.">
            <Select
                size="sm"
                aria-label="When the window closes"
                selectedKey={settings.general.close_action}
                onSelectionChange={(k) => save({ ...settings, general: { ...settings.general, close_action: k as Settings["general"]["close_action"] } })}
                items={[
                    { id: "tray", label: "Keep running in the tray (recommended)" },
                    { id: "quit", label: "Quit Relay" },
                    { id: "ask", label: "Ask me" },
                ]}
            >
                {(item) => <Select.Item id={item.id} label={item.label} />}
            </Select>
        </Section>
        <Section title="Language" description="More languages will follow.">
            <Select
                size="sm"
                aria-label="Language"
                selectedKey={settings.general.language}
                onSelectionChange={(k) => save({ ...settings, general: { ...settings.general, language: String(k) } })}
                items={[{ id: "en", label: "English" }]}
            >
                {(item) => <Select.Item id={item.id} label={item.label} />}
            </Select>
        </Section>
        <Section title="Setup" description="Go through the first-run setup again: Claude Code, the local model and the self-test.">
            <div>
                <Button size="sm" color="secondary" onClick={rerunSetup}>
                    Run setup again
                </Button>
            </div>
        </Section>
    </>
);

const ClaudePage = ({ settings, save }: PageProps) => {
    const [model, setModel] = useState(settings.claude.model);
    return (
        <>
            <Section title="Claude Code" description="Relay uses your own Claude Code and Claude subscription. It never sees your password or token.">
                <ClaudeSetup />
            </Section>
            <Section title="Model and effort" description="Passed to Claude Code for each task. Leave empty to use Claude Code's own default.">
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 sm:items-end">
                    <Input
                        label="Model"
                        placeholder="Claude Code default (for example opus or sonnet)"
                        size="sm"
                        value={model}
                        onChange={setModel}
                        onBlur={() => save({ ...settings, claude: { ...settings.claude, model: model.trim() } })}
                    />
                    <Select
                        label="Effort"
                        size="sm"
                        selectedKey={settings.claude.effort || "default"}
                        onSelectionChange={(k) => save({ ...settings, claude: { ...settings.claude, effort: k === "default" ? "" : String(k) } })}
                        items={[
                            { id: "default", label: "Claude Code default" },
                            { id: "low", label: "Low" },
                            { id: "medium", label: "Medium" },
                            { id: "high", label: "High" },
                            { id: "xhigh", label: "Extra high" },
                            { id: "max", label: "Max" },
                        ]}
                    >
                        {(item) => <Select.Item id={item.id} label={item.label} />}
                    </Select>
                </div>
            </Section>
        </>
    );
};

const BrowserPage = ({ settings, save }: PageProps) => {
    const [browsers, setBrowsers] = useState<{ name: string; path: string }[]>([]);
    useEffect(() => {
        api.browsers().then(setBrowsers);
    }, []);
    return (
        <>
            <Section title="Browser" description="The browser Relay uses for web tasks. Relay finds the browsers installed on this PC.">
                <Select
                    size="sm"
                    aria-label="Browser"
                    selectedKey={settings.browser.preferred}
                    onSelectionChange={(k) => save({ ...settings, browser: { preferred: String(k) } })}
                    items={[{ id: "auto", label: "Windows default browser" }, ...browsers.map((b) => ({ id: b.name, label: b.name, supportingText: b.path }))]}
                >
                    {(item) => <Select.Item id={item.id} label={item.label} supportingText={item.supportingText} />}
                </Select>
            </Section>
            <Section title="Relay extension" description="Lets Relay work inside your own signed-in browser profile.">
                <Card className="flex items-center justify-between gap-3">
                    <span className="text-sm text-secondary">The Brave extension is not available yet. Until then Relay works in the browser through the screen.</span>
                    <Badge color="gray" size="sm">
                        Coming soon
                    </Badge>
                </Card>
            </Section>
        </>
    );
};

const SafetyPage = ({ settings, save }: PageProps) => {
    const [topic, setTopic] = useState(settings.safety.ntfy_topic);
    const [alwaysAsk, setAlwaysAsk] = useState(settings.safety.always_ask.join(", "));
    const safety = settings.safety;
    return (
        <>
            <Section title="Approvals" description="Relay can ask before it clicks or types for Claude.">
                <Select
                    size="sm"
                    aria-label="Approval mode"
                    selectedKey={safety.approval_mode}
                    onSelectionChange={(k) => save({ ...settings, safety: { ...safety, approval_mode: String(k) } })}
                    items={[
                        { id: "ask", label: "Ask before acting (recommended)" },
                        { id: "off", label: "Don't ask" },
                    ]}
                >
                    {(item) => <Select.Item id={item.id} label={item.label} />}
                </Select>
                <Input
                    label="Always ask before these tools"
                    hint="Tool names separated by commas, for example: type, key. Used even when approvals are off."
                    size="sm"
                    value={alwaysAsk}
                    onChange={setAlwaysAsk}
                    onBlur={() =>
                        save({
                            ...settings,
                            safety: {
                                ...safety,
                                always_ask: alwaysAsk
                                    .split(",")
                                    .map((s) => s.trim())
                                    .filter(Boolean),
                            },
                        })
                    }
                />
            </Section>
            <Section title="Kill switch" description="Stops Relay at once, whatever it is doing.">
                <Card className="flex flex-col gap-2">
                    <KeyValue label="Mouse" value="Move the mouse into the top-left corner of the main screen" />
                    <KeyValue label="Keyboard" value="Press Ctrl + Alt + Pause" />
                    <p className="text-xs text-tertiary">After a stop, Claude asks you before Relay acts again.</p>
                </Card>
            </Section>
            <Section title="Phone approvals" description="Answer approval requests from your phone with the ntfy app.">
                <Toggle
                    label="Send approval requests to my phone"
                    isSelected={safety.phone_approvals}
                    onChange={(v) => save({ ...settings, safety: { ...safety, phone_approvals: v } })}
                />
                {safety.phone_approvals && (
                    <Input
                        label="ntfy topic"
                        hint="Pick a long, hard-to-guess name and subscribe to it in the ntfy app."
                        size="sm"
                        value={topic}
                        onChange={setTopic}
                        onBlur={() => save({ ...settings, safety: { ...safety, ntfy_topic: topic.trim() } })}
                    />
                )}
            </Section>
        </>
    );
};

const PlaybooksPage = ({ info }: PageProps) => {
    const [list, setList] = useState<Playbook[]>([]);
    const refresh = () => api.playbooks().then(setList);
    useEffect(() => {
        refresh();
    }, []);
    return (
        <Section title="Playbooks" description="Steps Relay learned for tasks it has done, so it can repeat them quickly.">
            <Card className="py-1">
                {list.length === 0 && <p className="py-3 text-sm text-tertiary">No playbooks yet. Relay saves one after a task goes well.</p>}
                {list.map((p) => (
                    <div key={p.name} className="flex items-center justify-between gap-3 border-b border-secondary py-3 last:border-b-0">
                        <div>
                            <div className="text-sm font-semibold text-primary">{p.name}</div>
                            <div className="text-xs text-tertiary">
                                {bytes(p.bytes)} · changed {new Date(p.modified * 1000).toLocaleString()}
                            </div>
                        </div>
                        <div className="flex gap-2">
                            <Button size="sm" color="secondary" onClick={() => api.openPath(p.path)}>
                                Edit
                            </Button>
                            <Button size="sm" color="tertiary-destructive" iconLeading={Trash01} onClick={() => api.playbookDelete(p.name).then(refresh)}>
                                Delete
                            </Button>
                        </div>
                    </div>
                ))}
            </Card>
            <div>
                <Button size="sm" color="secondary" iconLeading={Folder} onClick={() => api.openPath(info.playbooks_dir)}>
                    Open playbooks folder
                </Button>
            </div>
        </Section>
    );
};

const PrivacyPage = ({ settings, save, info }: PageProps) => {
    const runsDir = settings.privacy.runs_dir ?? info.default_runs_dir;
    const choose = async () => {
        const dir = await openDialog({ directory: true, title: "Folder for activity logs and screenshots" });
        if (typeof dir === "string") await save({ ...settings, privacy: { ...settings.privacy, runs_dir: dir } });
    };
    return (
        <>
            <Section title="Activity logs and screenshots" description="Relay keeps a log of every action, with screenshots, on this PC only.">
                <Card className="flex flex-col gap-3">
                    <KeyValue label="Folder" value={runsDir} />
                    <div className="flex flex-wrap gap-2">
                        <Button size="sm" color="secondary" iconLeading={Folder} onClick={() => api.openPath(runsDir)}>
                            Open folder
                        </Button>
                        <Button size="sm" color="secondary" onClick={choose}>
                            Change folder
                        </Button>
                    </div>
                </Card>
            </Section>
            <Section title="Keep for" description="Older logs and screenshots are deleted when Relay starts.">
                <Select
                    size="sm"
                    aria-label="Keep logs for"
                    selectedKey={String(settings.privacy.retention_days)}
                    onSelectionChange={(k) => save({ ...settings, privacy: { ...settings.privacy, retention_days: Number(k) } })}
                    items={[
                        { id: "7", label: "7 days" },
                        { id: "30", label: "30 days" },
                        { id: "90", label: "90 days" },
                        { id: "0", label: "Forever" },
                    ]}
                >
                    {(item) => <Select.Item id={item.id} label={item.label} />}
                </Select>
            </Section>
        </>
    );
};

const UpdatesPage = ({ settings, save, info }: PageProps) => {
    const jobs = useJobs();
    const [update, setUpdate] = useState<UpdateInfo | null>(null);
    const [checking, setChecking] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const check = async () => {
        setChecking(true);
        setError(null);
        try {
            setUpdate(await api.updateCheck());
        } catch (e) {
            setError(errorText(e));
        }
        setChecking(false);
    };
    return (
        <>
            <Section title="Version" description="Relay updates come from its GitHub releases.">
                <Card className="flex flex-col gap-3">
                    <KeyValue label="Installed" value={info.version} />
                    {update && <KeyValue label="Latest" value={update.latest} />}
                    {update && !update.available && <Notice tone="success">Relay is up to date.</Notice>}
                    {update?.available && (
                        <Notice>
                            Relay {update.latest} is available. The installer is saved to your Downloads folder and Relay restarts into it.
                        </Notice>
                    )}
                    <JobProgress job={jobs.update} onCancel={() => api.cancelJob("update")} />
                    {error && <Notice tone="error">{error}</Notice>}
                    <div className="flex flex-wrap gap-2">
                        <Button size="sm" color="secondary" iconLeading={RefreshCw01} onClick={check} isLoading={checking}>
                            Check for updates
                        </Button>
                        {update?.available && (
                            <Button
                                size="sm"
                                iconLeading={Download01}
                                onClick={() => {
                                    clearJob("update");
                                    api.updateInstall(update).catch((e) => setError(errorText(e)));
                                }}
                            >
                                Download and install
                            </Button>
                        )}
                    </div>
                </Card>
            </Section>
            <Section title="Automatic check" description="Look for a new version each time Relay starts.">
                <Toggle
                    label="Check for updates when Relay starts"
                    isSelected={settings.updates.check_on_start}
                    onChange={(v) => save({ ...settings, updates: { check_on_start: v } })}
                />
            </Section>
        </>
    );
};

const DiagnosticsPage = ({ info }: PageProps) => {
    const [checks, setChecks] = useState<Check[] | null>(null);
    const [report, setReport] = useState("");
    const [copied, setCopied] = useState(false);
    const run = async () => {
        setChecks(null);
        const r = await api.doctor();
        setChecks(r.checks);
        setReport(r.report);
    };
    useEffect(() => {
        run();
    }, []);
    return (
        <>
            <Section title="Checks" description="Everything Relay needs, checked now.">
                <Card className="py-1">
                    {checks === null && <p className="py-3 text-sm text-tertiary">Running checks…</p>}
                    {checks?.map((c) => (
                        <div key={c.name} className="flex items-start justify-between gap-3 border-b border-secondary py-3 last:border-b-0">
                            <span className="text-sm font-medium text-secondary">{c.name}</span>
                            <span className={cx("text-right text-sm break-all", c.ok ? "text-tertiary" : "text-error-primary")}>{c.detail}</span>
                        </div>
                    ))}
                </Card>
                <div className="flex flex-wrap gap-2">
                    <Button size="sm" color="secondary" iconLeading={RefreshCw01} onClick={run}>
                        Run again
                    </Button>
                    <Button
                        size="sm"
                        color="secondary"
                        iconLeading={Copy01}
                        isDisabled={!report}
                        onClick={async () => {
                            await navigator.clipboard.writeText(report);
                            setCopied(true);
                            setTimeout(() => setCopied(false), 2000);
                        }}
                    >
                        {copied ? "Copied" : "Copy report"}
                    </Button>
                </div>
            </Section>
            <Section title="Folders" description="Where Relay keeps its files.">
                <Card className="flex flex-col gap-2">
                    <KeyValue label="Settings" value={info.settings_file} />
                    <KeyValue label="Data" value={info.data_dir} />
                    <KeyValue label="Logs" value={info.logs_dir} />
                </Card>
                <div className="flex flex-wrap gap-2">
                    <Button size="sm" color="secondary" iconLeading={Folder} onClick={() => api.openPath(info.data_dir)}>
                        Open data folder
                    </Button>
                    <Button size="sm" color="secondary" iconLeading={Folder} onClick={() => api.openPath(info.logs_dir)}>
                        Open log folder
                    </Button>
                </div>
            </Section>
        </>
    );
};

const PAGES: { id: string; label: string; icon: FC<{ className?: string }>; description: string }[] = [
    { id: "general", label: "General", icon: Settings01, description: "Startup, the tray and language." },
    { id: "claude", label: "Claude", icon: Stars01, description: "Claude Code, your sign-in, and the model Relay asks for." },
    { id: "local-ai", label: "Local AI", icon: CpuChip01, description: "The models that run on your GPU to find and check things on screen." },
    { id: "connections", label: "Connections", icon: Link01, description: "MCP servers and connectors, shared with Claude Code." },
    { id: "browser", label: "Browser", icon: Globe01, description: "Which browser Relay uses for web tasks." },
    { id: "safety", label: "Safety", icon: Shield01, description: "Approvals, the kill switch and phone approvals." },
    { id: "playbooks", label: "Playbooks", icon: BookOpen01, description: "Tasks Relay has learned to repeat." },
    { id: "privacy", label: "Privacy", icon: Lock01, description: "Activity logs and screenshots." },
    { id: "updates", label: "Updates", icon: Download01, description: "Relay's version and updates." },
    { id: "diagnostics", label: "Diagnostics", icon: Activity, description: "Health checks and a report to share." },
];

export const SettingsScreen = ({
    settings,
    save,
    info,
    rerunSetup,
    page,
    setPage,
}: PageProps & { page: string; setPage: (p: string) => void }) => {
    const current = PAGES.find((p) => p.id === page) ?? PAGES[0];
    const props = { settings, save, info, rerunSetup };
    return (
        <div className="flex h-full min-h-0">
            <nav className="w-52 shrink-0 overflow-y-auto border-r border-secondary px-3 py-6">
                <div className="px-3 pb-3 text-xs font-semibold tracking-wide text-quaternary uppercase">Settings</div>
                {PAGES.map((p) => (
                    <button
                        key={p.id}
                        onClick={() => setPage(p.id)}
                        className={cx(
                            "flex w-full cursor-pointer items-center gap-3 rounded-md px-3 py-2 text-sm font-semibold transition",
                            p.id === current.id ? "bg-active text-secondary_hover" : "text-secondary hover:bg-primary_hover",
                        )}
                    >
                        <p.icon className="size-5 text-fg-quaternary" />
                        {p.label}
                    </button>
                ))}
            </nav>
            <div className="min-w-0 flex-1 overflow-y-auto px-8 py-6">
                <div className="mx-auto max-w-4xl">
                    <PageHeader title={current.label} description={current.description} />
                    {current.id === "general" && <GeneralPage {...props} />}
                    {current.id === "claude" && <ClaudePage {...props} />}
                    {current.id === "local-ai" && <LocalAiPage settings={settings} save={save} />}
                    {current.id === "connections" && <ConnectionsPage />}
                    {current.id === "browser" && <BrowserPage {...props} />}
                    {current.id === "safety" && <SafetyPage {...props} />}
                    {current.id === "playbooks" && <PlaybooksPage {...props} />}
                    {current.id === "privacy" && <PrivacyPage {...props} />}
                    {current.id === "updates" && <UpdatesPage {...props} />}
                    {current.id === "diagnostics" && <DiagnosticsPage {...props} />}
                </div>
            </div>
        </div>
    );
};
