import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Dialog, Heading } from "react-aria-components";
import { Button } from "@/components/base/buttons/button";
import { Modal, ModalOverlay } from "@/components/application/modals/modal";
import { Checkbox } from "@/components/base/checkbox/checkbox";
import { type AppInfo, type ChatSummary, type Settings, type UpdateInfo, api } from "@/lib/api";
import { ApprovalTray } from "@/components/relay/approval";
import { ChatScreen, type OpenChat } from "./screens/chat";
import { RelayMark, Sidebar } from "@/components/relay/sidebar";
import { useClaude } from "@/lib/claude";
import { SettingsScreen } from "./screens/settings";
import { Wizard } from "./screens/wizard";

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

const newId = () => (crypto.randomUUID ? crypto.randomUUID() : `${Date.now()}-${Math.random().toString(36).slice(2)}`);

export default function App() {
    const [settings, setSettings] = useState<Settings | null>(null);
    const [info, setInfo] = useState<AppInfo | null>(null);
    const [view, setView] = useState<"chat" | "settings" | "setup">("chat");
    const [settingsPage, setSettingsPage] = useState("general");
    const [closeAsk, setCloseAsk] = useState(false);
    const [update, setUpdate] = useState<UpdateInfo | null>(null);
    const [collapsed, setCollapsed] = useState(readCollapsed);
    // The conversation on screen; Relay opens on a fresh one, like the Claude app.
    const [chat, setChat] = useState<OpenChat>(() => ({ id: newId(), fresh: true }));
    const [chats, setChats] = useState<ChatSummary[]>([]);
    const loadChats = useCallback(() => {
        api.chatsList()
            .then(setChats)
            .catch(() => {});
    }, []);
    useEffect(loadChats, [loadChats]);
    const deleteChat = async (id: string) => {
        await api.chatDelete(id).catch(() => {});
        if (id === chat.id) setChat({ id: newId(), fresh: true });
        loadChats();
    };
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
                <Sidebar
                    collapsed={collapsed}
                    onToggle={toggleSidebar}
                    chats={chats}
                    activeId={view === "chat" ? chat.id : null}
                    onNew={() => {
                        setView("chat");
                        setChat({ id: newId(), fresh: true });
                    }}
                    onOpen={(id) => {
                        setView("chat");
                        if (id !== chat.id) setChat({ id, fresh: false });
                    }}
                    onRename={(id, title) => api.chatRename(id, title).then(loadChats).catch(() => {})}
                    onPin={(id, pinned) => api.chatPin(id, pinned).then(loadChats).catch(() => {})}
                    onDelete={deleteChat}
                    onSettings={() => setView("settings")}
                />
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
                            chat={chat}
                            onSaved={loadChats}
                            claudeSettings={settings.claude}
                            onClaudeSettings={(claude) => save({ ...settings, claude })}
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
