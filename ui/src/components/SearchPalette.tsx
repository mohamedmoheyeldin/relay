import { useEffect, useMemo, useRef, useState } from "react";
import { Dialog, VisuallyHidden } from "radix-ui";
import { MessageCircle, Search, SquarePen, X } from "lucide-react";
import { useApp } from "../store/app";
import { api } from "../lib/backend";
import type { SearchResult } from "../lib/types";
import { bucketOf } from "../lib/grouping";
import { cn, FolderIcon, MOD, Spinner } from "./ui";

type Row =
  | { kind: "new"; key: string }
  | { kind: "chat"; key: string; id: string; title: string; snippet: string; meta: string }
  | { kind: "project"; key: string; id: string; name: string; color: string };

function Highlight({ text, q }: { text: string; q: string }) {
  const query = q.trim();
  if (!query) return <>{text}</>;
  const i = text.toLowerCase().indexOf(query.toLowerCase());
  if (i < 0) return <>{text}</>;
  return (
    <>
      {text.slice(0, i)}
      <mark className="rounded-sm bg-accent-soft text-fg">{text.slice(i, i + query.length)}</mark>
      {text.slice(i + query.length)}
    </>
  );
}

export function SearchPalette() {
  const open = useApp((s) => s.searchOpen);
  const setOpen = useApp((s) => s.setSearchOpen);
  const convs = useApp((s) => s.convs);
  const [q, setQ] = useState("");
  const [res, setRes] = useState<SearchResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [sel, setSel] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);
  const seq = useRef(0);

  useEffect(() => {
    if (!open) {
      setQ("");
      setRes(null);
      setErr(null);
      setSel(0);
    }
  }, [open]);

  // Debounced backend search; ignore out-of-order replies.
  useEffect(() => {
    const query = q.trim();
    if (!query) {
      setRes(null);
      setLoading(false);
      return;
    }
    setLoading(true);
    const my = ++seq.current;
    const t = setTimeout(() => {
      api.search(query).then(
        (r) => {
          if (my !== seq.current) return;
          setRes(r);
          setErr(null);
          setLoading(false);
        },
        (e) => {
          if (my !== seq.current) return;
          setErr(e instanceof Error ? e.message : String(e));
          setLoading(false);
        },
      );
    }, 160);
    return () => clearTimeout(t);
  }, [q]);

  const rows: Row[] = useMemo(() => {
    const out: Row[] = [];
    if (!q.trim()) {
      out.push({ kind: "new", key: "new" });
      for (const c of convs.filter((c) => !c.archived).slice(0, 8))
        out.push({ kind: "chat", key: c.id, id: c.id, title: c.title || "New chat", snippet: "", meta: bucketOf(c.updated_at, Date.now()) });
      return out;
    }
    if (!res) return out;
    for (const p of res.projects) out.push({ kind: "project", key: `p-${p.id}`, id: p.id, name: p.name, color: p.color });
    for (const c of res.conversations)
      out.push({ kind: "chat", key: c.id, id: c.id, title: c.title || "New chat", snippet: c.snippet, meta: bucketOf(c.updated_at, Date.now()) });
    return out;
  }, [q, res, convs]);

  useEffect(() => setSel(0), [rows]);
  useEffect(() => {
    listRef.current?.querySelector(`[data-idx="${sel}"]`)?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  const choose = (r: Row | undefined) => {
    if (!r) return;
    setOpen(false);
    const s = useApp.getState();
    if (r.kind === "new") s.newChat();
    else if (r.kind === "chat") void s.openChat(r.id);
    else s.go({ kind: "project", id: r.id });
  };

  const firstChat = rows.findIndex((r) => r.kind === "chat");
  const firstProject = rows.findIndex((r) => r.kind === "project");

  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Portal>
        <Dialog.Overlay className="fade-in fixed inset-0 z-[80] bg-[var(--scrim)]" />
        <Dialog.Content
          className="dialog-in fixed top-[14vh] left-1/2 -translate-x-1/2 z-[81] flex max-h-[min(520px,80vh)] w-[min(640px,calc(100vw-24px))] flex-col overflow-hidden rounded-2xl border border-line bg-elevated shadow-[var(--shadow)]"
          aria-describedby={undefined}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setSel((s) => Math.min(rows.length - 1, s + 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setSel((s) => Math.max(0, s - 1));
            } else if (e.key === "Enter" && !e.nativeEvent.isComposing) {
              e.preventDefault();
              choose(rows[sel]);
            }
          }}
        >
          <VisuallyHidden.Root>
            <Dialog.Title>Search chats</Dialog.Title>
          </VisuallyHidden.Root>
          <div className="flex h-14 shrink-0 items-center gap-3 border-b border-line px-4">
            <Search size={18} className="shrink-0 text-subtle" aria-hidden="true" />
            <input
              autoFocus
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder="Search chats and projects…"
              aria-label="Search chats and projects"
              role="combobox"
              aria-expanded="true"
              aria-controls="search-results"
              aria-activedescendant={rows[sel] ? `sr-${rows[sel]!.key}` : undefined}
              className="h-full flex-1 bg-transparent text-[16px] outline-none"
            />
            {loading && <Spinner size={16} className="text-subtle" />}
            <Dialog.Close className="inline-flex h-8 w-8 items-center justify-center rounded-lg text-muted hover:bg-hover hover:text-fg" aria-label="Close">
              <X size={18} />
            </Dialog.Close>
          </div>
          <div ref={listRef} id="search-results" role="listbox" className="scroll-thin min-h-0 flex-1 overflow-y-auto p-2">
            {err && <div className="px-3 py-6 text-center text-[14px] text-danger">Search failed: {err}</div>}
            {!err && q.trim() && res && rows.length === 0 && (
              <div className="px-3 py-10 text-center text-[14px] text-subtle">No chats or projects match "{q.trim()}".</div>
            )}
            {rows.map((r, i) => (
              <div key={r.key}>
                {i === firstProject && <div className="px-3 pt-2 pb-1 text-[12px] font-medium text-subtle">Projects</div>}
                {i === firstChat && <div className="px-3 pt-2 pb-1 text-[12px] font-medium text-subtle">{q.trim() ? "Chats" : "Recent chats"}</div>}
                <div
                  id={`sr-${r.key}`}
                  data-idx={i}
                  role="option"
                  aria-selected={i === sel}
                  onMouseMove={() => sel !== i && setSel(i)}
                  onClick={() => choose(r)}
                  className={cn("flex items-center gap-3 rounded-xl px-3 py-2.5", i === sel && "bg-hover")}
                >
                  {r.kind === "new" ? (
                    <>
                      <SquarePen size={17} className="shrink-0" aria-hidden="true" />
                      <span className="flex-1 text-[14.5px]">New chat</span>
                      <span className="text-[12px] text-subtle">{MOD}+Shift+O</span>
                    </>
                  ) : r.kind === "project" ? (
                    <>
                      <FolderIcon color={r.color} size={17} />
                      <span className="flex-1 truncate text-[14.5px]">
                        <Highlight text={r.name} q={q} />
                      </span>
                    </>
                  ) : (
                    <>
                      <MessageCircle size={17} className="shrink-0 text-muted" aria-hidden="true" />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-[14.5px]">
                          <Highlight text={r.title} q={q} />
                        </span>
                        {r.snippet && (
                          <span className="block truncate text-[13px] text-subtle">
                            <Highlight text={r.snippet} q={q} />
                          </span>
                        )}
                      </span>
                      <span className="shrink-0 text-[12px] text-subtle">{r.meta}</span>
                    </>
                  )}
                </div>
              </div>
            ))}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
