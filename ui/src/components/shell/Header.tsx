import { useState, type ReactNode } from "react";
import { Ellipsis, Menu, SquarePen } from "lucide-react";
import { useApp, findConv } from "@/store/app";
import { useConversations } from "@/hooks/queries";
import { useNarrow } from "@/hooks/dom";
import { cn } from "@/lib/utils";
import { go, paths } from "@/lib/nav";
import { shortcutKeys } from "@/lib/platform";
import { IconButton } from "@/components/ui/icon-button";
import { useShortcutLabel } from "@/components/ui/kbd";
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { ChatMenuItems, useChatActions } from "./ChatMenu";
import { RenameDialog } from "./Dialogs";

/** Top bar of the main column: drag region, narrow-window menu, title, chat actions. */
export function Header({ title, chatId, right }: { title?: ReactNode; chatId?: string | null; right?: ReactNode }) {
  const narrow = useNarrow();
  const collapsed = useApp((s) => s.sidebarCollapsed);
  const toggle = useApp((s) => s.toggleSidebar);
  const sideLabel = useShortcutLabel(shortcutKeys("toggleSidebar"));
  const newLabel = useShortcutLabel(shortcutKeys("newChat"));
  return (
    <header
      data-tauri-drag-region
      className={cn(
        "flex h-(--relay-header-h) shrink-0 items-center gap-1 pl-2 pr-[max(8px,var(--relay-controls-w))]",
        narrow ? "pl-[calc(var(--relay-titlebar-inset)+8px)]" : collapsed ? "pl-[max(8px,calc(var(--relay-titlebar-inset)-var(--relay-sidebar-w-collapsed)+8px))]" : "",
      )}
    >
      {narrow && (
        <>
          <IconButton label="Open sidebar" shortcut={sideLabel} onClick={() => toggle(true)} className="no-drag">
            <Menu aria-hidden="true" />
          </IconButton>
          <IconButton label="New chat" shortcut={newLabel} onClick={() => go(paths.home())} className="no-drag">
            <SquarePen aria-hidden="true" />
          </IconButton>
        </>
      )}
      <div className="min-w-0 flex-1 truncate px-2 text-ui font-medium text-fg" data-tauri-drag-region>
        {title}
      </div>
      <div className="no-drag flex items-center gap-1">
        {right}
        {chatId && !chatId.startsWith("tmp-") && <ChatOverflow id={chatId} />}
      </div>
    </header>
  );
}

function ChatOverflow({ id }: { id: string }) {
  useConversations();
  const conv = findConv(id);
  const [open, setOpen] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const actions = useChatActions();
  if (!conv) return null;
  return (
    <>
      <DropdownMenu open={open} onOpenChange={setOpen}>
        <DropdownMenuTrigger asChild>
          <IconButton label="Chat options" noTip>
            <Ellipsis aria-hidden="true" />
          </IconButton>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-56">
          <ChatMenuItems conv={conv} kind="dropdown" onRename={() => setRenaming(true)} />
        </DropdownMenuContent>
      </DropdownMenu>
      <RenameDialog open={renaming} initial={conv.title || "New chat"} onClose={() => setRenaming(false)} onRename={(t) => actions.rename(conv.id, t)} />
    </>
  );
}

/** The current chat's title (from the cached list). */
export function useChatTitle(id: string | null): string | null {
  const convs = useConversations();
  if (!id) return null;
  return convs.data?.find((c) => c.id === id)?.title || (id.startsWith("tmp-") ? "New chat" : null);
}
