import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Dialog, Heading } from "react-aria-components";
import { LayoutLeft, MessageChatCircle, Plus, Settings01 } from "@untitledui/icons";
import { Button } from "@/components/base/buttons/button";
import { Modal, ModalOverlay } from "@/components/application/modals/modal";
import { Checkbox } from "@/components/base/checkbox/checkbox";
import { type AppInfo, type Settings, type UpdateInfo, api } from "@/lib/api";
import { cx } from "@/utils/cx";
import { ApprovalTray } from "@/components/relay/approval";
import { ChatScreen } from "./screens/chat";
import { useClaude } from "@/lib/claude";
import { SettingsScreen } from "./screens/settings";
import { Wizard } from "./screens/wizard";

const RelayMark = ({ className = "size-8" }: { className?: string }) => (
    <svg viewBox="0 0 32 32" className={cx("shrink-0", className)} aria-hidden>
        <rect width="32" height="32" rx="9" style={{ fill: "var(--color-bg-brand-solid)" }} />
        <path d="M9 21V11h6.5a3.5 3.5 0 0 1 0 7H9m6.5 0L21 23" stroke="white" strokeWidth="2.4" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
);

const CloseDialog = ({ open, onClose }: { open: boolean; onClose: () => void }) => {
    const [remember, setRemember] = useState(true);
    const choose = async (action: "tray" | "quit") => {
        onClose();
        await api.closeChoice(action, remember);
    };
    return (
        <ModalOverlay isOpen={open} onOpenChange={(o) => !o && onClose()} isDismissable>
            <Modal className="max-w-md">
                <Dialog className="p-6 outline-hidden">
                    <Heading slot="title" className="text-lg font-semibold text-primary">
                        Keep Relay running?
                    </Heading>
                    <p className="mt-2 text-sm text-tertiary">
                        Relay can stay in the tray so it is ready for your next task and can finish the current one. You can change this in Settings
                        &gt; General.
                    </p>
                    <div className="mt-4">
                        <Checkbox label="Don't ask again" isSelected={remember} onChange={setRemember} />
                    </div>
                    <div className="mt-6 flex flex-col-reverse gap-3 sm:flex-row sm:justify-end">
                        <Button color="secondary" onClick={() => choose("quit")}>
                            Quit Relay
                        </Button>
                        <Button onClick={() => choose("tray")}>Keep running in tray</Button>
                    </div>
                </Dialog>
            </Modal>
        </ModalOverlay>
    );
};

const COLLAPSED_KEY = "relay-sidebar-collapsed";
const readCollapsed = () => {
    try {
        return localStorage.getItem(COLLAPSED_KEY) === "1";
    } catch {
        return false;
    }
};

/** One quiet row in the sidebar; only its label hides when the sidebar collapses. */
const NavItem = ({
    icon: Icon,
    label,
    active,
    collapsed,
    onClick,
    accent,
}: {
    icon: typeof Plus;
    label: string;
    active?: boolean;
    collapsed: boolean;
    onClick: () => void;
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

export default function App() {
    const [settings, setSettings] = useState<Settings | null>(null);
    const [info, setInfo] = useState<AppInfo | null>(null);
    const [view, setView] = useState<"chat" | "settings" | "setup">("chat");
    const [settingsPage, setSettingsPage] = useState("general");
    const [closeAsk, setCloseAsk] = useState(false);
    const [update, setUpdate] = useState<UpdateInfo | null>(null);
    const [collapsed, setCollapsed] = useState(readCollapsed);
    // Bumped by the sidebar's "New chat"; the chat screen then starts a fresh conversation.
    const [newChat, setNewChat] = useState(0);
    const toggleSidebar = () => {
        setCollapsed((c) => {
            try {
                localStorage.setItem(COLLAPSED_KEY, c ? "0" : "1");
            } catch {
                /* the sidebar still works without saving its state */
            }
            return !c;
        });
    };
    const { signedIn: ready, refresh: refreshClaude } = useClaude();

    useEffect(() => {
        Promise.all([api.getSettings(), api.appInfo()]).then(([s, i]) => {
            setSettings(s);
            setInfo(i);
            if (!s.setup.completed) setView("setup");
        });
        const subs = [
            listen("close-requested", () => setCloseAsk(true)),
            listen<string>("navigate", (e) => setView(e.payload === "settings" ? "settings" : "chat")),
            listen<UpdateInfo>("update-available", (e) => setUpdate(e.payload)),
        ];
        return () => {
            subs.forEach((p) => p.then((f) => f()));
        };
    }, []);

    const save = useCallback(async (next: Settings) => {
        setSettings(next);
        setSettings(await api.saveSettings(next));
    }, []);

    if (!settings || !info)
        return (
            <div className="flex h-screen flex-col items-center justify-center gap-4 bg-primary text-sm text-tertiary">
                <RelayMark className="size-10 animate-pulse" />
                Starting Relay…
            </div>
        );

    return (
        <div className="flex h-screen bg-primary">
            <CloseDialog open={closeAsk} onClose={() => setCloseAsk(false)} />
            <ApprovalTray />
            {view !== "setup" && (
                <nav
                    aria-label="Relay"
                    className={cx("flex shrink-0 flex-col bg-secondary px-2 py-3 transition-[width] duration-200 ease-out", collapsed ? "w-[52px]" : "w-60")}
                >
                    <div className={cx("mb-4 flex h-9 items-center gap-2", collapsed ? "justify-center" : "justify-between pl-1.5")}>
                        {!collapsed && (
                            <div className="flex min-w-0 items-center gap-2 animate-in fade-in duration-200">
                                <RelayMark className="size-6" />
                                <span className="text-md font-semibold tracking-tight text-primary">Relay</span>
                            </div>
                        )}
                        <button
                            onClick={toggleSidebar}
                            aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
                            title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
                            className="flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-lg text-fg-tertiary transition outline-focus-ring hover:bg-tertiary/70 hover:text-fg-secondary focus-visible:outline-2"
                        >
                            <LayoutLeft className="size-[18px]" />
                        </button>
                    </div>
                    <div className="flex flex-col gap-0.5">
                        <NavItem
                            icon={Plus}
                            label="New chat"
                            accent
                            collapsed={collapsed}
                            onClick={() => {
                                setView("chat");
                                setNewChat((n) => n + 1);
                            }}
                        />
                        <NavItem icon={MessageChatCircle} label="Chat" active={view === "chat"} collapsed={collapsed} onClick={() => setView("chat")} />
                    </div>
                    <div className="mt-auto flex flex-col gap-0.5">
                        <NavItem icon={Settings01} label="Settings" active={view === "settings"} collapsed={collapsed} onClick={() => setView("settings")} />
                    </div>
                </nav>
            )}
            <div className="flex min-w-0 flex-1 flex-col">
                {update && view !== "setup" && (
                    <div className="flex items-center justify-center gap-3 border-b border-secondary bg-brand-primary px-6 py-1.5 text-sm text-brand-secondary animate-in fade-in slide-in-from-top-1 duration-300">
                        <span>Relay {update.latest} is available.</span>
                        <Button
                            size="sm"
                            color="link-color"
                            onClick={() => {
                                setSettingsPage("updates");
                                setView("settings");
                                setUpdate(null);
                            }}
                        >
                            See update
                        </Button>
                    </div>
                )}
                <div key={view} className="min-h-0 flex-1 animate-in fade-in duration-300">
                    {view === "setup" && (
                        <Wizard
                            settings={settings}
                            save={save}
                            onFinish={async () => {
                                setSettings(await api.getSettings());
                                refreshClaude();
                                setView("chat");
                            }}
                        />
                    )}
                    {view === "chat" && (
                        <ChatScreen
                            ready={ready}
                            newChatRequest={newChat}
                            approvalMode={settings.safety.approval_mode}
                            onSetup={() => setView("setup")}
                            onLocalAi={() => {
                                setSettingsPage("local-ai");
                                setView("settings");
                            }}
                        />
                    )}
                    {view === "settings" && (
                        <SettingsScreen
                            settings={settings}
                            save={save}
                            info={info}
                            rerunSetup={() => setView("setup")}
                            page={settingsPage}
                            setPage={(p) => {
                                setSettingsPage(p);
                                if (p === "claude") refreshClaude();
                            }}
                        />
                    )}
                </div>
            </div>
        </div>
    );
}
