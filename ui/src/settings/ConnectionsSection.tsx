import { useEffect, useState } from "react";
import { Braces, FileCog, Pencil, Plug, Plus, Trash } from "lucide-react";
import { useApp } from "../store/app";
import { api } from "../lib/backend";
import type { McpConfig } from "../lib/types";
import { parseServerJson } from "../lib/mcp";
import { Button, cn } from "../components/ui";
import { DialogShell, fieldClass } from "../components/Dialogs";
import { PageTitle } from "./parts";

const TEMPLATE = `{
  "command": "npx",
  "args": ["-y", "@modelcontextprotocol/server-filesystem", "C:\\\\Users\\\\you\\\\Documents"]
}`;

function describe(v: unknown): string {
  if (!v || typeof v !== "object") return "";
  const o = v as Record<string, unknown>;
  if (typeof o.url === "string") return o.url;
  const args = Array.isArray(o.args) ? o.args.map(String).join(" ") : "";
  return [typeof o.command === "string" ? o.command : "", args].filter(Boolean).join(" ");
}

type Editor = { mode: "add" } | { mode: "edit"; name: string } | { mode: "all" } | null;

export function ConnectionsSection() {
  const [cfg, setCfg] = useState<McpConfig | null>(null);
  const [loadErr, setLoadErr] = useState<string | null>(null);
  const [editor, setEditor] = useState<Editor>(null);
  const { toast, setConfirmDialog } = useApp.getState();

  const load = () =>
    api.mcpGet().then(
      (c) => {
        setCfg(c);
        setLoadErr(null);
      },
      (e) => setLoadErr(e instanceof Error ? e.message : String(e)),
    );
  useEffect(() => {
    void load();
  }, []);

  const save = async (servers: Record<string, unknown>) => {
    await api.mcpSet(servers);
    await load();
  };

  const remove = (name: string) =>
    setConfirmDialog({
      title: `Remove ${name}?`,
      body: "Relay will stop offering this connection's tools. You can add it again later.",
      action: "Remove",
      destructive: true,
      onConfirm: async () => {
        if (!cfg) return;
        const next = { ...cfg.servers };
        delete next[name];
        try {
          await save(next);
          toast(`${name} removed`);
        } catch (e) {
          toast(`Couldn't save: ${e instanceof Error ? e.message : e}`, "error");
        }
      },
    });

  const names = cfg ? Object.keys(cfg.servers).sort() : [];

  return (
    <>
      <PageTitle>Connections</PageTitle>
      <p className="-mt-4 mb-6 text-[14px] leading-6 text-muted">
        Connections (MCP servers) give Relay extra tools, like your email, calendar or GitHub. They start when a task needs them.
      </p>
      {loadErr && <div className="mb-4 rounded-2xl bg-danger-soft px-4 py-3 text-[14px]">Couldn't read the connections: {loadErr}</div>}
      {cfg?.error && (
        <div className="mb-4 rounded-2xl bg-danger-soft px-4 py-3 text-[14px]">
          <div className="font-medium">The config file has a problem</div>
          <div className="mt-0.5 font-mono text-[12.5px] text-muted">{cfg.error}</div>
        </div>
      )}
      {!cfg && !loadErr ? (
        <div className="space-y-2">
          <div className="skeleton h-16 rounded-2xl" />
          <div className="skeleton h-16 rounded-2xl" />
        </div>
      ) : cfg ? (
        <>
          {names.length === 0 ? (
            <div className="rounded-2xl border border-dashed border-line px-4 py-10 text-center">
              <Plug size={22} className="mx-auto mb-2 text-subtle" />
              <p className="text-[14px] text-subtle">No connections yet.</p>
            </div>
          ) : (
            <ul className="divide-y divide-[var(--border)] rounded-2xl border border-line">
              {names.map((n) => (
                <li key={n} className="flex items-center gap-3 px-4 py-3">
                  <span className="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-hover text-[14px] font-semibold uppercase">{n.slice(0, 1)}</span>
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-[14.5px] font-medium">{n}</div>
                    <div className="truncate font-mono text-[12px] text-subtle">{describe(cfg.servers[n])}</div>
                  </div>
                  <Button size="sm" variant="ghost" onClick={() => setEditor({ mode: "edit", name: n })} aria-label={`Edit ${n}`}>
                    <Pencil size={14} /> Edit
                  </Button>
                  <Button size="sm" variant="ghost" className="text-danger hover:text-danger" onClick={() => remove(n)} aria-label={`Remove ${n}`}>
                    <Trash size={14} />
                  </Button>
                </li>
              ))}
            </ul>
          )}
          <div className="mt-4 flex flex-wrap gap-2">
            <Button variant="primary" onClick={() => setEditor({ mode: "add" })}>
              <Plus size={16} /> Add connection
            </Button>
            <Button variant="secondary" onClick={() => setEditor({ mode: "all" })}>
              <Braces size={15} /> Edit as JSON
            </Button>
          </div>
          <div className="mt-6 flex flex-wrap items-center gap-2 text-[12.5px] text-subtle">
            <FileCog size={14} />
            <span className="break-all font-mono">{cfg.path}</span>
            <button type="button" className="underline-offset-2 hover:text-fg hover:underline" onClick={() => api.openLink("mcp").catch((e) => toast(String(e), "error"))}>
              Open
            </button>
          </div>
          <ServerEditor editor={editor} cfg={cfg} onClose={() => setEditor(null)} onSave={save} />
        </>
      ) : null}
    </>
  );
}

function ServerEditor({ editor, cfg, onClose, onSave }: { editor: Editor; cfg: McpConfig; onClose: () => void; onSave: (s: Record<string, unknown>) => Promise<void> }) {
  const [name, setName] = useState("");
  const [text, setText] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const toast = useApp((s) => s.toast);

  useEffect(() => {
    setErr(null);
    setSaving(false);
    if (!editor) return;
    if (editor.mode === "add") {
      setName("");
      setText(TEMPLATE);
    } else if (editor.mode === "edit") {
      setName(editor.name);
      setText(JSON.stringify(cfg.servers[editor.name], null, 2));
    } else {
      setText(JSON.stringify(cfg.servers, null, 2));
    }
  }, [editor, cfg]);

  const whole = editor?.mode === "all";
  const submit = async () => {
    if (!editor) return;
    const parsed = parseServerJson(text, whole);
    if (!parsed.ok) return setErr(parsed.error);
    let next: Record<string, unknown>;
    if (whole) next = parsed.value;
    else {
      const n = name.trim();
      if (!n) return setErr("Give the connection a name.");
      if (!/^[\w.-]+$/.test(n)) return setErr("Use letters, numbers, dots, dashes or underscores in the name.");
      if (n in cfg.servers && !(editor.mode === "edit" && editor.name === n)) return setErr(`A connection called "${n}" already exists.`);
      next = { ...cfg.servers };
      if (editor.mode === "edit" && editor.name !== n) delete next[editor.name];
      next[n] = parsed.value;
    }
    setSaving(true);
    try {
      await onSave(next);
      toast("Connections saved");
      onClose();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
      setSaving(false);
    }
  };

  return (
    <DialogShell
      open={editor !== null}
      onOpenChange={(o) => !o && onClose()}
      title={editor?.mode === "add" ? "Add connection" : editor?.mode === "edit" ? `Edit ${editor.name}` : "Edit connections"}
      description={whole ? "The whole mcpServers object. Each key is a connection name." : "Paste the server's JSON from its documentation."}
      wide
    >
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
        className="space-y-4"
      >
        {!whole && (
          <label className="block">
            <span className="mb-1.5 block text-[13px] font-medium text-muted">Name</span>
            <input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. github" className={fieldClass} spellCheck={false} />
          </label>
        )}
        <label className="block">
          <span className="mb-1.5 block text-[13px] font-medium text-muted">JSON</span>
          <textarea
            value={text}
            onChange={(e) => {
              setText(e.target.value);
              setErr(null);
            }}
            onKeyDown={(e) => {
              if (e.key === "Tab" && !e.shiftKey) {
                e.preventDefault();
                const el = e.currentTarget;
                const s = el.selectionStart;
                const v = el.value;
                setText(v.slice(0, s) + "  " + v.slice(el.selectionEnd));
                requestAnimationFrame(() => el.setSelectionRange(s + 2, s + 2));
              }
            }}
            rows={whole ? 16 : 10}
            spellCheck={false}
            className={cn(fieldClass, "scroll-thin resize-y font-mono text-[13px] leading-5", err && "border-danger")}
            aria-invalid={Boolean(err)}
          />
        </label>
        {err && (
          <p role="alert" className="text-[13px] text-danger">
            {err}
          </p>
        )}
        <div className="flex items-center justify-end gap-2">
          <Button
            variant="ghost"
            className="mr-auto"
            onClick={() => {
              const p = parseServerJson(text, whole);
              if (p.ok) {
                setText(JSON.stringify(p.value, null, 2));
                setErr(null);
              } else setErr(p.error);
            }}
          >
            Format
          </Button>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" type="submit" disabled={saving}>
            Save
          </Button>
        </div>
      </form>
    </DialogShell>
  );
}
