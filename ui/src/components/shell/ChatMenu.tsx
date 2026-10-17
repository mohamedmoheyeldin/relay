// Items for a chat's "..." menu and right-click menu (same content in both).
import { Archive, ArchiveRestore, Check, FolderInput, FolderMinus, Pencil, Pin, PinOff, Trash2 } from "lucide-react";
import type { ConvSummary } from "@/lib/types";
import { useApp } from "@/store/app";
import { useDeleteConversation, useMoveConversation, useProjects, useUpdateConversation } from "@/hooks/queries";
import { FolderIcon } from "@/components/common";
import {
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
} from "@/components/ui/dropdown-menu";
import {
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from "@/components/ui/context-menu";

const KINDS = {
  dropdown: {
    Item: DropdownMenuItem,
    Label: DropdownMenuLabel,
    Separator: DropdownMenuSeparator,
    Sub: DropdownMenuSub,
    SubTrigger: DropdownMenuSubTrigger,
    SubContent: DropdownMenuSubContent,
  },
  context: {
    Item: ContextMenuItem,
    Label: ContextMenuLabel,
    Separator: ContextMenuSeparator,
    Sub: ContextMenuSub,
    SubTrigger: ContextMenuSubTrigger,
    SubContent: ContextMenuSubContent,
  },
} as const;

/** Chat actions with optimistic updates; also used by keyboard shortcuts on sidebar rows. */
export function useChatActions() {
  const update = useUpdateConversation();
  const move = useMoveConversation();
  const del = useDeleteConversation();
  return {
    rename: (id: string, title: string) => {
      const t = title.trim();
      if (t) update.mutate({ id, patch: { title: t } });
    },
    setPinned: (id: string, pinned: boolean) => update.mutate({ id, patch: { pinned } }),
    setArchived: (id: string, archived: boolean) => update.mutate({ id, patch: { archived } }),
    move: (id: string, projectId: string | null) => move.mutate({ id, projectId }),
    askDelete: (conv: ConvSummary) => useApp.getState().askDeleteConv(conv, () => del.mutate(conv.id)),
  };
}

export function ChatMenuItems({ conv, onRename, kind }: { conv: ConvSummary; onRename: () => void; kind: "dropdown" | "context" }) {
  const M = KINDS[kind];
  const projects = useProjects();
  const a = useChatActions();
  return (
    <>
      <M.Item onSelect={() => setTimeout(onRename, 0)}>
        <Pencil aria-hidden="true" /> Rename
        <span className="ml-auto pl-6 text-caption text-fg-muted">F2</span>
      </M.Item>
      <M.Item onSelect={() => a.setPinned(conv.id, !conv.pinned)}>
        {conv.pinned ? <PinOff aria-hidden="true" /> : <Pin aria-hidden="true" />} {conv.pinned ? "Unpin" : "Pin"}
      </M.Item>
      <M.Sub>
        <M.SubTrigger>
          <FolderInput aria-hidden="true" /> Move to project
        </M.SubTrigger>
        <M.SubContent>
          {(projects.data ?? []).length === 0 && <M.Label>No projects yet</M.Label>}
          {(projects.data ?? []).map((p) => (
            <M.Item key={p.id} disabled={conv.project_id === p.id} onSelect={() => a.move(conv.id, p.id)}>
              <FolderIcon color={p.color} size={15} />
              <span className="truncate">{p.name}</span>
              {conv.project_id === p.id && <Check className="ml-auto text-fg" aria-hidden="true" />}
            </M.Item>
          ))}
          {conv.project_id && (
            <>
              <M.Separator />
              <M.Item onSelect={() => a.move(conv.id, null)}>
                <FolderMinus aria-hidden="true" /> Remove from project
              </M.Item>
            </>
          )}
        </M.SubContent>
      </M.Sub>
      <M.Item onSelect={() => a.setArchived(conv.id, !conv.archived)}>
        {conv.archived ? <ArchiveRestore aria-hidden="true" /> : <Archive aria-hidden="true" />} {conv.archived ? "Restore from archive" : "Archive"}
      </M.Item>
      <M.Separator />
      <M.Item variant="destructive" onSelect={() => setTimeout(() => a.askDelete(conv), 0)}>
        <Trash2 aria-hidden="true" /> Delete…
        <span className="ml-auto pl-6 text-caption opacity-70">Del</span>
      </M.Item>
    </>
  );
}
