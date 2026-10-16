// The left sidebar, laid out like the Claude desktop app: New chat and search at the
// top, pinned chats, recents grouped by day, and the account row at the bottom.
import { type ReactNode, useEffect, useRef, useState } from "react";
import { Menu, MenuItem, MenuTrigger, Popover } from "react-aria-components";
import { DotsHorizontal, Edit03, LayoutLeft, Pin02, Plus, SearchLg, Settings01, Trash01, XClose } from "@untitledui/icons";
import type { ChatSummary } from "@/lib/api";
import { useClaude } from "@/lib/claude";
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

/** Recents grouped the way the Claude app does it. */
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

const IconButton = ({ label, onClick, children }: { label: string; onClick: () => void; children: ReactNode }) => (
    <button
        onClick={onClick}
        aria-label={label}
        title={label}
        className="flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-lg text-fg-tertiary transition duration-150 outline-focus-ring hover:bg-tertiary/70 hover:text-fg-secondary focus-visible:outline-2"
    >
        {children}
    </button>
);

const NavRow = ({
    icon: Icon,
    label,
    collapsed,
    onClick,
    active,
    accent,
}: {
    icon: typeof Plus;
    label: string;
    collapsed: boolean;
    onClick: () => void;
    active?: boolean;
    accent?: boolean;
}) => (
    <button
        title={collapsed ? label : undefined}
        aria-label={label}
        aria-current={active ? "page" : undefined}
        onClick={onClick}
        className={cx(
            "group flex h-9 w-full cursor-pointer items-center gap-2.5 rounded-lg px-1.5 text-sm transition duration-150 ease-out outline-focus-ring focus-visible:outline-2",
            active ? "bg-tertiary font-medium text-primary" : "text-secondary hover:bg-tertiary/70 hover:text-primary",
            accent && "font-medium text-primary",
        )}
    >
        {accent ? (
            <span className="flex size-6 shrink-0 items-center justify-center rounded-full bg-brand-solid text-white transition duration-150 group-hover:scale-105 group-hover:bg-brand-solid_hover">
                <Icon className="size-4" />
            </span>
        ) : (
            <span className="flex size-6 shrink-0 items-center justify-center">
                <Icon className={cx("size-[18px] transition", active ? "text-fg-secondary" : "text-fg-tertiary group-hover:text-fg-secondary")} />
            </span>
        )}
        <span className={cx("truncate", collapsed && "sr-only")}>{label}</span>
    </button>
);

const menuItem =
    "flex cursor-pointer items-center gap-2 rounded-md px-2.5 py-1.5 text-sm text-secondary outline-hidden data-[focused]:bg-tertiary data-[focused]:text-primary";

const ChatRow = ({
    chat,
    active,
    onOpen,
    onRename,
    onPin,
    onDelete,
}: {
    chat: ChatSummary;
    active: boolean;
    onOpen: () => void;
    onRename: (title: string) => void;
    onPin: () => void;
    onDelete: () => void;
}) => {
    const [editing, setEditing] = useState(false);
    const [name, setName] = useState(chat.title);
    const field = useRef<HTMLInputElement>(null);
    useEffect(() => {
        if (editing) field.current?.select();
    }, [editing]);
    const commit = () => {
        setEditing(false);
        const next = name.trim();
        if (next && next !== chat.title) onRename(next);
        else setName(chat.title);
    };

    if (editing)
        return (
            <input
                ref={field}
                aria-label="Chat name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                onBlur={commit}
                onKeyDown={(e) => {
                    if (e.key === "Enter") commit();
                    if (e.key === "Escape") {
                        setName(chat.title);
                        setEditing(false);
                    }
                }}
                className="h-8 w-full rounded-lg bg-primary px-2 text-sm text-primary ring-1 ring-brand outline-hidden"
            />
        );
    return (
        <div
            className={cx(
                "group relative flex h-8 items-center rounded-lg text-sm transition duration-150",
                active ? "bg-tertiary text-primary" : "text-secondary hover:bg-tertiary/70 hover:text-primary",
            )}
        >
            <button
                onClick={onOpen}
                onDoubleClick={() => setEditing(true)}
                title={chat.title}
                aria-current={active ? "page" : undefined}
                className="h-full min-w-0 flex-1 cursor-pointer truncate px-2 text-left outline-focus-ring focus-visible:outline-2"
            >
                {chat.title}
            </button>
            <MenuTrigger>
                <button
                    aria-label={`More for ${chat.title}`}
                    className={cx(
                        "mr-1 flex size-6 shrink-0 cursor-pointer items-center justify-center rounded-md text-fg-quaternary transition hover:bg-quaternary/60 hover:text-fg-secondary focus-visible:opacity-100 aria-expanded:opacity-100",
                        active ? "opacity-100" : "opacity-0 group-hover:opacity-100",
                    )}
                >
                    <DotsHorizontal className="size-4" />
                </button>
                <Popover placement="bottom end" className="min-w-40 rounded-xl bg-primary p-1 shadow-lg ring-1 ring-secondary outline-hidden animate-in fade-in zoom-in-95 duration-150">
                    <Menu
                        className="outline-hidden"
                        onAction={(key) => {
                            if (key === "pin") onPin();
                            if (key === "rename") setEditing(true);
                            if (key === "delete") onDelete();
                        }}
                    >
                        <MenuItem id="pin" className={menuItem}>
                            <Pin02 className="size-4" />
                            {chat.pinned ? "Unpin" : "Pin"}
                        </MenuItem>
                        <MenuItem id="rename" className={menuItem}>
                            <Edit03 className="size-4" />
                            Rename
                        </MenuItem>
                        <MenuItem id="delete" className={cx(menuItem, "text-error-primary data-[focused]:text-error-primary")}>
                            <Trash01 className="size-4" />
                            Delete
                        </MenuItem>
                    </Menu>
                </Popover>
            </MenuTrigger>
        </div>
    );
};

const initials = (text: string) =>
    text
        .split(/[\s@._-]+/)
        .filter(Boolean)
        .slice(0, 2)
        .map((w) => w[0]?.toUpperCase())
        .join("") || "R";

/** The account row: who Claude is signed in as, like the Claude app's footer. */
const Account = ({ collapsed, onOpen }: { collapsed: boolean; onOpen: () => void }) => {
    const { state } = useClaude();
    const auth = state?.auth;
    const name = auth?.loggedIn ? auth.email || "Claude account" : "Not signed in to Claude";
    const plan = auth?.loggedIn && auth.subscriptionType ? auth.subscriptionType.replace(/^\w/, (c) => c.toUpperCase()) : auth?.loggedIn ? "" : "Local AI only";
    return (
        <button
            onClick={onOpen}
            title={collapsed ? name : "Settings"}
            className="group flex h-11 w-full cursor-pointer items-center gap-2.5 rounded-lg px-1 text-left transition duration-150 outline-focus-ring hover:bg-tertiary/70 focus-visible:outline-2"
        >
            <span className="flex size-7 shrink-0 items-center justify-center rounded-full bg-quaternary text-xs font-semibold text-secondary">
                {auth?.loggedIn ? initials(auth.email || "C") : "R"}
            </span>
            {!collapsed && (
                <span className="flex min-w-0 flex-1 flex-col leading-tight">
                    <span className="truncate text-sm font-medium text-primary">{name}</span>
                    {plan && <span className="truncate text-xs text-tertiary">{plan}</span>}
                </span>
            )}
            {!collapsed && <Settings01 className="size-4 shrink-0 text-fg-quaternary transition group-hover:text-fg-secondary" />}
        </button>
    );
};

export const Sidebar = ({
    collapsed,
    onToggle,
    chats,
    activeId,
    onNew,
    onOpen,
    onRename,
    onPin,
    onDelete,
    onSettings,
}: {
    collapsed: boolean;
    onToggle: () => void;
    chats: ChatSummary[];
    activeId: string | null;
    onNew: () => void;
    onOpen: (id: string) => void;
    onRename: (id: string, title: string) => void;
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
            onOpen={() => onOpen(c.id)}
            onRename={(t) => onRename(c.id, t)}
            onPin={() => onPin(c.id, !c.pinned)}
            onDelete={() => onDelete(c.id)}
        />
    );
    const heading = (label: string) => <span className="px-2 pt-4 pb-1 text-xs font-medium text-quaternary">{label}</span>;

    return (
        <nav aria-label="Relay" className={cx("flex shrink-0 flex-col bg-secondary px-2 py-3 transition-[width] duration-200 ease-out", collapsed ? "w-[52px]" : "w-64")}>
            <div className={cx("mb-3 flex h-9 items-center gap-2", collapsed ? "justify-center" : "justify-between pl-1.5")}>
                {!collapsed && (
                    <div className="flex min-w-0 items-center gap-2 animate-in fade-in duration-200">
                        <RelayMark className="size-6" />
                        <span className="text-md font-semibold tracking-tight text-primary">Relay</span>
                    </div>
                )}
                <IconButton label={collapsed ? "Open sidebar" : "Close sidebar"} onClick={onToggle}>
                    <LayoutLeft className="size-[18px]" />
                </IconButton>
            </div>
            <div className="flex flex-col gap-0.5">
                <NavRow icon={Plus} label="New chat" accent collapsed={collapsed} onClick={onNew} />
                {collapsed ? (
                    <NavRow icon={SearchLg} label="Search chats" collapsed onClick={onToggle} />
                ) : searching ? (
                    <div className="flex h-9 items-center gap-2 rounded-lg bg-primary px-2 ring-1 ring-secondary focus-within:ring-brand animate-in fade-in duration-150">
                        <SearchLg className="size-4 shrink-0 text-fg-quaternary" />
                        <input
                            autoFocus
                            aria-label="Search chats"
                            placeholder="Search chats"
                            value={query}
                            onChange={(e) => setQuery(e.target.value)}
                            onKeyDown={(e) => {
                                if (e.key === "Escape") {
                                    setQuery("");
                                    setSearching(false);
                                }
                            }}
                            className="min-w-0 flex-1 bg-transparent text-sm text-primary outline-hidden placeholder:text-placeholder"
                        />
                        <button
                            aria-label="Close search"
                            onClick={() => {
                                setQuery("");
                                setSearching(false);
                            }}
                            className="cursor-pointer text-fg-quaternary hover:text-fg-secondary"
                        >
                            <XClose className="size-4" />
                        </button>
                    </div>
                ) : (
                    <NavRow icon={SearchLg} label="Search chats" collapsed={false} onClick={() => setSearching(true)} />
                )}
            </div>
            {!collapsed && (
                <div className="-mx-1 mt-1 flex min-h-0 flex-1 flex-col overflow-y-auto px-1 animate-in fade-in duration-200">
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
                    {q && shown.length === 0 && <p className="px-2 pt-4 text-sm text-tertiary">No chats match "{query.trim()}".</p>}
                </div>
            )}
            <div className="mt-auto border-t border-secondary pt-2">
                <Account collapsed={collapsed} onOpen={onSettings} />
            </div>
        </nav>
    );
};
