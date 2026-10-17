import { useMemo, useRef, useState } from "react";
import { Ellipsis, MessageSquare, Pencil, Pin, PinOff, Trash2 } from "lucide-react";
import { useApp } from "@/store/app";
import { folderColor } from "@/lib/theme";
import { useConversations, useDeleteProject, useProjects } from "@/hooks/queries";
import { useRovingList } from "@/hooks/roving";
import { go, paths } from "@/lib/nav";
import { Composer } from "@/components/chat/Composer";
import { ContextControls } from "@/components/chat/ContextControls";
import { EmptyState, FolderIcon } from "@/components/common";
import { IconButton } from "@/components/ui/icon-button";
import { Skeleton } from "@/components/ui/skeleton";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { ChatMenuItems, useChatActions } from "@/components/shell/ChatMenu";
import { RenameDialog } from "@/components/shell/Dialogs";
import type { ConvSummary } from "@/lib/types";

const fmtDate = (t: number) => new Date(t).toLocaleDateString(undefined, { month: "short", day: "numeric" });

export function ProjectPage({ projectId }: { projectId: string }) {
  const projects = useProjects();
  const convs = useConversations();
  const project = projects.data?.find((p) => p.id === projectId);
  const pinned = useApp((s) => s.pinnedProjects.includes(projectId));
  const chats = useMemo(() => (convs.data ?? []).filter((c) => c.project_id === projectId && !c.archived), [convs.data, projectId]);
  const del = useDeleteProject();
  const listRef = useRef<HTMLUListElement>(null);
  const onKeyDown = useRovingList(listRef);
  const { setProjectDialog, setConfirmDialog, togglePinProject } = useApp.getState();

  if (!project) {
    return (
      <div className="mx-auto w-full max-w-(--relay-content-max) px-6 pt-[8vh]">
        {projects.isPending ? (
          <>
            <Skeleton className="mb-6 h-10 w-64" />
            <Skeleton className="h-28 w-full rounded-composer" />
          </>
        ) : (
          <EmptyState title="This project no longer exists." />
        )}
      </div>
    );
  }

  const edit = () => setProjectDialog({ mode: "edit", project });

  return (
    <div className="min-h-0 flex-1 overflow-y-auto px-4 scroll-thin">
      <div className="mx-auto w-full max-w-(--relay-content-max) pt-[6vh] pb-12">
        <div className="mb-6 flex items-center gap-3">
          <span
            className="inline-flex size-11 shrink-0 items-center justify-center rounded-lg"
            style={{ background: `color-mix(in srgb, ${folderColor(project.color)} 14%, transparent)` }}
          >
            <FolderIcon color={project.color} size={22} />
          </span>
          <h1 className="min-w-0 flex-1 truncate font-display text-title">{project.name}</h1>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <IconButton label="Project options" noTip>
                <Ellipsis aria-hidden="true" />
              </IconButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onSelect={() => setTimeout(edit, 0)}>
                <Pencil aria-hidden="true" /> Edit project
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => togglePinProject(project.id)}>
                {pinned ? <PinOff aria-hidden="true" /> : <Pin aria-hidden="true" />} {pinned ? "Unpin from sidebar" : "Pin to sidebar"}
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                variant="destructive"
                onSelect={() =>
                  setTimeout(
                    () =>
                      setConfirmDialog({
                        title: "Delete project?",
                        body: `"${project.name}" will be deleted. Its chats are kept and move back to your recents.`,
                        action: "Delete project",
                        destructive: true,
                        onConfirm: () => del.mutate(project.id),
                      }),
                    0,
                  )
                }
              >
                <Trash2 aria-hidden="true" /> Delete project…
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>

        <Composer projectId={project.id} placeholder={`New chat in ${project.name}`} autoFocus />
        <ContextControls size="sm" className="mt-2" />

        <button type="button" onClick={edit} className="mt-6 flex w-full items-start gap-3 rounded-lg border border-line bg-surface px-4 py-3 text-left transition-colors hover:bg-hover">
          <span className="min-w-0 flex-1">
            <span className="block text-ui font-medium">Instructions</span>
            <span className="mt-0.5 line-clamp-2 block text-caption leading-5 text-fg-muted">
              {project.instructions.trim() || "Tell Relay how to work in this project, for example which folders to use or what to avoid."}
            </span>
          </span>
          <Pencil className="mt-1 size-4 shrink-0 text-fg-muted" aria-hidden="true" />
        </button>

        <h2 className="mt-10 mb-2 px-1 text-micro font-medium tracking-[0.02em] text-fg-muted">Chats in this project</h2>
        {convs.isPending ? (
          <div className="space-y-2">
            <Skeleton className="h-11" />
            <Skeleton className="h-11" />
          </div>
        ) : chats.length === 0 ? (
          <EmptyState icon={<MessageSquare />} title="No chats yet">
            Start one above, or move a chat here from its menu.
          </EmptyState>
        ) : (
          <ul ref={listRef} onKeyDown={onKeyDown} className="divide-y divide-line border-y border-line">
            {chats.map((c) => (
              <ProjectChatRow key={c.id} conv={c} />
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function ProjectChatRow({ conv }: { conv: ConvSummary }) {
  const [open, setOpen] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const actions = useChatActions();
  return (
    <li className="group flex items-center">
      <button type="button" data-roving onClick={() => go(paths.chat(conv.id))} className="flex min-w-0 flex-1 items-center gap-3 px-2 py-3 text-left hover:bg-hover">
        <MessageSquare className="size-4 shrink-0 text-fg-muted" aria-hidden="true" />
        <span className="min-w-0 flex-1 truncate text-ui">{conv.title || "New chat"}</span>
        <span className="shrink-0 text-caption text-fg-muted">{fmtDate(conv.updated_at)}</span>
      </button>
      <DropdownMenu open={open} onOpenChange={setOpen}>
        <DropdownMenuTrigger asChild>
          <IconButton label={`Options for ${conv.title || "chat"}`} noTip size="icon-sm" tabIndex={-1} className="opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 data-[state=open]:opacity-100">
            <Ellipsis aria-hidden="true" />
          </IconButton>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-56">
          <ChatMenuItems conv={conv} kind="dropdown" onRename={() => setRenaming(true)} />
        </DropdownMenuContent>
      </DropdownMenu>
      <RenameDialog open={renaming} initial={conv.title || "New chat"} onClose={() => setRenaming(false)} onRename={(t) => actions.rename(conv.id, t)} />
    </li>
  );
}
