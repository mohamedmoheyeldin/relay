import { useMemo, useState } from "react";
import { Popover } from "radix-ui";
import { Folder, FolderPlus, Globe, HardDrive, Mail, Search, Sparkles } from "lucide-react";
import { useApp } from "../store/app";
import { Composer } from "./Composer";
import { cn, FolderIcon } from "./ui";

const HEADINGS = ["What should we work on?", "What can I help with?", "Ready when you are.", "What's on your list today?", "Where should we start?"];

export const SUGGESTIONS = [
  { icon: Sparkles, label: "Clean up my Downloads folder", color: "#3fae6a" },
  { icon: HardDrive, label: "Find what is using my disk space", color: "#4c8dff" },
  { icon: Mail, label: "Open my email and summarise it", color: "#ec7a35" },
  { icon: Globe, label: "Search the web for ", text: "Search the web for ", color: "#a371f7" },
];

export function Home() {
  const [heading] = useState(() => HEADINGS[Math.floor(Math.random() * HEADINGS.length)]!);
  const [projectId, setProjectId] = useState<string | null>(null);
  const fill = useApp((s) => s.fillComposer);

  return (
    <div className="scroll-thin flex min-h-0 flex-1 flex-col overflow-y-auto px-4">
      <div className="m-auto flex w-full max-w-[768px] flex-col items-center pb-[8vh]">
        <h1 className="mb-7 text-center text-[28px] leading-9 font-normal tracking-[-0.01em] text-fg">{heading}</h1>
        <Composer projectId={projectId} onClearProject={() => setProjectId(null)} autoFocus />
        <div className="mt-3 flex flex-wrap justify-center gap-2">
          <ProjectPicker value={projectId} onChange={setProjectId} />
          {SUGGESTIONS.map((s) => (
            <button
              key={s.label}
              type="button"
              onClick={() => fill(s.text ?? s.label)}
              className="inline-flex h-9 items-center gap-2 rounded-full border border-line px-3.5 text-[13.5px] text-muted transition-colors duration-150 hover:bg-hover hover:text-fg"
            >
              <s.icon size={15} style={{ color: s.color }} aria-hidden="true" />
              {s.text ? "Search the web" : s.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

export function ProjectPicker({ value, onChange }: { value: string | null; onChange: (id: string | null) => void }) {
  const projects = useApp((s) => s.projects);
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const selected = projects.find((p) => p.id === value);
  const list = useMemo(() => projects.filter((p) => p.name.toLowerCase().includes(q.trim().toLowerCase())), [projects, q]);
  const { setProjectDialog } = useApp.getState();

  return (
    <Popover.Root
      open={open}
      onOpenChange={(o) => {
        setOpen(o);
        if (!o) setQ("");
      }}
    >
      <Popover.Trigger asChild>
        <button
          type="button"
          className={cn(
            "inline-flex h-9 items-center gap-2 rounded-full border border-line px-3.5 text-[13.5px] text-muted transition-colors duration-150 hover:bg-hover hover:text-fg data-[state=open]:bg-hover",
            selected && "text-fg",
          )}
        >
          {selected ? <FolderIcon color={selected.color} size={15} /> : <Folder size={15} aria-hidden="true" />}
          {selected ? selected.name : "Choose project"}
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content align="start" sideOffset={8} className="menu pop-in w-[260px] p-1.5" onOpenAutoFocus={(e) => e.preventDefault()}>
          <div className="flex items-center gap-2 border-b border-line px-2.5 pb-2 pt-1">
            <Search size={15} className="text-subtle" aria-hidden="true" />
            <input
              autoFocus
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder="Search projects"
              aria-label="Search projects"
              className="h-7 w-full bg-transparent text-[14px] outline-none"
            />
          </div>
          <div className="scroll-thin mt-1.5 max-h-64 overflow-y-auto">
            <button
              type="button"
              className="menu-item w-full bg-hover/60 text-left hover:bg-hover"
              onClick={() => {
                setOpen(false);
                setProjectDialog({ mode: "create" });
              }}
            >
              <FolderPlus size={16} /> New project
            </button>
            {list.map((p) => (
              <button
                key={p.id}
                type="button"
                className="menu-item w-full text-left hover:bg-hover"
                onClick={() => {
                  onChange(p.id === value ? null : p.id);
                  setOpen(false);
                }}
              >
                <FolderIcon color={p.color} size={16} />
                <span className="truncate">{p.name}</span>
                {p.id === value && <span className="kbd">Selected</span>}
              </button>
            ))}
            {projects.length > 0 && list.length === 0 && <div className="menu-label">No matching projects</div>}
          </div>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
