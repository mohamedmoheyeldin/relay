// Settings > Connections: one MCP hub shared with Claude Code (D-047).
import { useCallback, useEffect, useState } from "react";
import { LinkExternal01, LogIn01, LogOut01, PauseCircle, PlayCircle, Plus, Power01, PuzzlePiece01, RefreshCw01, Trash01 } from "@untitledui/icons";
import { Badge } from "@/components/base/badges/badges";
import { Button } from "@/components/base/buttons/button";
import { Input } from "@/components/base/input/input";
import { Select } from "@/components/base/select/select";
import { Toggle } from "@/components/base/toggle/toggle";
import { Card, Notice, Section, Skeleton, Spinner } from "@/components/relay/ui";
import { type CatalogServer, type McpEntry, type McpStatus, type Plugin, type ServerSpec, api, errorText } from "@/lib/api";
import { clearJob, useJobs, waitForJob } from "@/lib/jobs";

const statusBadge = (s: McpStatus, text: string) => {
    const color = s === "connected" ? "success" : s === "needs_auth" || s === "paused" ? "warning" : s === "failed" ? "error" : "gray";
    const label =
        s === "connected"
            ? "Connected"
            : s === "needs_auth"
              ? "Needs sign-in"
              : s === "paused"
                ? "Paused"
                : s === "failed"
                  ? "Failed"
                  : s === "disabled"
                    ? "Off"
                    : s === "not_configured"
                      ? "Not set up"
                      : text;
    return (
        <Badge size="sm" color={color}>
            {label}
        </Badge>
    );
};

const displayName = (e: McpEntry) => e.name.replace(/^claude\.ai /, "").replace(/^plugin:[^:]+:/, "");
const pluginLabel = (id: string) => id.split("@")[0];

/** What a row says under its name when its status needs explaining. */
const explain = (e: McpEntry): string | null => {
    switch (e.status) {
        case "needs_auth":
            return e.kind === "claude_ai" ? "Sign in on claude.ai to use it." : "Sign in once and Claude Code keeps you signed in.";
        case "not_configured":
            return `The ${e.plugin ? pluginLabel(e.plugin) : "plugin"} plugin needs settings for this server before it can start. If you don't use it, turn the plugin off.`;
        case "failed":
            return e.status_text;
        default:
            return null;
    }
};

const Row = ({ e, onChange, setError }: { e: McpEntry; onChange: () => void; setError: (m: string | null) => void }) => {
    const jobs = useJobs();
    const [busy, setBusy] = useState<string | null>(null);
    const loginJob = `mcp-login:${e.name}`;
    const login = jobs[loginJob];
    const signingIn = login?.state === "running";
    const run = async (what: string, f: () => Promise<unknown>) => {
        setBusy(what);
        setError(null);
        try {
            await f();
        } catch (err) {
            setError(errorText(err));
        }
        setBusy(null);
        onChange();
    };
    const signIn = () =>
        run("login", async () => {
            clearJob(loginJob);
            await api.mcpLogin(e.name);
            await waitForJob(loginJob);
        });
    const canPause = e.kind === "builtin" || e.kind === "local";
    const isWeb = e.transport === "http" || e.transport === "sse";
    const note = explain(e);
    return (
        <div className="flex flex-col gap-3 border-b border-secondary px-4 py-3.5 transition-colors last:border-b-0 hover:bg-primary_hover/40 sm:px-5">
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                        <span className="text-sm font-semibold text-primary">{displayName(e)}</span>
                        {e.kind === "builtin" && (
                            <Badge size="sm" color="brand">
                                Built in
                            </Badge>
                        )}
                        {e.kind === "plugin" && e.plugin && (
                            <Badge size="sm" color="gray">
                                {pluginLabel(e.plugin)} plugin
                            </Badge>
                        )}
                        {statusBadge(e.status, e.status_text)}
                    </div>
                    {e.target && (
                        <div className="mt-0.5 truncate text-xs text-tertiary" title={e.target}>
                            {e.target}
                        </div>
                    )}
                    {note && <div className={e.status === "failed" ? "mt-1 text-xs text-error-primary" : "mt-1 text-xs text-tertiary"}>{note}</div>}
                </div>
                <div className="flex flex-wrap gap-2">
                    {e.status === "needs_auth" && e.kind !== "claude_ai" && (
                        <Button size="sm" iconLeading={LogIn01} isLoading={signingIn} showTextWhileLoading onClick={signIn}>
                            Sign in
                        </Button>
                    )}
                    {e.kind === "claude_ai" && (
                        <Button size="sm" color="secondary" iconTrailing={LinkExternal01} onClick={() => api.openPath("https://claude.ai/settings/connectors")}>
                            Manage on claude.ai
                        </Button>
                    )}
                    {e.status === "connected" && isWeb && e.kind !== "claude_ai" && (
                        <Button size="sm" color="tertiary" iconLeading={LogOut01} isLoading={busy === "logout"} onClick={() => run("logout", () => api.mcpLogout(e.name))}>
                            Sign out
                        </Button>
                    )}
                    {e.kind === "plugin" && e.plugin && (
                        <Button
                            size="sm"
                            color="secondary"
                            iconLeading={Power01}
                            isLoading={busy === "plugin"}
                            onClick={() => run("plugin", () => api.pluginSetEnabled(e.plugin!, false))}
                        >
                            Turn off plugin
                        </Button>
                    )}
                    {canPause && e.status !== "paused" && (
                        <Button size="sm" color="secondary" iconLeading={PauseCircle} isLoading={busy === "pause"} onClick={() => run("pause", () => api.mcpPause(e.name))}>
                            Pause
                        </Button>
                    )}
                    {e.status === "paused" && (
                        <Button size="sm" color="secondary" iconLeading={PlayCircle} isLoading={busy === "resume"} onClick={() => run("resume", () => api.mcpResume(e.name))}>
                            Resume
                        </Button>
                    )}
                    {e.kind === "local" && (
                        <Button size="sm" color="tertiary-destructive" iconLeading={Trash01} isDisabled={!!busy} onClick={() => run("remove", () => api.mcpRemove(e.name))}>
                            Remove
                        </Button>
                    )}
                </div>
            </div>
            {signingIn && (
                <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg bg-secondary px-3 py-2 text-sm text-secondary animate-in fade-in duration-200">
                    <span className="flex items-center gap-2">
                        <Spinner />
                        Finish signing in in your browser. Relay notices when you're done.
                    </span>
                    <Button size="sm" color="link-gray" onClick={() => api.cancelJob(loginJob)}>
                        Cancel
                    </Button>
                </div>
            )}
            {login?.state === "error" && <Notice tone="error">{login.error}</Notice>}
        </div>
    );
};

const Plugins = ({ onChange }: { onChange: () => void }) => {
    const [list, setList] = useState<Plugin[] | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [busy, setBusy] = useState<string | null>(null);
    const refresh = useCallback(() => {
        api.plugins()
            .then(setList)
            .catch((e) => setError(errorText(e)));
    }, []);
    useEffect(refresh, [refresh]);
    const toggle = async (p: Plugin, enabled: boolean) => {
        setBusy(p.id);
        setError(null);
        try {
            await api.pluginSetEnabled(p.id, enabled);
        } catch (e) {
            setError(errorText(e));
        }
        setBusy(null);
        refresh();
        onChange();
    };
    return (
        <Section title="Claude Code plugins" description="A plugin can bring servers, skills and commands. Turning one off turns off everything it brings, for Claude Code too.">
            {error && <Notice tone="error">{error}</Notice>}
            <Card className="overflow-hidden p-0">
                {list === null ? (
                    <div className="flex flex-col gap-3 p-5">
                        <Skeleton className="h-4 w-40" />
                        <Skeleton className="h-4 w-56" />
                    </div>
                ) : list.length === 0 ? (
                    <p className="px-5 py-4 text-sm text-tertiary">No plugins installed.</p>
                ) : (
                    list.map((p) => (
                        <div key={p.id} className="flex items-center justify-between gap-3 border-b border-secondary px-4 py-3 last:border-b-0 sm:px-5">
                            <div className="flex min-w-0 items-center gap-3">
                                <PuzzlePiece01 className="size-5 shrink-0 text-fg-quaternary" />
                                <div className="min-w-0">
                                    <div className="truncate text-sm font-semibold text-primary">{pluginLabel(p.id)}</div>
                                    <div className="truncate text-xs text-tertiary">
                                        {p.id.split("@")[1]}
                                        {p.version && ` · ${p.version}`}
                                    </div>
                                </div>
                            </div>
                            <Toggle aria-label={`Turn ${pluginLabel(p.id)} on or off`} size="sm" isSelected={p.enabled} isDisabled={busy === p.id} onChange={(v) => toggle(p, v)} />
                        </div>
                    ))
                )}
            </Card>
        </Section>
    );
};

const AddServer = ({ catalog, onAdded }: { catalog: CatalogServer[]; onAdded: () => void }) => {
    const [preset, setPreset] = useState("custom");
    const [name, setName] = useState("");
    const [transport, setTransport] = useState<"stdio" | "http" | "sse">("stdio");
    const [command, setCommand] = useState("");
    const [args, setArgs] = useState("");
    const [url, setUrl] = useState("");
    const [secrets, setSecrets] = useState<{ key: string; value: string }[]>([]);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const choose = (id: string) => {
        setPreset(id);
        const c = catalog.find((x) => x.id === id);
        if (!c) return;
        setName(c.id);
        setTransport(c.transport as "stdio" | "http");
        setCommand(c.command);
        setArgs(c.args.join(" "));
        setUrl(c.url);
        setSecrets(c.secret_env.map((key) => ({ key, value: "" })));
    };

    const add = async () => {
        setBusy(true);
        setError(null);
        const spec: ServerSpec =
            transport === "stdio"
                ? { transport, name, command, args: args.split(/\s+/).filter(Boolean), env: {} }
                : { transport, name, url, headers: {} };
        try {
            await api.mcpAdd(spec, secrets);
            setName("");
            setCommand("");
            setArgs("");
            setUrl("");
            setSecrets([]);
            setPreset("custom");
            onAdded();
        } catch (e) {
            setError(errorText(e));
        }
        setBusy(false);
    };

    return (
        <Card className="flex flex-col gap-3">
            <Select
                label="Start from"
                size="sm"
                selectedKey={preset}
                onSelectionChange={(k) => choose(String(k))}
                items={[{ id: "custom", label: "Add by hand" }, ...catalog.map((c) => ({ id: c.id, label: c.name, supportingText: c.description }))]}
            >
                {(item) => <Select.Item id={item.id} label={item.label} supportingText={item.supportingText} />}
            </Select>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <Input label="Name" placeholder="my-server" value={name} onChange={setName} size="sm" />
                <Select
                    label="Type"
                    size="sm"
                    selectedKey={transport}
                    onSelectionChange={(k) => setTransport(k as "stdio" | "http" | "sse")}
                    items={[
                        { id: "stdio", label: "Local program (stdio)" },
                        { id: "http", label: "Web address (HTTP)" },
                        { id: "sse", label: "Web address (SSE)" },
                    ]}
                >
                    {(item) => <Select.Item id={item.id} label={item.label} />}
                </Select>
            </div>
            {transport === "stdio" ? (
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-[200px_1fr]">
                    <Input label="Command" placeholder="npx" value={command} onChange={setCommand} size="sm" />
                    <Input label="Arguments" placeholder="-y some-mcp-server" value={args} onChange={setArgs} size="sm" />
                </div>
            ) : (
                <Input label="URL" placeholder="https://example.com/mcp" value={url} onChange={setUrl} size="sm" />
            )}
            {transport === "stdio" && (
                <div className="flex flex-col gap-2">
                    <span className="text-sm font-medium text-secondary">Secrets</span>
                    <p className="text-xs text-tertiary">Kept in Windows Credential Manager and handed to the server when it starts. They are never written to a file.</p>
                    {secrets.map((s, i) => (
                        <div key={i} className="grid grid-cols-[1fr_1fr_auto] items-end gap-2">
                            <Input
                                aria-label="Secret name"
                                placeholder="API_KEY"
                                size="sm"
                                value={s.key}
                                onChange={(v) => setSecrets(secrets.map((x, j) => (j === i ? { ...x, key: v } : x)))}
                            />
                            <Input
                                aria-label="Secret value"
                                placeholder="value"
                                size="sm"
                                type="password"
                                value={s.value}
                                onChange={(v) => setSecrets(secrets.map((x, j) => (j === i ? { ...x, value: v } : x)))}
                            />
                            <Button size="sm" color="tertiary" iconLeading={Trash01} aria-label="Remove secret" onClick={() => setSecrets(secrets.filter((_, j) => j !== i))} />
                        </div>
                    ))}
                    <div>
                        <Button size="sm" color="link-color" iconLeading={Plus} onClick={() => setSecrets([...secrets, { key: "", value: "" }])}>
                            Add a secret
                        </Button>
                    </div>
                </div>
            )}
            {transport !== "stdio" && <p className="text-xs text-tertiary">If the server needs a sign-in, Claude Code asks for it the first time it connects.</p>}
            {error && <Notice tone="error">{error}</Notice>}
            <div>
                <Button size="sm" iconLeading={Plus} onClick={add} isLoading={busy} isDisabled={!name || (transport === "stdio" ? !command : !url)}>
                    Add to Claude and Relay
                </Button>
            </div>
        </Card>
    );
};

export const ConnectionsPage = () => {
    const [entries, setEntries] = useState<McpEntry[] | null>(null);
    const [catalog, setCatalog] = useState<CatalogServer[]>([]);
    const [error, setError] = useState<string | null>(null);
    const [loading, setLoading] = useState(false);

    const refresh = useCallback(async () => {
        setLoading(true);
        try {
            setEntries(await api.mcpList());
        } catch (e) {
            setError(errorText(e));
        }
        setLoading(false);
    }, []);
    useEffect(() => {
        refresh();
        api.mcpCatalog().then(setCatalog);
    }, [refresh]);

    const groups: { title: string; description: string; kinds: McpEntry["kind"][] }[] = [
        { title: "Relay", description: "Relay's own tools: the screen and the browser. They can be paused but not removed.", kinds: ["builtin"] },
        { title: "Your servers", description: "Local and web MCP servers. Whatever is here, Claude can use too.", kinds: ["local"] },
        { title: "Claude connectors", description: "Connectors from your Claude account, like Gmail and Calendar. Manage them on claude.ai.", kinds: ["claude_ai"] },
        { title: "Plugin servers", description: "Servers that come with Claude Code plugins. Sign in to the ones you use; turn off plugins you don't need.", kinds: ["plugin"] },
    ];

    return (
        <>
            <div className="flex flex-wrap items-center justify-between gap-3 pt-4">
                <p className="text-sm text-tertiary">This list is Claude Code's own MCP list, so a change here is a change for Claude too.</p>
                <Button size="sm" color="secondary" iconLeading={RefreshCw01} onClick={refresh} isLoading={loading}>
                    Refresh
                </Button>
            </div>
            {error && (
                <div className="pt-4">
                    <Notice tone="error">{error}</Notice>
                </div>
            )}
            {entries === null && !error && (
                <Section title="Servers" description="Asking Claude Code for its servers. It checks each one, which takes a few seconds.">
                    <Card className="flex flex-col gap-4 p-5">
                        {[48, 64, 40].map((w) => (
                            <div key={w} className="flex items-center gap-3">
                                <Skeleton className="size-5 rounded-full" />
                                <Skeleton className="h-4" style={{ width: `${w}%` }} />
                            </div>
                        ))}
                    </Card>
                </Section>
            )}
            {entries &&
                groups.map((g) => {
                    const rows = entries.filter((e) => g.kinds.includes(e.kind));
                    if (rows.length === 0 && g.kinds[0] !== "local" && g.kinds[0] !== "builtin") return null;
                    return (
                        <Section key={g.title} title={g.title} description={g.description}>
                            <Card className="overflow-hidden p-0">
                                {rows.length === 0 ? (
                                    <p className="px-5 py-4 text-sm text-tertiary">
                                        {g.kinds[0] === "builtin" ? "relay-screen is not registered yet. It is added when Claude Code is installed." : "None yet."}
                                    </p>
                                ) : (
                                    rows.map((e) => <Row key={e.name} e={e} onChange={refresh} setError={setError} />)
                                )}
                            </Card>
                        </Section>
                    );
                })}
            <Plugins onChange={refresh} />
            <Section title="Add a server" description="Pick a well-known server or enter one by hand. It is added for Claude Code at user scope.">
                <AddServer catalog={catalog} onAdded={refresh} />
            </Section>
        </>
    );
};
