// Ctrl/Cmd+K: search chats and projects, and run commands. Combobox + listbox with
// aria-activedescendant; Up/Down/Home/End move, Enter opens, Esc closes.
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Dialog as DialogPrimitive, VisuallyHidden } from "radix-ui";
import { History, Keyboard, MessageSquare, Moon, Search, Settings, ShieldCheck, SquarePen, Sun } from "lucide-react";
import { useApp } from "@/store/app";
import { useAppearance } from "@/store/appearance";
import { useConversations, useSearch } from "@/hooks/queries";
import { bucketOf } from "@/lib/grouping";
import { go, paths } from "@/lib/nav";
import { shortcutKeys, type Keys } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { FolderIcon } from "@/components/common";
import { Shortcut } from "@/components/ui/kbd";
import { overlayClass } from "@/components/ui/dialog";

type Row =
  | { kind: "command"; key: string; label: string; icon: ReactNode; keys?: Keys; run: () => void; keywords: string }
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
      <mark className="rounded-xs bg-brand-soft text-fg">{text.slice(i, i + query.length)}</mark>
      {text.slice(i + query.length)}
    </>
  );
}

export function SearchPalette() {
  const open = useApp((s) => s.searchOpen);
  const setOpen = useApp((s) => s.setSearchOpen);
  return (
    <DialogPrimitive.Root open={open} onOpenChange={setOpen}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className={cn(overlayClass, "z-(--relay-z-palette)")} />
        {open && <PaletteBody onClose={() => setOpen(false)} />}
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}

function PaletteBody({ onClose }: { onClose: () => void }) {
  const [q, setQ] = useState("");
  const [debounced, setDebounced] = useState("");
  const [sel, setSel] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);
  const convs = useConversations();
  const search = useSearch(debounced);
  const scheme = useAppearance((s) => s.scheme);

  useEffect(() => {
    const t = setTimeout(() => setDebounced(q.trim()), 140);
    return () => clearTimeout(t);
  }, [q]);

  const commands: Row[] = useMemo(
    () => [
      { kind: "command", key: "cmd-new", label: "New chat", icon: <SquarePen />, keys: shortcutKeys("newChat"), run: () => go(paths.home()), keywords: "new chat start" },
      { kind: "command", key: "cmd-tasks", label: "Task history", icon: <History />, keys: shortcutKeys("tasks"), run: () => go(paths.tasks()), keywords: "tasks history runs workflows" },
      { kind: "command", key: "cmd-privacy", label: "AI & Privacy settings", icon: <ShieldCheck />, run: () => go(paths.settings("privacy")), keywords: "privacy provider claude codex local cloud share" },
      { kind: "command", key: "cmd-settings", label: "Settings", icon: <Settings />, keys: shortcutKeys("settings"), run: () => go(paths.settings()), keywords: "settings preferences" },
      { kind: "command", key: "cmd-keys", label: "Keyboard shortcuts", icon: <Keyboard />, keys: shortcutKeys("shortcuts"), run: () => go(paths.settings("keyboard")), keywords: "keyboard shortcuts keys" },
      {
        kind: "command",
        key: "cmd-theme",
        label: scheme === "dark" ? "Switch to light mode" : "Switch to dark mode",
        icon: scheme === "dark" ? <Sun /> : <Moon />,
        run: () => useAppearance.getState().set({ mode: scheme === "dark" ? "light" : "dark" }),
        keywords: "theme dark light mode appearance",
      },
    ],
    [scheme],
  );

  const query = q.trim().toLowerCase();
  const rows: Row[] = useMemo(() => {
    const out: Row[] = [];
    const cmd = commands.filter((c) => c.kind === "command" && (!query || c.label.toLowerCase().includes(query) || c.keywords.includes(query)));
    if (!query) {
      out.push(...cmd.slice(0, 3));
      for (const c of (convs.data ?? []).slice(0, 8))
        out.push({ kind: "chat", key: c.id, id: c.id, title: c.title || "New chat", snippet: "", meta: bucketOf(c.updated_at, Date.now()) });
      return out;
    }
    out.push(...cmd);
    const res = search.data && debounced ? search.data : null;
    if (res) {
      for (const p of res.projects) out.push({ kind: "project", key: `p-${p.id}`, id: p.id, name: p.name, color: p.color });
      for (const c of res.conversations)
        out.push({ kind: "chat", key: c.id, id: c.id, title: c.title || "New chat", snippet: c.snippet, meta: bucketOf(c.updated_at, Date.now()) });
    }
    return out;
  }, [commands, query, convs.data, search.data, debounced]);

  useEffect(() => setSel(0), [query, rows.length]);
  useEffect(() => {
    listRef.current?.querySelector(`[data-idx="${sel}"]`)?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  const choose = (r: Row | undefined) => {
    if (!r) return;
    onClose();
    if (r.kind === "command") r.run();
    else if (r.kind === "chat") go(paths.chat(r.id));
    else go(paths.project(r.id));
  };

  const firstOf = (k: Row["kind"]) => rows.findIndex((r) => r.kind === k);
  const heading: Record<number, string> = {};
  if (firstOf("command") >= 0) heading[firstOf("command")] = query ? "Commands" : "Quick actions";
  if (firstOf("project") >= 0) heading[firstOf("project")] = "Projects";
  if (firstOf("chat") >= 0) heading[firstOf("chat")] = query ? "Chats" : "Recent chats";
  const searching = Boolean(query) && (search.isFetching || debounced !== query);

  return (
    <DialogPrimitive.Content
      className={cn(
        "fixed top-[12vh] left-1/2 z-(--relay-z-palette) flex max-h-[min(540px,76vh)] w-[min(620px,calc(100vw-24px))] -translate-x-1/2 flex-col overflow-hidden rounded-xl border border-line bg-raised shadow-dialog outline-none",
        "duration-(--relay-dur) data-open:animate-in data-open:fade-in-0 data-open:zoom-in-[0.98]",
      )}
      aria-describedby={undefined}
      data-testid="search-palette"
      onKeyDown={(e) => {
        if (e.key === "ArrowDown") {
          e.preventDefault();
          setSel((s) => Math.min(rows.length - 1, s + 1));
        } else if (e.key === "ArrowUp") {
          e.preventDefault();
          setSel((s) => Math.max(0, s - 1));
        } else if (e.key === "Home" && e.ctrlKey) {
          e.preventDefault();
          setSel(0);
        } else if (e.key === "End" && e.ctrlKey) {
          e.preventDefault();
          setSel(rows.length - 1);
        } else if (e.key === "Enter" && !e.nativeEvent.isComposing) {
          e.preventDefault();
          choose(rows[sel]);
        }
      }}
    >
      <VisuallyHidden.Root>
        <DialogPrimitive.Title>Search and commands</DialogPrimitive.Title>
      </VisuallyHidden.Root>
      <div className="flex h-13 shrink-0 items-center gap-3 border-b border-line px-4">
        <Search className="size-[18px] shrink-0 text-fg-muted" aria-hidden="true" />
        <input
          autoFocus
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Search chats, projects and commands"
          aria-label="Search chats, projects and commands"
          role="combobox"
          aria-expanded="true"
          aria-controls="palette-results"
          aria-autocomplete="list"
          aria-activedescendant={rows[sel] ? `pal-${rows[sel]!.key}` : undefined}
          className="h-full flex-1 bg-transparent text-body text-fg outline-none placeholder:text-fg-muted"
        />
        <kbd className="hidden rounded-xs border border-line px-1.5 py-0.5 text-micro text-fg-muted sm:inline">Esc</kbd>
      </div>
      <div ref={listRef} id="palette-results" role="listbox" aria-label="Results" className="min-h-0 flex-1 overflow-y-auto p-1.5 scroll-thin">
        {search.isError && query && <div className="px-3 py-6 text-center text-ui text-danger">Search failed. Try again.</div>}
        {rows.map((r, i) => (
          <div key={r.key}>
            {heading[i] && <div className="px-2.5 pt-2 pb-1 text-micro font-medium text-fg-muted">{heading[i]}</div>}
            <div
              id={`pal-${r.key}`}
              data-idx={i}
              role="option"
              aria-selected={i === sel}
              onMouseMove={() => sel !== i && setSel(i)}
              onClick={() => choose(r)}
              className={cn("flex min-h-10 items-center gap-3 rounded-md px-2.5 py-2", i === sel && "bg-hover")}
            >
              {r.kind === "command" ? (
                <>
                  <span className="text-fg-2 [&_svg]:size-4" aria-hidden="true">
                    {r.icon}
                  </span>
                  <span className="flex-1 text-ui">
                    <Highlight text={r.label} q={q} />
                  </span>
                  {r.keys && <Shortcut keys={r.keys} />}
                </>
              ) : r.kind === "project" ? (
                <>
                  <FolderIcon color={r.color} size={16} />
                  <span className="flex-1 truncate text-ui">
                    <Highlight text={r.name} q={q} />
                  </span>
                  <span className="text-caption text-fg-muted">Project</span>
                </>
              ) : (
                <>
                  <MessageSquare className="size-4 shrink-0 text-fg-muted" aria-hidden="true" />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-ui">
                      <Highlight text={r.title} q={q} />
                    </span>
                    {r.snippet && (
                      <span className="block truncate text-caption text-fg-muted">
                        <Highlight text={r.snippet} q={q} />
                      </span>
                    )}
                  </span>
                  <span className="shrink-0 text-caption text-fg-muted">{r.meta}</span>
                </>
              )}
            </div>
          </div>
        ))}
        {query && !searching && rows.length === 0 && <div className="px-3 py-10 text-center text-ui text-fg-muted">Nothing matches “{q.trim()}”.</div>}
        {searching && rows.length === 0 && (
          <div className="space-y-2 p-2" aria-label="Searching">
            <div className="skeleton h-9" />
            <div className="skeleton h-9" />
          </div>
        )}
      </div>
    </DialogPrimitive.Content>
  );
}
