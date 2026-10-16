// The left sidebar, laid out like the Claude desktop app: a row of icon buttons, a
// short list (New chat, Search), pinned chats, recents, and Settings at the bottom.
// Account details stay in Settings.
import { type ReactNode, useEffect, useRef, useState } from "react";
import { Button as AriaButton, Menu, MenuItem, MenuTrigger, Popover } from "react-aria-components";
import { ArrowLeft, ArrowRight, DotsHorizontal, Edit03, LayoutLeft, Pin02, Plus, SearchLg, Settings01, Trash01, XClose } from "@untitledui/icons";
import type { ChatSummary } from "@/lib/api";
import { cx } from "@/utils/cx";

/** Relay's mark (src/assets/relay-mark.svg): a lowercase r of two linked nodes, the
 * local hands (dot) handing off to the cloud brain (ring). */
export const RelayMark = ({ className = "size-8" }: { className?: string }) => (
    <svg viewBox="0 0 1024 1024" className={cx("shrink-0", className)} aria-hidden>
        <rect width="1024" height="1024" rx="232" style={{ fill: "var(--color-bg-brand-solid)" }} />
        <g fill="none" stroke="white" strokeWidth="116" strokeLinecap="round" strokeLinejoin="round">
            <path d="M312 742V478a140 140 0 0 1 140-140H512" />
            <circle cx="652" cy="338" r="130" strokeWidth="108" />
        </g>
        <circle cx="312" cy="742" r="124" fill="white" />
    </svg>
);

const DAY = 24 * 60 * 60 * 1000;

/** Recents grouped by day, newest first. */
export const groupByDay = (chats: ChatSummary[], now = Date.now()) => {
    const start = new Date(now);
    start.setHours(0, 0, 0, 0);
    const today = start.getTime();
    const groups: { label: string; chats: ChatSummary[] }[] = [
        { label: "Today", chats: [] },
        { label: "Yesterday", chats: [] },
        { label: "Previous 7 days", chats: [] },
        { label: "Previous 30 days", chats: [] },
        { label: "Older", chats: [] },
    ];
    for (const c of chats) {
        const at = c.updated;
        const i = at >= today ? 0 : at >= today - DAY ? 1 : at >= today - 7 * DAY ? 2 : at >= today - 30 * DAY ? 3 : 4;
        groups[i].chats.push(c);
    }
    return groups.filter((g) => g.chats.length);
};

const IconButton = ({ label, onClick, disabled, children }: { label: string; onClick: () => void; disabled?: boolean; children: ReactNode }) => (
    <button
        onClick={onClick}
        disabled={disabled}
        aria-label={label}
        title={label}
        className="relative z-40 flex size-7 shrink-0 cursor-pointer items-center justify-center rounded-md text-fg-tertiary transition duration-100 outline-focus-ring hover:bg-tertiary hover:text-fg-secondary focus-visible:outline-2 disabled:cursor-default disabled:opacity-35 disabled:hover:bg-transparent"
    >
        {children}
    </button>
);

/** One line of the sidebar list: a small icon and a label, nothing louder. */
const Row = ({ icon: Icon, label, onClick, active }: { icon: typeof Plus; label: string; onClick: () => void; active?: boolean }) => (
    <button
        aria-current={active ? "page" : undefined}
        onClick={onClick}
        className={cx(
            "flex h-7 w-full cursor-pointer items-center gap-2.5 rounded-md px-2 text-left text-sm transition duration-100 outline-focus-ring focus-visible:outline-2",
            active ? "bg-quaternary text-primary" : "text-secondary hover:bg-tertiary hover:text-primary",
        )}
    >
        <Icon className="size-4 shrink-0 text-fg-secondary" />
        <span className="truncate">{label}</span>
    </button>
);

/** The menu on a chat: Pin, Rename and Delete, as in the Claude app. Also used by the chat's title bar. */
export const ChatMenu = ({
    title,
    pinned,
    onPin,
    onRename,
    onDelete,
    className,
    children,
}: {
    title: string;
    pinned: boolean;
    onPin: () => void;
    onRename: () => void;
    onDelete: () => void;
    className?: string;
    children?: ReactNode;
}) => {
    const item =
        "flex cursor-pointer items-center gap-2.5 rounded-md px-2 py-1.5 text-sm text-secondary outline-hidden data-[focused]:bg-tertiary data-[focused]:text-primary";
    return (
        <MenuTrigger>
            <AriaButton aria-label={`Options for ${title}`} className={className}>
                {children ?? <DotsHorizontal className="size-4" />}
            </AriaButton>
            <Popover placement="bottom end" offset={4} className="min-w-44 rounded-xl bg-primary_alt p-1 shadow-xl ring-1 ring-secondary outline-hidden">
                <Menu
                    aria-label={`Options for ${title}`}
                    className="outline-hidden"
                    onAction={(key) => {
                        if (key === "pin") onPin();
                        if (key === "rename") onRename();
                        if (key === "delete") onDelete();
                    }}
                >
                    <MenuItem id="pin" className={item}>
                        <Pin02 className="size-4 text-fg-tertiary" />
                        {pinned ? "Unpin" : "Pin"}
                    </MenuItem>
                    <MenuItem id="rename" className={item}>
                        <Edit03 className="size-4 text-fg-tertiary" />
                        Rename
                    </MenuItem>
                    <MenuItem id="delete" className={cx(item, "text-error-primary data-[focused]:text-error-primary")}>
                        <Trash01 className="size-4" />
                        Delete
                    </MenuItem>
                </Menu>
            </Popover>
        </MenuTrigger>
    );
};

const ChatRow = ({
    chat,
    active,
    renaming,
    onOpen,
    onStartRename,
    onRename,
    onPin,
    onDelete,
}: {
    chat: ChatSummary;
    active: boolean;
    renaming: boolean;
    onOpen: () => void;
    onStartRename: () => void;
    onRename: (title: string | null) => void;
    onPin: () => void;
    onDelete: () => void;
}) => {
    const [name, setName] = useState(chat.title);
    const field = useRef<HTMLInputElement>(null);
    useEffect(() => {
        if (renaming) {
            setName(chat.title);
            requestAnimationFrame(() => field.current?.select());
        }
    }, [renaming]);
    const commit = () => {
        const next = name.trim();
        onRename(next && next !== chat.title ? next : null);
    };

    if (renaming)
        return (
            <input
                ref={field}
                aria-label="Chat name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                onBlur={commit}
                onKeyDown={(e) => {
                    if (e.key === "Enter") commit();
                    if (e.key === "Escape") onRename(null);
                }}
                className="h-7 w-full rounded-md bg-primary px-2 text-sm text-primary ring-1 ring-brand outline-hidden"
            />
        );
    return (
        <div
            className={cx(
                "group relative flex h-7 items-center rounded-md text-sm transition duration-100",
                active ? "bg-quaternary text-primary" : "text-secondary hover:bg-tertiary hover:text-primary",
            )}
        >
            <button
                onClick={onOpen}
                onDoubleClick={onStartRename}
                title={chat.title}
                aria-current={active ? "page" : undefined}
                className="flex h-full min-w-0 flex-1 cursor-pointer items-center gap-2.5 px-2 text-left outline-focus-ring focus-visible:outline-2"
            >
                <span className={cx("size-[7px] shrink-0 rounded-full ring-1", active ? "bg-fg-brand-primary ring-transparent" : "ring-fg-quaternary")} />
                <span className="truncate">{chat.title}</span>
            </button>
            <ChatMenu
                title={chat.title}
                pinned={chat.pinned}
                onPin={onPin}
                onRename={onStartRename}
                onDelete={onDelete}
                className={cx(
                    "mr-1 flex size-6 shrink-0 cursor-pointer items-center justify-center rounded-md text-fg-quaternary transition outline-hidden hover:bg-quaternary/60 hover:text-fg-secondary data-[focus-visible]:opacity-100 data-[pressed]:opacity-100 aria-expanded:opacity-100",
                    active ? "opacity-100" : "opacity-0 group-hover:opacity-100",
                )}
            />
        </div>
    );
};

export const Sidebar = ({
    collapsed,
    onToggle,
    chats,
    activeId,
    settingsOpen,
    canBack,
    canForward,
    onBack,
    onForward,
    onNew,
    onOpen,
    renamingId,
    onStartRename,
    onRename,
    onPin,
    onDelete,
    onSettings,
}: {
    collapsed: boolean;
    onToggle: () => void;
    chats: ChatSummary[];
    activeId: string | null;
    settingsOpen: boolean;
    canBack: boolean;
    canForward: boolean;
    onBack: () => void;
    onForward: () => void;
    onNew: () => void;
    onOpen: (id: string) => void;
    renamingId: string | null;
    onStartRename: (id: string) => void;
    onRename: (id: string, title: string | null) => void;
    onPin: (id: string, pinned: boolean) => void;
    onDelete: (id: string) => void;
    onSettings: () => void;
}) => {
    const [searching, setSearching] = useState(false);
    const [query, setQuery] = useState("");
    const q = query.trim().toLowerCase();
    const shown = q ? chats.filter((c) => c.title.toLowerCase().includes(q)) : chats;
    const pinned = shown.filter((c) => c.pinned);
    const groups = groupByDay(shown.filter((c) => !c.pinned));
    const row = (c: ChatSummary) => (
        <ChatRow
            key={c.id}
            chat={c}
            active={c.id === activeId}
            renaming={c.id === renamingId}
            onOpen={() => onOpen(c.id)}
            onStartRename={() => onStartRename(c.id)}
            onRename={(t) => onRename(c.id, t)}
            onPin={() => onPin(c.id, !c.pinned)}
            onDelete={() => onDelete(c.id)}
        />
    );
    const heading = (label: string) => <span className="px-2 pt-6 pb-1.5 text-[13px] text-quaternary">{label}</span>;
    const closeSearch = () => {
        setQuery("");
        setSearching(false);
    };

    if (collapsed)
        return (
            <nav aria-label="Relay" className="flex w-12 shrink-0 flex-col items-center gap-1 border-r border-secondary bg-secondary pt-1 pb-2.5">
                <IconButton label="Open sidebar" onClick={onToggle}>
                    <LayoutLeft className="size-4" />
                </IconButton>
                <IconButton label="New chat" onClick={onNew}>
                    <Plus className="size-4" />
                </IconButton>
                <IconButton
                    label="Search chats"
                    onClick={() => {
                        onToggle();
                        setSearching(true);
                    }}
                >
                    <SearchLg className="size-4" />
                </IconButton>
                <div className="mt-auto">
                    <IconButton label="Settings" onClick={onSettings}>
                        <Settings01 className="size-4" />
                    </IconButton>
                </div>
            </nav>
        );

    return (
        <nav aria-label="Relay" className="flex w-[288px] shrink-0 flex-col border-r border-secondary bg-secondary px-2 pb-2">
            <div className="flex h-9 items-center gap-1 px-0.5">
                <IconButton label="Close sidebar" onClick={onToggle}>
                    <LayoutLeft className="size-4" />
                </IconButton>
                <IconButton label="Back" onClick={onBack} disabled={!canBack}>
                    <ArrowLeft className="size-4" />
                </IconButton>
                <IconButton label="Forward" onClick={onForward} disabled={!canForward}>
                    <ArrowRight className="size-4" />
                </IconButton>
            </div>
            <div className="mt-2 flex flex-col gap-0.5">
                <Row icon={Plus} label="New chat" onClick={onNew} />
                {searching ? (
                    <div className="flex h-7 items-center gap-2.5 rounded-md bg-primary px-2 ring-1 ring-secondary focus-within:ring-brand">
                        <SearchLg className="size-4 shrink-0 text-fg-tertiary" />
                        <input
                            autoFocus
                            aria-label="Search chats"
                            placeholder="Search chats"
                            value={query}
                            onChange={(e) => setQuery(e.target.value)}
                            onKeyDown={(e) => e.key === "Escape" && closeSearch()}
                            className="min-w-0 flex-1 bg-transparent text-sm text-primary outline-hidden placeholder:text-placeholder"
                        />
                        <button aria-label="Close search" onClick={closeSearch} className="cursor-pointer text-fg-quaternary hover:text-fg-secondary">
                            <XClose className="size-3.5" />
                        </button>
                    </div>
                ) : (
                    <Row icon={SearchLg} label="Search" onClick={() => setSearching(true)} />
                )}
            </div>
            <div className="-mx-1 mt-1 flex min-h-0 flex-1 flex-col overflow-y-auto px-1 pb-2">
                {pinned.length > 0 && (
                    <>
                        {heading("Pinned")}
                        <div className="flex flex-col gap-0.5">{pinned.map(row)}</div>
                    </>
                )}
                {groups.map((g) => (
                    <div key={g.label} className="flex flex-col">
                        {heading(g.label)}
                        <div className="flex flex-col gap-0.5">{g.chats.map(row)}</div>
                    </div>
                ))}
                {q && shown.length === 0 && <p className="px-2 pt-5 text-sm text-tertiary">No chats match "{query.trim()}".</p>}
                {!q && chats.length === 0 && <p className="px-2 pt-5 text-xs text-quaternary">Your chats will show here.</p>}
            </div>
            <div className="border-t border-secondary pt-2">
                <Row icon={Settings01} label="Settings" active={settingsOpen} onClick={onSettings} />
            </div>
        </nav>
    );
};
