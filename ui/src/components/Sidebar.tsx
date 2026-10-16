import { memo, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { ContextMenu, DropdownMenu } from "radix-ui";
import {
  CircleQuestionMark,
  Ellipsis,
  Keyboard,
  MessageCircle,
  OctagonX,
  PanelLeft,
  Pencil,
  Pin,
  PinOff,
  Plug,
  Plus,
  Search,
  Settings as SettingsIcon,
  SlidersHorizontal,
  SquarePen,
  Trash,
} from "lucide-react";
import { useApp } from "../store/app";
import { groupByDate, sidebarSections } from "../lib/grouping";
import type { ConvSummary, Project } from "../lib/types";
import { Avatar, cn, FolderIcon, IconButton, MOD, RelayMark, Tip } from "./ui";
import { ChatMenuItems } from "./menus";
import { useNarrow } from "./hooks";

export function Sidebar() {
  const open = useApp((s) => s.sidebarOpen);
  const narrow = useNarrow();
  const setSidebar = useApp((s) => s.setSidebar);

  // Entering the narrow layout closes the overlay; going wide restores the saved preference.
  const first = useRef(true);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    if (narrow) useApp.setState({ sidebarOpen: false });
    else {
      let saved: string | null = null;
      try {
        saved = localStorage.getItem("relay.sidebarOpen");
      } catch {
        /* ignore */
      }
      useApp.setState({ sidebarOpen: saved !== "false" });
    }
  }, [narrow]);

  if (narrow) {
    return (
      <>
        <div
          aria-hidden="true"
          onClick={() => setSidebar(false)}
          className={cn(
            "fixed inset-0 z-40 bg-[var(--scrim)] transition-opacity duration-200",
            open ? "opacity-100" : "pointer-events-none opacity-0",
          )}
        />
        <aside
          aria-label="Sidebar"
          aria-hidden={!open}
          inert={!open}
          className={cn(
            "fixed inset-y-0 left-0 z-50 w-[min(var(--sidebar-w),86vw)] bg-sidebar shadow-2xl transition-transform duration-200 ease-[var(--ease-out-soft)] will-change-transform",
            open ? "translate-x-0" : "-translate-x-full",
          )}
        >
          <SidebarContent />
        </aside>
      </>
    );
  }
  return (
    <aside
      aria-label="Sidebar"
      inert={!open}
      className={cn(
        "relative h-full shrink-0 overflow-hidden bg-sidebar transition-[width] duration-200 ease-[var(--ease-out-soft)]",
        open ? "w-[var(--sidebar-w)] border-r border-line" : "w-0",
      )}
      data-theme-border
    >
      <div className={cn("h-full w-[var(--sidebar-w)] transition-opacity duration-150", open ? "opacity-100" : "opacity-0")}>
        <SidebarContent />
      </div>
    </aside>
  );
}

function NavRow({ icon, label, kbd, onClick, active }: { icon: ReactNode; label: string; kbd?: string; onClick: () => void; active?: boolean }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "group flex h-9 w-full items-center gap-2.5 rounded-lg px-2.5 text-left text-[14px] text-fg transition-colors duration-100 hover:bg-hover",
        active && "bg-active",
      )}
    >
      <span className="flex w-5 justify-center text-fg">{icon}</span>
      <span className="flex-1 truncate">{label}</span>
      {kbd && <span className="text-xs text-subtle opacity-0 transition-opacity group-hover:opacity-100">{kbd}</span>}
    </button>
  );
}

function SectionTitle({ children, action }: { children: ReactNode; action?: ReactNode }) {
  return (
    <div className="group/sec flex h-8 items-center justify-between px-2.5 pt-1">
      <h2 className="text-[13px] font-medium text-subtle">{children}</h2>
      {action}
    </div>
  );
}

function SidebarContent() {
  const { newChat, setSearchOpen, openSettings, toggleSidebar, setProjectDialog } = useApp.getState();
  const convs = useApp((s) => s.convs);
  const convsLoaded = useApp((s) => s.convsLoaded);
  const projects = useApp((s) => s.projects);
  const projectsLoaded = useApp((s) => s.projectsLoaded);
  const pinnedProjects = useApp((s) => s.pinnedProjects);
  const isHome = useApp((s) => s.view.kind === "home");

  const { pinned, recents } = useMemo(() => sidebarSections(convs), [convs]);
  const groups = useMemo(() => groupByDate(recents, Date.now()), [recents]);
  const pinnedP = useMemo(() => projects.filter((p) => pinnedProjects.includes(p.id)), [projects, pinnedProjects]);

  return (
    <nav className="flex h-full flex-col" aria-label="Chats and projects">
      <div className="flex h-[52px] shrink-0 items-center justify-between px-3">
        <button type="button" onClick={() => newChat()} className="flex items-center gap-2 rounded-lg px-1.5 py-1 hover:bg-hover" aria-label="Relay home">
          <RelayMark size={20} />
          <span className="text-[17px] font-semibold tracking-[-0.01em]">Relay</span>
        </button>
        <IconButton label="Close sidebar" kbd={`${MOD}+Shift+S`} onClick={toggleSidebar}>
          <PanelLeft size={19} />
        </IconButton>
      </div>

      <div className="space-y-px px-2 pb-2">
        <NavRow icon={<SquarePen size={17} />} label="New chat" kbd={`${MOD}+Shift+O`} onClick={() => newChat()} active={isHome} />
        <NavRow icon={<Search size={17} />} label="Search chats" kbd={`${MOD}+K`} onClick={() => setSearchOpen(true)} />
        <NavRow icon={<Plug size={17} />} label="Connections" onClick={() => openSettings("connections")} />
      </div>

      <div className="scroll-thin min-h-0 flex-1 overflow-y-auto px-2 pb-3">
        {(pinned.length > 0 || pinnedP.length > 0) && (
          <section className="mb-3">
            <SectionTitle>Pinned</SectionTitle>
            {pinnedP.map((p) => (
              <ProjectRow key={p.id} project={p} pinned />
            ))}
            {pinned.map((c) => (
              <ChatRow key={c.id} conv={c} icon />
            ))}
          </section>
        )}

        <section className="mb-3">
          <SectionTitle
            action={
              <IconButton label="New project" className="h-7 w-7" onClick={() => setProjectDialog({ mode: "create" })}>
                <Plus size={16} />
              </IconButton>
            }
          >
            Projects
          </SectionTitle>
          {!projectsLoaded ? (
            <SkeletonRows n={2} />
          ) : projects.length === 0 ? (
            <button
              type="button"
              onClick={() => setProjectDialog({ mode: "create" })}
              className="flex h-9 w-full items-center gap-2.5 rounded-lg px-2.5 text-[14px] text-subtle hover:bg-hover hover:text-fg"
            >
              <Plus size={16} /> New project
            </button>
          ) : (
            projects.filter((p) => !pinnedProjects.includes(p.id)).map((p) => <ProjectRow key={p.id} project={p} />)
          )}
        </section>

        <section>
          {!convsLoaded ? (
            <>
              <SectionTitle>Recents</SectionTitle>
              <SkeletonRows n={6} />
            </>
          ) : groups.length === 0 ? (
            <>
              <SectionTitle>Recents</SectionTitle>
              <p className="px-2.5 py-1 text-[13px] text-subtle">Your chats will show up here.</p>
            </>
          ) : (
            groups.map((g) => (
              <div key={g.label} className="mb-3">
                <SectionTitle>{g.label}</SectionTitle>
                {g.items.map((c) => (
                  <ChatRow key={c.id} conv={c} />
                ))}
              </div>
            ))
          )}
        </section>
      </div>

      <ProfileFooter />
    </nav>
  );
}

function SkeletonRows({ n }: { n: number }) {
  return (
    <div className="space-y-2 px-2.5 py-1.5" aria-hidden="true">
      {Array.from({ length: n }, (_, i) => (
        <div key={i} className="skeleton h-4" style={{ width: `${55 + ((i * 37) % 40)}%` }} />
      ))}
    </div>
  );
}

const ChatRow = memo(function ChatRow({ conv, icon }: { conv: ConvSummary; icon?: boolean }) {
  const active = useApp((s) => s.view.kind === "chat" && s.view.id === conv.id);
  const running = useApp((s) => s.activeRun?.conv_id === conv.id || s.sending === conv.id);
  const openChat = useApp((s) => s.openChat);
  const renameConv = useApp((s) => s.renameConv);
  const [editing, setEditing] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const title = conv.title || "New chat";

  if (editing) {
    return <RenameInput initial={title} onDone={(v) => { setEditing(false); if (v !== null && v !== title) renameConv(conv.id, v); }} />;
  }

  return (
    <ContextMenu.Root>
      <ContextMenu.Trigger asChild>
        <div
          className={cn(
            "group relative flex h-9 items-center rounded-lg transition-colors duration-100 hover:bg-hover",
            (active || menuOpen) && "bg-active hover:bg-active",
          )}
        >
          <button
            type="button"
            onClick={() => openChat(conv.id)}
            onDoubleClick={() => setEditing(true)}
            aria-current={active ? "page" : undefined}
            className="flex h-full min-w-0 flex-1 items-center gap-2.5 rounded-lg pr-8 pl-2.5 text-left text-[14px]"
          >
            {icon && <MessageCircle size={16} className="shrink-0 text-muted" aria-hidden="true" />}
            <span className="truncate">{title}</span>
            {running && <span className="ml-auto h-2 w-2 shrink-0 rounded-full bg-accent" aria-label="Working" />}
          </button>
          <DropdownMenu.Root open={menuOpen} onOpenChange={setMenuOpen}>
            <DropdownMenu.Trigger asChild>
              <button
                type="button"
                aria-label={`Options for ${title}`}
                className={cn(
                  "absolute right-1 inline-flex h-7 w-7 items-center justify-center rounded-md text-muted hover:text-fg focus-visible:opacity-100",
                  menuOpen || active ? "opacity-100" : "opacity-0 group-hover:opacity-100",
                )}
              >
                <Ellipsis size={17} />
              </button>
            </DropdownMenu.Trigger>
            <DropdownMenu.Portal>
              <DropdownMenu.Content className="menu pop-in" align="start" sideOffset={4}>
                <ChatMenuItems conv={conv} onRename={() => setEditing(true)} />
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
        </div>
      </ContextMenu.Trigger>
      <ContextMenu.Portal>
        <ContextMenu.Content className="menu pop-in">
          <ChatMenuItems conv={conv} ctx onRename={() => setEditing(true)} />
        </ContextMenu.Content>
      </ContextMenu.Portal>
    </ContextMenu.Root>
  );
});

function RenameInput({ initial, onDone }: { initial: string; onDone: (v: string | null) => void }) {
  const ref = useRef<HTMLInputElement>(null);
  const done = useRef(false);
  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);
  const finish = (v: string | null) => {
    if (done.current) return;
    done.current = true;
    onDone(v && v.trim() ? v.trim() : null);
  };
  return (
    <div className="flex h-9 items-center rounded-lg bg-active px-1.5">
      <input
        ref={ref}
        defaultValue={initial}
        aria-label="Chat title"
        className="h-7 w-full rounded-md border border-accent bg-bg px-2 text-[14px] outline-none"
        onKeyDown={(e) => {
          if (e.key === "Enter") finish(e.currentTarget.value);
          if (e.key === "Escape") {
            e.stopPropagation();
            finish(null);
          }
        }}
        onBlur={(e) => finish(e.currentTarget.value)}
      />
    </div>
  );
}

function ProjectRow({ project, pinned }: { project: Project; pinned?: boolean }) {
  const active = useApp((s) => s.view.kind === "project" && s.view.id === project.id);
  const { go, togglePinProject, setProjectDialog, setConfirmDialog, deleteProject } = useApp.getState();
  const [menuOpen, setMenuOpen] = useState(false);
  const items = (Mn: typeof DropdownMenu) => (
    <>
      <Mn.Item className="menu-item" onSelect={() => setTimeout(() => setProjectDialog({ mode: "edit", project }), 0)}>
        <Pencil size={16} /> Edit project
      </Mn.Item>
      <Mn.Item className="menu-item" onSelect={() => togglePinProject(project.id)}>
        {pinned ? <PinOff size={16} /> : <Pin size={16} />} {pinned ? "Unpin" : "Pin"}
      </Mn.Item>
      <Mn.Separator className="menu-sep" />
      <Mn.Item
        className="menu-item danger"
        onSelect={() =>
          setTimeout(
            () =>
              setConfirmDialog({
                title: "Delete project?",
                body: `"${project.name}" will be deleted. Its chats stay and move back to your recents.`,
                action: "Delete",
                destructive: true,
                onConfirm: () => deleteProject(project.id),
              }),
            0,
          )
        }
      >
        <Trash size={16} /> Delete project
      </Mn.Item>
    </>
  );
  return (
    <ContextMenu.Root>
      <ContextMenu.Trigger asChild>
        <div className={cn("group relative flex h-9 items-center rounded-lg hover:bg-hover", (active || menuOpen) && "bg-active hover:bg-active")}>
          <button
            type="button"
            onClick={() => go({ kind: "project", id: project.id })}
            aria-current={active ? "page" : undefined}
            className="flex h-full min-w-0 flex-1 items-center gap-2.5 rounded-lg pr-8 pl-2.5 text-left text-[14px]"
          >
            <FolderIcon color={project.color} size={17} />
            <span className="truncate">{project.name}</span>
          </button>
          <DropdownMenu.Root open={menuOpen} onOpenChange={setMenuOpen}>
            <DropdownMenu.Trigger asChild>
              <button
                type="button"
                aria-label={`Options for ${project.name}`}
                className={cn(
                  "absolute right-1 inline-flex h-7 w-7 items-center justify-center rounded-md text-muted hover:text-fg",
                  menuOpen ? "opacity-100" : "opacity-0 group-hover:opacity-100 focus-visible:opacity-100",
                )}
              >
                <Ellipsis size={17} />
              </button>
            </DropdownMenu.Trigger>
            <DropdownMenu.Portal>
              <DropdownMenu.Content className="menu pop-in" align="start" sideOffset={4}>
                {items(DropdownMenu)}
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
        </div>
      </ContextMenu.Trigger>
      <ContextMenu.Portal>
        <ContextMenu.Content className="menu pop-in">{items(ContextMenu as unknown as typeof DropdownMenu)}</ContextMenu.Content>
      </ContextMenu.Portal>
    </ContextMenu.Root>
  );
}

function ProfileFooter() {
  const settings = useApp((s) => s.settings);
  const status = useApp((s) => s.status);
  const busy = useApp((s) => Boolean(s.activeRun));
  const { openSettings } = useApp.getState();
  const name = settings?.user_name?.trim() || "You";
  const statusLine = !status
    ? "Connecting…"
    : busy || status.busy
      ? "Working…"
      : status.route_mode === "local_only"
        ? `Local AI${status.local_model ? ` · ${status.local_model}` : ""}`
        : status.route_mode === "cloud_only"
          ? "Claude"
          : status.local_model
            ? `Auto · ${status.local_model}`
            : "Auto · Claude";
  const hotkey = status?.kill_hotkey || settings?.hotkey || "Ctrl+Alt+Shift+Esc";

  return (
    <div className="shrink-0 border-t border-line p-2">
      <DropdownMenu.Root>
        <DropdownMenu.Trigger asChild>
          <button type="button" className="flex w-full items-center gap-2.5 rounded-lg p-2 text-left hover:bg-hover data-[state=open]:bg-hover">
            <Avatar name={name} size={30} />
            <span className="min-w-0 flex-1">
              <span className="block truncate text-[14px] leading-5">{name}</span>
              <span className="flex items-center gap-1.5 truncate text-[12px] leading-4 text-subtle">
                <span
                  className={cn("inline-block h-1.5 w-1.5 shrink-0 rounded-full", !status ? "bg-subtle" : status.ready ? "bg-success" : "bg-danger")}
                  aria-hidden="true"
                />
                {statusLine}
              </span>
            </span>
          </button>
        </DropdownMenu.Trigger>
        <DropdownMenu.Portal>
          <DropdownMenu.Content side="top" align="start" sideOffset={8} className="menu pop-in w-[calc(var(--sidebar-w)-16px)]">
            <div className="flex items-center gap-2.5 px-2.5 py-2">
              <Avatar name={name} size={26} />
              <div className="min-w-0">
                <div className="truncate text-[14px]">{name}</div>
                <div className="truncate text-[12px] text-subtle">{status?.detail || "Relay"}</div>
              </div>
            </div>
            <DropdownMenu.Separator className="menu-sep" />
            <DropdownMenu.Item className="menu-item" onSelect={() => openSettings("personalization")}>
              <SlidersHorizontal size={16} /> Personalization
            </DropdownMenu.Item>
            <DropdownMenu.Item className="menu-item" onSelect={() => openSettings("general")}>
              <SettingsIcon size={16} /> Settings <span className="kbd">{MOD}+,</span>
            </DropdownMenu.Item>
            <DropdownMenu.Item className="menu-item" onSelect={() => openSettings("keyboard")}>
              <Keyboard size={16} /> Keyboard shortcuts
            </DropdownMenu.Item>
            <DropdownMenu.Separator className="menu-sep" />
            <DropdownMenu.Item className="menu-item" onSelect={() => openSettings("about")}>
              <CircleQuestionMark size={16} /> Help and about
            </DropdownMenu.Item>
            <Tip label="Stops any running task immediately" side="right">
              <div className="menu-item text-muted" role="note">
                <OctagonX size={16} className="text-danger" />
                <span className="text-[13px] leading-4">
                  Emergency stop
                  <span className="block text-[12px] text-subtle">{hotkey}</span>
                </span>
              </div>
            </Tip>
          </DropdownMenu.Content>
        </DropdownMenu.Portal>
      </DropdownMenu.Root>
    </div>
  );
}
