import { DropdownMenu, ContextMenu } from "radix-ui";
import { Archive, ArchiveRestore, Check, FolderInput, FolderMinus, Pencil, Pin, PinOff, Trash, ChevronRight } from "lucide-react";
import { useApp } from "../store/app";
import type { ConvSummary } from "../lib/types";
import { FolderIcon } from "./ui";

type M = typeof DropdownMenu;
const asM = (x: typeof ContextMenu) => x as unknown as M;

/** Items for a chat's "..." menu and right-click menu (same content). */
export function ChatMenuItems({ conv, onRename, ctx = false }: { conv: ConvSummary; onRename: () => void; ctx?: boolean }) {
  const Mn: M = ctx ? asM(ContextMenu) : DropdownMenu;
  const projects = useApp((s) => s.projects);
  const { setPinned, setArchived, moveToProject, askDeleteConv } = useApp.getState();
  return (
    <>
      <Mn.Item className="menu-item" onSelect={() => setTimeout(onRename, 0)}>
        <Pencil size={16} /> Rename
      </Mn.Item>
      <Mn.Item className="menu-item" onSelect={() => setPinned(conv.id, !conv.pinned)}>
        {conv.pinned ? <PinOff size={16} /> : <Pin size={16} />} {conv.pinned ? "Unpin" : "Pin"}
      </Mn.Item>
      <Mn.Sub>
        <Mn.SubTrigger className="menu-item">
          <FolderInput size={16} /> Move to project
          <ChevronRight size={15} className="ml-auto text-subtle" />
        </Mn.SubTrigger>
        <Mn.Portal>
          <Mn.SubContent className="menu pop-in max-h-80 overflow-y-auto scroll-thin" sideOffset={6} alignOffset={-6}>
            {projects.length === 0 && <div className="menu-label">No projects yet</div>}
            {projects.map((p) => (
              <Mn.Item key={p.id} className="menu-item" disabled={conv.project_id === p.id} onSelect={() => moveToProject(conv.id, p.id)}>
                <FolderIcon color={p.color} size={16} />
                <span className="truncate">{p.name}</span>
                {conv.project_id === p.id && <Check size={15} className="ml-auto" />}
              </Mn.Item>
            ))}
            {conv.project_id && (
              <>
                <Mn.Separator className="menu-sep" />
                <Mn.Item className="menu-item" onSelect={() => moveToProject(conv.id, null)}>
                  <FolderMinus size={16} /> Remove from project
                </Mn.Item>
              </>
            )}
          </Mn.SubContent>
        </Mn.Portal>
      </Mn.Sub>
      <Mn.Item className="menu-item" onSelect={() => setArchived(conv.id, !conv.archived)}>
        {conv.archived ? <ArchiveRestore size={16} /> : <Archive size={16} />} {conv.archived ? "Unarchive" : "Archive"}
      </Mn.Item>
      <Mn.Separator className="menu-sep" />
      <Mn.Item className="menu-item danger" onSelect={() => setTimeout(() => askDeleteConv(conv.id), 0)}>
        <Trash size={16} /> Delete
      </Mn.Item>
    </>
  );
}
