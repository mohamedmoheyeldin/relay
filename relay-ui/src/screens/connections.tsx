// Settings > Connections: one MCP hub shared with Claude Code (D-047).
import { useCallback, useEffect, useState } from "react";
import { LinkExternal01, PauseCircle, PlayCircle, Plus, RefreshCw01, Trash01 } from "@untitledui/icons";
import { Badge } from "@/components/base/badges/badges";
import { Button } from "@/components/base/buttons/button";
import { Input } from "@/components/base/input/input";
import { Select } from "@/components/base/select/select";
import { Card, Notice, Section } from "@/components/relay/ui";
import { type CatalogServer, type McpEntry, type McpStatus, type ServerSpec, api, errorText } from "@/lib/api";

const statusBadge = (s: McpStatus, text: string) => {
    const color = s === "connected" ? "success" : s === "needs_auth" || s === "paused" ? "warning" : s === "failed" ? "error" : "gray";
    const label =
        s === "connected" ? "Connected" : s === "needs_auth" ? "Needs sign-in" : s === "paused" ? "Paused" : s === "failed" ? "Failed" : s === "disabled" ? "Disabled" : s === "not_configured" ? "Not set up" : text;
    return (
        <Badge size="sm" color={color}>
            {label}
        </Badge>
    );
};

const Row = ({ e, onChange, setError }: { e: McpEntry; onChange: () => void; setError: (m: string | null) => void }) => {
    const [busy, setBusy] = useState(false);
    const run = async (f: () => Promise<unknown>) => {
        setBusy(true);
        setError(null);
        try {
            await f();
        } catch (err) {
            setError(errorText(err));
        }
        setBusy(false);
        onChange();
    };
    const canPause = e.kind === "builtin" || e.kind === "local";
    return (
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-secondary py-3 last:border-b-0">
            <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                    <span className="text-sm font-semibold text-primary">{e.name.replace(/^claude\.ai /, "")}</span>
                    {e.kind === "builtin" && (
                        <Badge size="sm" color="brand">
                            Built in
                        </Badge>
                    )}
                    {statusBadge(e.status, e.status_text)}
                    <span className="text-xs text-quaternary uppercase">{e.transport}</span>
                </div>
                <div className="mt-0.5 truncate text-xs text-tertiary" title={e.target}>
                    {e.target}
                </div>
                {e.status === "failed" && <div className="mt-0.5 text-xs text-error-primary">{e.status_text}</div>}
            </div>
            <div className="flex gap-2">
                {e.kind === "claude_ai" && (
                    <Button size="sm" color="secondary" iconTrailing={LinkExternal01} onClick={() => api.openPath("https://claude.ai/settings/connectors")}>
                        Manage
                    </Button>
                )}
                {canPause && e.status !== "paused" && (
                    <Button size="sm" color="secondary" iconLeading={PauseCircle} isLoading={busy} onClick={() => run(() => api.mcpPause(e.name))}>
                        Pause
                    </Button>
                )}
                {e.status === "paused" && (
                    <Button size="sm" color="secondary" iconLeading={PlayCircle} isLoading={busy} onClick={() => run(() => api.mcpResume(e.name))}>
                        Resume
                    </Button>
                )}
                {e.kind === "local" && (
                    <Button size="sm" color="tertiary-destructive" iconLeading={Trash01} isDisabled={busy} onClick={() => run(() => api.mcpRemove(e.name))}>
                        Remove
                    </Button>
                )}
            </div>
        </div>
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
        { title: "Relay", description: "Relay's own tools. They can be paused but not removed.", kinds: ["builtin"] },
        { title: "Your servers", description: "Local and web MCP servers. Whatever is here, Claude can use too.", kinds: ["local"] },
        { title: "Claude connectors", description: "Connectors from your Claude account, like Gmail and Calendar. Manage them on claude.ai.", kinds: ["claude_ai"] },
        { title: "Plugin servers", description: "Servers that come with Claude Code plugins.", kinds: ["plugin"] },
    ];

    return (
        <>
            <div className="flex items-center justify-between pt-4">
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
            {entries === null && !error && <p className="py-6 text-sm text-tertiary">Asking Claude Code for its servers…</p>}
            {entries &&
                groups.map((g) => {
                    const rows = entries.filter((e) => g.kinds.includes(e.kind));
                    if (rows.length === 0 && g.kinds[0] !== "local" && g.kinds[0] !== "builtin") return null;
                    return (
                        <Section key={g.title} title={g.title} description={g.description}>
                            <Card className="py-1">
                                {rows.length === 0 ? (
                                    <p className="py-3 text-sm text-tertiary">
                                        {g.kinds[0] === "builtin" ? "relay-screen is not registered yet. It is added when Claude Code is installed." : "None yet."}
                                    </p>
                                ) : (
                                    rows.map((e) => <Row key={e.name} e={e} onChange={refresh} setError={setError} />)
                                )}
                            </Card>
                        </Section>
                    );
                })}
            <Section title="Add a server" description="Pick a well-known server or enter one by hand. It is added for Claude Code at user scope.">
                <AddServer catalog={catalog} onAdded={refresh} />
            </Section>
        </>
    );
};
