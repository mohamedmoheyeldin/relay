// Sidebar: 280px panel that collapses to a 56px icon rail (slide by transform; state remembered),
// or an overlay drawer below 768px. Sections: Pinned, Projects, Recents grouped by date.
import { memo, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useLocation } from "react-router";
import {
  CircleHelp,
  Ellipsis,
  History,
  Keyboard,
  OctagonX,
  PanelLeftClose,
  PanelLeftOpen,
  Pencil,
  Pin,
  PinOff,
  Plus,
  Search,
  Settings as SettingsIcon,
  SlidersHorizontal,
  SquarePen,
  Trash2,
} from "lucide-react";
import { useApp } from "@/store/app";
import { groupByDate, sidebarSections } from "@/lib/grouping";
import type { ConvSummary, Project } from "@/lib/types";
import { cn } from "@/lib/utils";
import { go, paths } from "@/lib/nav";
import { shortcutKeys } from "@/lib/platform";
import { useConversations, useDeleteProject, useHybrid, useProjects, useSettings, useStatus, useBusy } from "@/hooks/queries";
import { useNarrow } from "@/hooks/dom";
import { useRovingList } from "@/hooks/roving";
import { Avatar, FolderIcon, RelayMark, StatusDot } from "@/components/common";
import { IconButton } from "@/components/ui/icon-button";
import { Skeleton } from "@/components/ui/skeleton";
import { useShortcutLabel } from "@/components/ui/kbd";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuSeparator, ContextMenuTrigger } from "@/components/ui/context-menu";
import { ChatMenuItems, useChatActions } from "./ChatMenu";

export function Sidebar() {
  const narrow = useNarrow();
  const collapsed = useApp((s) => s.sidebarCollapsed);
  const drawerOpen = useApp((s) => s.drawerOpen);
  const setDrawerOpen = useApp((s) => s.setDrawerOpen);
  const location = useLocation();

  // Navigating closes the drawer on narrow windows.
  useEffect(() => {
    setDrawerOpen(false);
  }, [location.pathname, setDrawerOpen]);

  // Drawer focus: move focus in when it opens, Esc closes it, focus returns to the opener.
  const drawerRef = useRef<HTMLElement>(null);
  useEffect(() => {
    if (!narrow || !drawerOpen) return;
    const opener = document.activeElement as HTMLElement | null;
    const t = setTimeout(() => drawerRef.current?.querySelector<HTMLElement>("button, [href], input")?.focus(), 50);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !document.querySelector("[role=menu],[role=dialog],[role=alertdialog]")) {
        e.preventDefault();
        setDrawerOpen(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      clearTimeout(t);
      window.removeEventListener("keydown", onKey);
      if (opener && document.contains(opener)) opener.focus();
    };
  }, [narrow, drawerOpen, setDrawerOpen]);

  if (narrow) {
    return (
      <>
        <div
          aria-hidden="true"
          onClick={() => setDrawerOpen(false)}
          className={cn(
            "fixed inset-0 z-(--relay-z-overlay) bg-scrim transition-opacity duration-(--relay-dur-panel)",
            drawerOpen ? "opacity-100" : "pointer-events-none opacity-0",
          )}
        />
        <aside
          ref={drawerRef}
          aria-label="Sidebar"
          aria-hidden={!drawerOpen}
          inert={!drawerOpen}
          className={cn(
            "fixed inset-y-0 left-0 z-(--relay-z-drawer) w-[min(var(--relay-sidebar-w),86vw)] border-r border-line bg-sidebar shadow-dialog transition-transform duration-(--relay-dur-panel) ease-relay will-change-transform",
            "[:root[data-material=mica]_&]:bg-bg [:root[data-material=vibrancy]_&]:bg-bg",
            drawerOpen ? "translate-x-0" : "-translate-x-full",
          )}
        >
          <SidebarPanel />
        </aside>
      </>
    );
  }

  return (
    <aside
      aria-label="Sidebar"
      className="relative z-(--relay-z-sidebar) h-full shrink-0"
      style={{ width: collapsed ? "var(--relay-sidebar-w-collapsed)" : "var(--relay-sidebar-w)" }}
      data-collapsed={collapsed}
    >
      {/* The 280px panel slides left by transform; its right 56px stays visible as the rail. */}
      <div
        className="absolute inset-y-0 left-0 w-(--relay-sidebar-w) border-r border-line bg-sidebar transition-transform duration-(--relay-dur-panel) ease-relay will-change-transform"
        style={{ transform: collapsed ? "translateX(calc(var(--relay-sidebar-w-collapsed) - var(--relay-sidebar-w)))" : "none" }}
      >
        <div className={cn("h-full transition-opacity duration-(--relay-dur)", collapsed ? "pointer-events-none opacity-0" : "opacity-100")} inert={collapsed}>
          <SidebarPanel />
        </div>
        <div
          className={cn(
            "absolute inset-y-0 right-0 w-(--relay-sidebar-w-collapsed) transition-opacity duration-(--relay-dur)",
            collapsed ? "opacity-100" : "pointer-events-none opacity-0",
          )}
          inert={!collapsed}
        >
          <Rail />
        </div>
      </div>
    </aside>
  );
}

function TopBar({ children }: { children: ReactNode }) {
  return (
    <div data-tauri-drag-region className="flex h-12 shrink-0 items-center gap-1 pr-2 pl-[calc(var(--relay-titlebar-inset)+12px)]">
      {children}
    </div>
  );
}

function Rail() {
  const toggle = useApp((s) => s.toggleSidebar);
  const setSearch = useApp((s) => s.setSearchOpen);
  const settings = useSettings();
  const newLabel = useShortcutLabel(shortcutKeys("newChat"));
  const searchLabel = useShortcutLabel(shortcutKeys("search"));
  const tasksLabel = useShortcutLabel(shortcutKeys("tasks"));
  const sideLabel = useShortcutLabel(shortcutKeys("toggleSidebar"));
  return (
    <nav aria-label="Sidebar (collapsed)" className="flex h-full flex-col items-center gap-1 pt-(--relay-titlebar-top) pb-3">
      <div className="flex h-12 items-center" data-tauri-drag-region>
        <IconButton label="Expand sidebar" shortcut={sideLabel} tipSide="right" onClick={() => toggle(false)}>
          <PanelLeftOpen aria-hidden="true" />
        </IconButton>
      </div>
      <IconButton label="New chat" shortcut={newLabel} tipSide="right" onClick={() => go(paths.home())}>
        <SquarePen aria-hidden="true" />
      </IconButton>
      <IconButton label="Search" shortcut={searchLabel} tipSide="right" onClick={() => setSearch(true)}>
        <Search aria-hidden="true" />
      </IconButton>
      <IconButton label="Task history" shortcut={tasksLabel} tipSide="right" onClick={() => go(paths.tasks())}>
        <History aria-hidden="true" />
      </IconButton>
      <span className="flex-1" />
      <IconButton label="Settings" tipSide="right" onClick={() => go(paths.settings())}>
        <SettingsIcon aria-hidden="true" />
      </IconButton>
      <Avatar name={settings.data?.user_name || "You"} size={28} className="mt-1" />
    </nav>
  );
}

function NavRow({ icon, label, shortcut, onClick, active, testId }: { icon: ReactNode; label: string; shortcut?: string; onClick: () => void; active?: boolean; testId?: string }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={active ? "page" : undefined}
      data-testid={testId}
      className={cn(
        "group flex h-8 w-full items-center gap-2.5 rounded-md px-2 text-left text-ui text-fg transition-colors duration-(--relay-dur-fast) hover:bg-hover",
        active && "bg-active hover:bg-active",
      )}
    >
      <span className="flex w-5 justify-center text-fg-2 [&_svg]:size-4">{icon}</span>
      <span className="flex-1 truncate">{label}</span>
      {shortcut && <span className="text-micro text-fg-muted opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100">{shortcut}</span>}
    </button>
  );
}

function SectionTitle({ children, action }: { children: ReactNode; action?: ReactNode }) {
  return (
    <div className="flex h-7 items-center justify-between pr-1 pl-2">
      <h2 className="text-micro font-medium tracking-[0.02em] text-fg-muted">{children}</h2>
      {action}
    </div>
  );
}

function SidebarPanel() {
  const toggle = useApp((s) => s.toggleSidebar);
  const setSearch = useApp((s) => s.setSearchOpen);
  const narrow = useNarrow();
  const location = useLocation();
  const convs = useConversations();
  const projects = useProjects();
  const pinnedProjects = useApp((s) => s.pinnedProjects);
  const listRef = useRef<HTMLDivElement>(null);
  const onListKeyDown = useRovingList(listRef);

  const { pinned, recents } = useMemo(() => sidebarSections(convs.data ?? []), [convs.data]);
  const groups = useMemo(() => groupByDate(recents, Date.now()), [recents]);
  const pinnedP = useMemo(() => (projects.data ?? []).filter((p) => pinnedProjects.includes(p.id)), [projects.data, pinnedProjects]);
  const otherP = useMemo(() => (projects.data ?? []).filter((p) => !pinnedProjects.includes(p.id)), [projects.data, pinnedProjects]);

  const newLabel = useShortcutLabel(shortcutKeys("newChat"));
  const searchLabel = useShortcutLabel(shortcutKeys("search"));
  const tasksLabel = useShortcutLabel(shortcutKeys("tasks"));
  const sideLabel = useShortcutLabel(shortcutKeys("toggleSidebar"));

  return (
    <nav className="flex h-full flex-col" aria-label="Chats and projects">
      <TopBar>
        <button type="button" onClick={() => go(paths.home())} className="no-drag flex items-center gap-2 rounded-md px-1.5 py-1 hover:bg-hover" aria-label="Relay home">
          <RelayMark size={20} />
          <span className="font-serif text-[19px] leading-6 tracking-[-0.01em]">Relay</span>
        </button>
        <span className="flex-1" data-tauri-drag-region />
        <IconButton label={narrow ? "Close sidebar" : "Collapse sidebar"} shortcut={sideLabel} onClick={() => toggle(narrow)} className="no-drag">
          <PanelLeftClose aria-hidden="true" />
        </IconButton>
      </TopBar>

      <div className="space-y-px px-2 pb-2">
        <NavRow icon={<SquarePen />} label="New chat" shortcut={newLabel} onClick={() => go(paths.home())} active={location.pathname === "/"} testId="nav-new-chat" />
        <NavRow icon={<Search />} label="Search" shortcut={searchLabel} onClick={() => setSearch(true)} testId="nav-search" />
        <NavRow icon={<History />} label="Task history" shortcut={tasksLabel} onClick={() => go(paths.tasks())} active={location.pathname.startsWith("/tasks")} testId="nav-tasks" />
      </div>

      <div ref={listRef} onKeyDown={onListKeyDown} className="min-h-0 flex-1 overflow-y-auto px-2 pb-3 scroll-thin">
        {(pinned.length > 0 || pinnedP.length > 0) && (
          <section className="mb-3" aria-label="Pinned">
            <SectionTitle>Pinned</SectionTitle>
            {pinnedP.map((p) => (
              <ProjectRow key={p.id} project={p} pinned />
            ))}
            {pinned.map((c) => (
              <ChatRow key={c.id} conv={c} />
            ))}
          </section>
        )}

        <section className="mb-3" aria-label="Projects">
          <SectionTitle
            action={
              <IconButton label="New project" size="icon-sm" onClick={() => useApp.getState().setProjectDialog({ mode: "create" })}>
                <Plus aria-hidden="true" />
              </IconButton>
            }
          >
            Projects
          </SectionTitle>
          {projects.isPending ? (
            <SkeletonRows n={2} />
          ) : (projects.data ?? []).length === 0 ? (
            <button
              type="button"
              onClick={() => useApp.getState().setProjectDialog({ mode: "create" })}
              className="flex h-8 w-full items-center gap-2.5 rounded-md px-2 text-ui text-fg-muted hover:bg-hover hover:text-fg"
            >
              <Plus className="size-4" aria-hidden="true" /> New project
            </button>
          ) : (
            otherP.map((p) => <ProjectRow key={p.id} project={p} />)
          )}
        </section>

        <section aria-label="Recent chats">
          {convs.isPending ? (
            <>
              <SectionTitle>Recents</SectionTitle>
              <SkeletonRows n={6} />
            </>
          ) : convs.isError ? (
            <>
              <SectionTitle>Recents</SectionTitle>
              <p className="px-2 py-1 text-caption text-danger">Couldn't load chats.</p>
            </>
          ) : groups.length === 0 ? (
            <>
              <SectionTitle>Recents</SectionTitle>
              <p className="px-2 py-1 text-caption text-fg-muted">Your chats will show up here.</p>
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
    <div className="space-y-2.5 px-2 py-1.5" aria-hidden="true">
      {Array.from({ length: n }, (_, i) => (
        <Skeleton key={i} className="h-3.5" style={{ width: `${55 + ((i * 37) % 40)}%` }} />
      ))}
    </div>
  );
}

const ChatRow = memo(function ChatRow({ conv }: { conv: ConvSummary }) {
  const location = useLocation();
  const active = location.pathname === paths.chat(conv.id);
  const running = useApp((s) => s.activeRun?.conv_id === conv.id || s.sending === conv.id);
  const actions = useChatActions();
  const [editing, setEditing] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const title = conv.title || "New chat";

  if (editing) {
    return (
      <RenameInput
        initial={title}
        onDone={(v) => {
          setEditing(false);
          if (v !== null && v !== title) actions.rename(conv.id, v);
        }}
      />
    );
  }

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div
          className={cn(
            "group relative flex h-8 items-center rounded-md transition-colors duration-(--relay-dur-fast) hover:bg-hover",
            (active || menuOpen) && "bg-active hover:bg-active",
          )}
          data-testid="chat-row"
        >
          <button
            type="button"
            data-roving
            data-roving-active={active ? "true" : undefined}
            onClick={() => go(paths.chat(conv.id))}
            onDoubleClick={() => setEditing(true)}
            onKeyDown={(e) => {
              if (e.key === "F2") {
                e.preventDefault();
                setEditing(true);
              } else if (e.key === "Delete") {
                e.preventDefault();
                actions.askDelete(conv);
              }
            }}
            aria-current={active ? "page" : undefined}
            className="flex h-full min-w-0 flex-1 items-center gap-2 rounded-md pr-8 pl-2 text-left text-ui text-fg"
          >
            <span className="truncate">{title}</span>
            {running && (
              <>
                <StatusDot tone="busy" className="ml-auto" />
                <span className="sr-only">(working)</span>
              </>
            )}
          </button>
          <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                tabIndex={-1}
                aria-label={`Options for ${title}`}
                className={cn(
                  "absolute right-1 inline-flex size-6 items-center justify-center rounded-sm text-fg-muted hover:bg-hover hover:text-fg",
                  menuOpen || active ? "opacity-100" : "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100",
                )}
              >
                <Ellipsis className="size-4" aria-hidden="true" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-56">
              <ChatMenuItems conv={conv} kind="dropdown" onRename={() => setEditing(true)} />
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent className="w-56">
        <ChatMenuItems conv={conv} kind="context" onRename={() => setEditing(true)} />
      </ContextMenuContent>
    </ContextMenu>
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
    <div className="flex h-8 items-center rounded-md bg-active px-1">
      <input
        ref={ref}
        defaultValue={initial}
        aria-label="Chat title"
        maxLength={120}
        className="h-6 w-full rounded-sm border border-brand bg-surface px-1.5 text-ui outline-none"
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
  const location = useLocation();
  const active = location.pathname === paths.project(project.id);
  const del = useDeleteProject();
  const [menuOpen, setMenuOpen] = useState(false);
  const { togglePinProject, setProjectDialog, setConfirmDialog } = useApp.getState();
  const askDelete = () =>
    setConfirmDialog({
      title: "Delete project?",
      body: `"${project.name}" will be deleted. Its chats are kept and move back to your recents.`,
      action: "Delete project",
      destructive: true,
      onConfirm: () => del.mutate(project.id),
    });
  const items = (kind: "dropdown" | "context") => {
    const Item = kind === "dropdown" ? DropdownMenuItem : ContextMenuItem;
    const Sep = kind === "dropdown" ? DropdownMenuSeparator : ContextMenuSeparator;
    return (
      <>
        <Item onSelect={() => setTimeout(() => setProjectDialog({ mode: "edit", project }), 0)}>
          <Pencil aria-hidden="true" /> Edit project
        </Item>
        <Item onSelect={() => togglePinProject(project.id)}>
          {pinned ? <PinOff aria-hidden="true" /> : <Pin aria-hidden="true" />} {pinned ? "Unpin" : "Pin"}
        </Item>
        <Sep />
        <Item variant="destructive" onSelect={() => setTimeout(askDelete, 0)}>
          <Trash2 aria-hidden="true" /> Delete project…
        </Item>
      </>
    );
  };
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div className={cn("group relative flex h-8 items-center rounded-md hover:bg-hover", (active || menuOpen) && "bg-active hover:bg-active")}>
          <button
            type="button"
            data-roving
            data-roving-active={active ? "true" : undefined}
            onClick={() => go(paths.project(project.id))}
            aria-current={active ? "page" : undefined}
            className="flex h-full min-w-0 flex-1 items-center gap-2 rounded-md pr-8 pl-2 text-left text-ui"
          >
            <FolderIcon color={project.color} size={16} />
            <span className="truncate">{project.name}</span>
          </button>
          <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                tabIndex={-1}
                aria-label={`Options for ${project.name}`}
                className={cn(
                  "absolute right-1 inline-flex size-6 items-center justify-center rounded-sm text-fg-muted hover:bg-hover hover:text-fg",
                  menuOpen ? "opacity-100" : "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100",
                )}
              >
                <Ellipsis className="size-4" aria-hidden="true" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start">{items("dropdown")}</DropdownMenuContent>
          </DropdownMenu>
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent>{items("context")}</ContextMenuContent>
    </ContextMenu>
  );
}

function ProfileFooter() {
  const settings = useSettings();
  const status = useStatus();
  const hybrid = useHybrid();
  const busy = useBusy();
  const name = settings.data?.user_name?.trim() || "You";
  const st = status.data;
  const mode = hybrid.data?.privacy_mode;
  const modeLabel = mode === "local_only" ? "Local only" : mode === "cloud_preferred" ? "Cloud preferred" : mode === "hybrid_auto" ? "Hybrid" : null;
  const statusLine = !st ? (status.isError ? "Status unavailable" : "Connecting…") : busy ? "Working on a task…" : [modeLabel, st.ready ? st.provider : st.detail].filter(Boolean).join(" · ");
  const tone = !st ? (status.isError ? "warn" : "idle") : busy ? "busy" : st.ready ? "ok" : "warn";
  const hotkey = st?.kill_hotkey || settings.data?.hotkey || "Ctrl+Alt+Shift+Esc";
  const settingsLabel = useShortcutLabel(shortcutKeys("settings"));
  const keysLabel = useShortcutLabel(shortcutKeys("shortcuts"));

  return (
    <div className="shrink-0 border-t border-line p-2">
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            className="flex w-full items-center gap-2.5 rounded-md p-1.5 text-left transition-colors hover:bg-hover data-[state=open]:bg-hover"
            data-testid="profile-button"
          >
            <Avatar name={name} size={30} />
            <span className="sr-only">Account menu: </span>
            <span className="min-w-0 flex-1">
              <span className="block truncate text-ui leading-5 text-fg">{name}</span>
              <span className="flex items-center gap-1.5 truncate text-caption text-fg-muted">
                <StatusDot tone={tone} />
                <span className="truncate">{statusLine}</span>
              </span>
            </span>
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent side="top" align="start" className="w-[calc(var(--relay-sidebar-w)-16px)]">
          <DropdownMenuItem onSelect={() => go(paths.settings("general"))}>
            <SettingsIcon aria-hidden="true" /> Settings
            <DropdownMenuShortcut>{settingsLabel}</DropdownMenuShortcut>
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => go(paths.settings("personalization"))}>
            <SlidersHorizontal aria-hidden="true" /> Personalization
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => go(paths.settings("keyboard"))}>
            <Keyboard aria-hidden="true" /> Keyboard shortcuts
            <DropdownMenuShortcut>{keysLabel}</DropdownMenuShortcut>
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => go(paths.settings("about"))}>
            <CircleHelp aria-hidden="true" /> Help and about
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <div className="flex items-start gap-2.5 px-2 py-1.5 text-caption text-fg-muted" role="note">
            <OctagonX className="mt-0.5 size-4 shrink-0 text-danger" aria-hidden="true" />
            <span>
              Emergency stop, anywhere: <span className="font-medium text-fg-2">{hotkey}</span>
            </span>
          </div>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
