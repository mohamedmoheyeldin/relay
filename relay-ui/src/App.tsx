import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Dialog, Heading } from "react-aria-components";
import { MessageChatCircle, Settings01 } from "@untitledui/icons";
import { Button } from "@/components/base/buttons/button";
import { Modal, ModalOverlay } from "@/components/application/modals/modal";
import { Checkbox } from "@/components/base/checkbox/checkbox";
import { type AppInfo, type Settings, type UpdateInfo, api } from "@/lib/api";
import { cx } from "@/utils/cx";
import { ChatScreen } from "./screens/chat";
import { useClaudeState } from "./screens/claude-setup";
import { SettingsScreen } from "./screens/settings";
import { Wizard } from "./screens/wizard";

const RelayMark = () => (
    <svg viewBox="0 0 32 32" className="size-8" aria-hidden>
        <rect width="32" height="32" rx="8" style={{ fill: "var(--color-bg-brand-solid)" }} />
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
            <Modal>
                <Dialog className="w-full max-w-md rounded-2xl bg-primary p-6 shadow-xl outline-hidden">
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

export default function App() {
    const [settings, setSettings] = useState<Settings | null>(null);
    const [info, setInfo] = useState<AppInfo | null>(null);
    const [view, setView] = useState<"chat" | "settings" | "setup">("chat");
    const [settingsPage, setSettingsPage] = useState("general");
    const [closeAsk, setCloseAsk] = useState(false);
    const [update, setUpdate] = useState<UpdateInfo | null>(null);
    const { state: claude, refresh: refreshClaude } = useClaudeState();

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

    if (!settings || !info) return <div className="flex h-screen items-center justify-center bg-primary text-tertiary">Starting Relay…</div>;

    const ready = !!claude?.installed && !!claude.auth.loggedIn;
    const nav = [
        { id: "chat" as const, label: "Chat", icon: MessageChatCircle },
        { id: "settings" as const, label: "Settings", icon: Settings01 },
    ];

    return (
        <div className="flex h-screen bg-primary">
            <CloseDialog open={closeAsk} onClose={() => setCloseAsk(false)} />
            {view !== "setup" && (
                <nav className="flex w-16 shrink-0 flex-col items-center gap-2 border-r border-secondary bg-secondary_alt py-4">
                    <RelayMark />
                    <div className="h-4" />
                    {nav.map((n) => (
                        <button
                            key={n.id}
                            title={n.label}
                            aria-label={n.label}
                            onClick={() => setView(n.id)}
                            className={cx(
                                "flex size-10 cursor-pointer items-center justify-center rounded-lg transition",
                                view === n.id ? "bg-active text-fg-secondary" : "text-fg-quaternary hover:bg-primary_hover",
                            )}
                        >
                            <n.icon className="size-5" />
                        </button>
                    ))}
                </nav>
            )}
            <div className="flex min-w-0 flex-1 flex-col">
                {update && view !== "setup" && (
                    <div className="flex items-center justify-between gap-3 bg-brand-section_subtle px-6 py-2 text-sm text-white">
                        <span>Relay {update.latest} is available.</span>
                        <Button
                            size="sm"
                            color="secondary"
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
                <div className="min-h-0 flex-1">
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
                    {view === "chat" && <ChatScreen ready={ready} onSetup={() => setView("setup")} />}
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
