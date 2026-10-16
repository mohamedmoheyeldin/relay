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
    const [renamingId, setRenamingId] = useState<string | null>(null);
    // Back and forward, like the Claude app's arrows: every chat or Settings visit.
    type Place = { view: "chat"; chat: OpenChat } | { view: "settings" };
    const [trail, setTrail] = useState<{ places: Place[]; at: number }>(() => ({ places: [{ view: "chat", chat }], at: 0 }));
    const show = (p: Place) => {
        setView(p.view);
        if (p.view === "chat") setChat(p.chat);
    };
    const go = (p: Place) => {
        show(p);
        setTrail((t) => {
            const same = t.places[t.at];
            if (same && same.view === p.view && (p.view === "settings" || (same.view === "chat" && same.chat.id === p.chat.id))) return t;
            const places = [...t.places.slice(0, t.at + 1), p].slice(-50);
            return { places, at: places.length - 1 };
        });
    };
    const step = (by: number) => {
        const at = trail.at + by;
        const p = trail.places[at];
        if (!p) return;
        // A chat left before anything was said in it was never saved.
        const known = p.view === "settings" || p.chat.fresh || chats.some((c) => c.id === p.chat.id);
        show(known ? p : { view: "chat", chat: { id: newId(), fresh: true } });
        setTrail({ ...trail, at });
    };
    const openChat = (id: string) => go({ view: "chat", chat: { id, fresh: false } });
    const newChatPlace = () => go({ view: "chat", chat: { id: newId(), fresh: true } });
    const deleteChat = async (id: string) => {
        await api.chatDelete(id).catch(() => {});
        if (id === chat.id) newChatPlace();
        loadChats();
    };
    const renameChat = async (id: string, title: string | null) => {
        setRenamingId(null);
        if (title) await api.chatRename(id, title).catch(() => {});
        loadChats();
    };
    const pinChat = (id: string, pinned: boolean) => api.chatPin(id, pinned).then(loadChats).catch(() => {});
    const current = chats.find((c) => c.id === chat.id) ?? null;
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
                    settingsOpen={view === "settings"}
                    canBack={trail.at > 0}
                    canForward={trail.at < trail.places.length - 1}
                    onBack={() => step(-1)}
                    onForward={() => step(1)}
                    onNew={newChatPlace}
                    onOpen={(id) => (id === chat.id && view === "chat" ? undefined : openChat(id))}
                    renamingId={renamingId}
                    onStartRename={(id) => {
                        if (collapsed) toggleSidebar();
                        setRenamingId(id);
                    }}
                    onRename={renameChat}
                    onPin={pinChat}
                    onDelete={deleteChat}
                    onSettings={() => go({ view: "settings" })}
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
                            saved={current}
                            onRename={() => current && (collapsed ? (toggleSidebar(), setRenamingId(current.id)) : setRenamingId(current.id))}
                            onPin={() => current && pinChat(current.id, !current.pinned)}
                            onDelete={() => current && deleteChat(current.id)}
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
