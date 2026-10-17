// Settings › Connections: the owner's MCP servers (mcp.json), edited per server or as raw JSON.
import { useEffect, useId, useState } from "react";
import { Braces, FileCog, Pencil, Plug, Plus, Trash2 } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { useMcp } from "@/hooks/queries";
import { useApp } from "@/store/app";
import { api, errorText } from "@/lib/backend";
import { qk } from "@/lib/query";
import { parseServerJson } from "@/lib/mcp";
import type { McpConfig } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { EmptyState, ErrorNote } from "@/components/common";
import { LoadState, PageTitle } from "./parts";

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
  const mcp = useMcp();
  const qc = useQueryClient();
  const [editor, setEditor] = useState<Editor>(null);
  const { toast, setConfirmDialog } = useApp.getState();
  const save = async (servers: Record<string, unknown>) => {
    await api.mcpSet(servers);
    await qc.invalidateQueries({ queryKey: qk.mcp });
  };
  const cfg = mcp.data;
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
          toast(`Couldn't save: ${errorText(e)}`, "error");
        }
      },
    });
  const names = cfg ? Object.keys(cfg.servers).sort() : [];

  return (
    <>
      <PageTitle description="Connections (MCP servers) give Relay extra tools, like your email, calendar or code hosting. They start only when a task needs them.">Connections</PageTitle>
      {!cfg ? (
        <LoadState error={mcp.error} retry={() => void mcp.refetch()} rows={2} />
      ) : (
        <>
          {cfg.error && (
            <div className="mb-4">
              <ErrorNote title="The config file has a problem">
                <span className="font-mono text-[12px]">{cfg.error}</span>
              </ErrorNote>
            </div>
          )}
          {names.length === 0 ? (
            <EmptyState icon={<Plug />} title="No connections yet" />
          ) : (
            <ul className="divide-y divide-line rounded-xl border border-line bg-surface">
              {names.map((n) => (
                <li key={n} className="flex items-center gap-3 px-4 py-3">
                  <span className="inline-flex size-9 shrink-0 items-center justify-center rounded-lg bg-sunk text-ui font-semibold text-fg-2 uppercase" aria-hidden="true">
                    {n.slice(0, 1)}
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-ui font-medium">{n}</div>
                    <div className="truncate font-mono text-[12px] text-fg-muted">{describe(cfg.servers[n])}</div>
                  </div>
                  <Button size="sm" variant="ghost" onClick={() => setEditor({ mode: "edit", name: n })} aria-label={`Edit ${n}`}>
                    <Pencil aria-hidden="true" /> Edit
                  </Button>
                  <Button size="sm" variant="danger-ghost" onClick={() => remove(n)} aria-label={`Remove ${n}`}>
                    <Trash2 aria-hidden="true" />
                  </Button>
                </li>
              ))}
            </ul>
          )}
          <div className="mt-4 flex flex-wrap gap-2">
            <Button variant="primary" onClick={() => setEditor({ mode: "add" })}>
              <Plus aria-hidden="true" /> Add connection
            </Button>
            <Button variant="secondary" onClick={() => setEditor({ mode: "all" })}>
              <Braces aria-hidden="true" /> Edit as JSON
            </Button>
          </div>
          <div className="mt-6 flex flex-wrap items-center gap-2 text-caption text-fg-muted">
            <FileCog className="size-3.5" aria-hidden="true" />
            <span className="font-mono break-all">{cfg.path}</span>
            <button type="button" className="text-brand-text underline-offset-2 hover:underline" onClick={() => api.openLink("mcp").catch((e) => toast(errorText(e), "error"))}>
              Open
            </button>
          </div>
          <ServerEditor editor={editor} cfg={cfg} onClose={() => setEditor(null)} onSave={save} />
        </>
      )}
    </>
  );
}

function ServerEditor({ editor, cfg, onClose, onSave }: { editor: Editor; cfg: McpConfig; onClose: () => void; onSave: (s: Record<string, unknown>) => Promise<void> }) {
  const [name, setName] = useState("");
  const [text, setText] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const nameId = useId();
  const jsonId = useId();

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
      useApp.getState().toast("Connections saved");
      onClose();
    } catch (e) {
      setErr(errorText(e));
      setSaving(false);
    }
  };

  return (
    <Dialog open={editor !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="w-[min(640px,calc(100vw-24px))]">
        <DialogHeader>
          <DialogTitle>{editor?.mode === "add" ? "Add connection" : editor?.mode === "edit" ? `Edit ${editor.name}` : "Edit connections"}</DialogTitle>
          <DialogDescription>{whole ? "The whole mcpServers object. Each key is a connection name." : "Paste the server's JSON from its documentation."}</DialogDescription>
        </DialogHeader>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void submit();
          }}
          className="space-y-4"
        >
          {!whole && (
            <div className="space-y-1.5">
              <Label htmlFor={nameId}>Name</Label>
              <Input id={nameId} autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. github" spellCheck={false} />
            </div>
          )}
          <div className="space-y-1.5">
            <Label htmlFor={jsonId}>JSON</Label>
            <Textarea
              id={jsonId}
              value={text}
              onChange={(e) => {
                setText(e.target.value);
                setErr(null);
              }}
              rows={whole ? 16 : 10}
              spellCheck={false}
              className={cn("font-mono text-[13px] leading-5", err && "border-danger")}
              aria-invalid={Boolean(err)}
              aria-describedby={err ? `${jsonId}-err` : undefined}
            />
            <p className="text-caption text-fg-muted">Use Format to tidy the indentation.</p>
          </div>
          {err && (
            <p id={`${jsonId}-err`} role="alert" className="text-caption text-danger">
              {err}
            </p>
          )}
          <DialogFooter className="items-center">
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
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
