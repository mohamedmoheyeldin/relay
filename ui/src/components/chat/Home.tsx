import { useMemo, useState } from "react";
import { FolderPlus, HardDrive, ListChecks, Search, Sparkles } from "lucide-react";
import { useApp } from "@/store/app";
import { useProjects, useSettings } from "@/hooks/queries";
import { cn } from "@/lib/utils";
import { RelayMark, FolderIcon } from "@/components/common";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Composer } from "./Composer";
import { ContextControls } from "./ContextControls";

export const SUGGESTIONS = [
  { icon: Sparkles, label: "Tidy up my Downloads folder" },
  { icon: HardDrive, label: "Find what is using my disk space" },
  { icon: ListChecks, label: "Open Notepad and type a shopping list" },
];

export function greeting(name: string | undefined, hour = new Date().getHours()): string {
  const part = hour < 5 ? "Working late" : hour < 12 ? "Good morning" : hour < 18 ? "Good afternoon" : "Good evening";
  const first = name?.trim().split(/\s+/)[0];
  return first ? `${part}, ${first}` : part;
}

export function Home() {
  const [projectId, setProjectId] = useState<string | null>(null);
  const settings = useSettings();
  const fill = useApp((s) => s.fillComposer);
  // The name arrives a moment after first paint; reserve the line so nothing shifts.
  const title = greeting(settings.data?.user_name);

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-4 scroll-thin">
      <div className="m-auto flex w-full max-w-[680px] flex-col items-center pt-6 pb-[10vh]">
        <RelayMark size={34} className="mb-4" />
        <h1 className="mb-8 min-h-[42px] text-center font-display text-[30px] leading-[42px] tracking-[-0.015em] text-fg sm:text-display">{title}</h1>
        <Composer projectId={projectId} onClearProject={() => setProjectId(null)} autoFocus leading={!projectId ? <ProjectPicker value={projectId} onChange={setProjectId} /> : undefined} />
        <ContextControls className="mt-3" />
        <div className="mt-8 flex flex-wrap justify-center gap-2">
          {SUGGESTIONS.map((s) => (
            <button
              key={s.label}
              type="button"
              onClick={() => fill(s.label)}
              className="inline-flex h-8 items-center gap-2 rounded-full border border-line px-3 text-caption text-fg-2 transition-colors duration-(--relay-dur-fast) hover:bg-hover hover:text-fg"
            >
              <s.icon className="size-3.5 text-fg-muted" aria-hidden="true" />
              {s.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

export function ProjectPicker({ value, onChange }: { value: string | null; onChange: (id: string | null) => void }) {
  const projects = useProjects();
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const list = useMemo(() => (projects.data ?? []).filter((p) => p.name.toLowerCase().includes(q.trim().toLowerCase())), [projects.data, q]);
  const { setProjectDialog } = useApp.getState();

  return (
    <Popover
      open={open}
      onOpenChange={(o) => {
        setOpen(o);
        if (!o) setQ("");
      }}
    >
      <PopoverTrigger
        className={cn(
          "inline-flex h-8 items-center gap-1.5 rounded-full px-2.5 text-caption text-fg-muted transition-colors hover:bg-hover hover:text-fg data-[state=open]:bg-hover",
        )}
        aria-label="Choose a project for this chat"
      >
        <FolderPlus className="size-4" aria-hidden="true" />
        <span className="hidden sm:inline">Project</span>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-[260px] p-1">
        <div className="flex items-center gap-2 border-b border-line px-2 pt-1 pb-2">
          <Search className="size-4 text-fg-muted" aria-hidden="true" />
          <input
            autoFocus
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="Search projects"
            aria-label="Search projects"
            className="h-7 w-full bg-transparent text-ui outline-none placeholder:text-fg-muted"
          />
        </div>
        <div className="mt-1 max-h-64 overflow-y-auto scroll-thin" role="listbox" aria-label="Projects">
          {list.map((p) => (
            <button
              key={p.id}
              type="button"
              role="option"
              aria-selected={p.id === value}
              className="flex h-8 w-full items-center gap-2.5 rounded-sm px-2 text-left text-ui hover:bg-hover focus-visible:bg-hover"
              onClick={() => {
                onChange(p.id === value ? null : p.id);
                setOpen(false);
              }}
            >
              <FolderIcon color={p.color} size={15} />
              <span className="truncate">{p.name}</span>
            </button>
          ))}
          {projects.data && list.length === 0 && <p className="px-2 py-2 text-caption text-fg-muted">{projects.data.length ? "No matching projects" : "No projects yet"}</p>}
          <button
            type="button"
            className="mt-1 flex h-8 w-full items-center gap-2.5 rounded-sm border-t border-line px-2 text-left text-ui text-fg-2 hover:bg-hover"
            onClick={() => {
              setOpen(false);
              setProjectDialog({ mode: "create" });
            }}
          >
            <FolderPlus className="size-4 text-fg-muted" aria-hidden="true" /> New project…
          </button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
