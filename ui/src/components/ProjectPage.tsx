import { useMemo } from "react";
import { DropdownMenu } from "radix-ui";
import { Ellipsis, MessageCircle, Pencil, Pin, PinOff, Trash } from "lucide-react";
import { useApp } from "../store/app";
import { folderColor } from "../lib/theme";
import { Composer } from "./Composer";
import { Button, FolderIcon, IconButton } from "./ui";
import { ChatMenuItems } from "./menus";

const fmtDate = (t: number) => new Date(t).toLocaleDateString(undefined, { month: "short", day: "numeric" });

export function ProjectPage({ projectId }: { projectId: string }) {
  const project = useApp((s) => s.projects.find((p) => p.id === projectId));
  const loaded = useApp((s) => s.projectsLoaded);
  const convs = useApp((s) => s.convs);
  const pinned = useApp((s) => s.pinnedProjects.includes(projectId));
  const chats = useMemo(() => convs.filter((c) => c.project_id === projectId && !c.archived), [convs, projectId]);
  const { setProjectDialog, setConfirmDialog, deleteProject, togglePinProject, openChat, setRenameDialog } = useApp.getState();

  if (!project) {
    return (
      <div className="flex flex-1 items-center justify-center text-[14px] text-subtle">
        {loaded ? "This project no longer exists." : <div className="skeleton h-6 w-48" />}
      </div>
    );
  }

  const edit = () => setProjectDialog({ mode: "edit", project });

  return (
    <div className="scroll-thin min-h-0 flex-1 overflow-y-auto px-4">
      <div className="mx-auto w-full max-w-[768px] pt-[6vh] pb-12">
        <div className="mb-6 flex items-center gap-3">
          <span
            className="inline-flex h-11 w-11 shrink-0 items-center justify-center rounded-xl"
            style={{ background: `color-mix(in srgb, ${folderColor(project.color)} 16%, transparent)` }}
          >
            <FolderIcon color={project.color} size={22} />
          </span>
          <h1 className="min-w-0 flex-1 truncate text-[26px] leading-9 font-normal">{project.name}</h1>
          <DropdownMenu.Root>
            <DropdownMenu.Trigger asChild>
              <IconButton label="Project options">
                <Ellipsis size={19} />
              </IconButton>
            </DropdownMenu.Trigger>
            <DropdownMenu.Portal>
              <DropdownMenu.Content className="menu pop-in" align="end" sideOffset={6}>
                <DropdownMenu.Item className="menu-item" onSelect={() => setTimeout(edit, 0)}>
                  <Pencil size={16} /> Edit project
                </DropdownMenu.Item>
                <DropdownMenu.Item className="menu-item" onSelect={() => togglePinProject(project.id)}>
                  {pinned ? <PinOff size={16} /> : <Pin size={16} />} {pinned ? "Unpin" : "Pin to sidebar"}
                </DropdownMenu.Item>
                <DropdownMenu.Separator className="menu-sep" />
                <DropdownMenu.Item
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
                </DropdownMenu.Item>
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
        </div>

        <Composer projectId={project.id} placeholder={`New chat in ${project.name}`} autoFocus />

        <button
          type="button"
          onClick={edit}
          className="mt-4 flex w-full items-start gap-3 rounded-2xl border border-line px-4 py-3 text-left transition-colors hover:bg-hover"
        >
          <span className="min-w-0 flex-1">
            <span className="block text-[14px] font-medium">Instructions</span>
            <span className="mt-0.5 line-clamp-2 block text-[13.5px] text-subtle">
              {project.instructions.trim() || "Tell Relay how to work in this project, e.g. which folders to use or what to avoid."}
            </span>
          </span>
          <Pencil size={15} className="mt-1 shrink-0 text-subtle" aria-hidden="true" />
        </button>

        <h2 className="mt-8 mb-2 px-1 text-[13px] font-medium text-subtle">Chats in this project</h2>
        {chats.length === 0 ? (
          <p className="rounded-2xl border border-dashed border-line px-4 py-6 text-center text-[14px] text-subtle">
            No chats yet. Start one above, or move a chat here from its menu.
          </p>
        ) : (
          <ul className="divide-y divide-[var(--border)] border-y border-line">
            {chats.map((c) => (
              <li key={c.id} className="group flex items-center">
                <button
                  type="button"
                  onClick={() => void openChat(c.id)}
                  className="flex min-w-0 flex-1 items-center gap-3 px-2 py-3 text-left hover:bg-hover"
                >
                  <MessageCircle size={16} className="shrink-0 text-subtle" aria-hidden="true" />
                  <span className="min-w-0 flex-1 truncate text-[14.5px]">{c.title || "New chat"}</span>
                  <span className="shrink-0 text-[12.5px] text-subtle">{fmtDate(c.updated_at)}</span>
                </button>
                <DropdownMenu.Root>
                  <DropdownMenu.Trigger asChild>
                    <IconButton label={`Options for ${c.title || "chat"}`} className="h-8 w-8 opacity-0 group-hover:opacity-100 focus-visible:opacity-100 data-[state=open]:opacity-100">
                      <Ellipsis size={17} />
                    </IconButton>
                  </DropdownMenu.Trigger>
                  <DropdownMenu.Portal>
                    <DropdownMenu.Content className="menu pop-in" align="end" sideOffset={4}>
                      <ChatMenuItems conv={c} onRename={() => setRenameDialog({ id: c.id, title: c.title || "New chat" })} />
                    </DropdownMenu.Content>
                  </DropdownMenu.Portal>
                </DropdownMenu.Root>
              </li>
            ))}
          </ul>
        )}
        <div className="mt-6 flex justify-center">
          <Button variant="ghost" size="sm" onClick={edit}>
            <Pencil size={14} /> Edit project
          </Button>
        </div>
      </div>
    </div>
  );
}
