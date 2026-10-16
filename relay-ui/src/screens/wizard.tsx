// First-run setup (D-044): one page of checks that run on their own. Buttons only
// appear where something needs the owner: installing Cloud AI, signing in,
// downloading the suggested model.
import { type ReactNode, useEffect } from "react";
import { ArrowRight, CpuChip01, ShieldTick, Stars01 } from "@untitledui/icons";
import { Button } from "@/components/base/buttons/button";
import { Card } from "@/components/relay/ui";
import { type Settings, api } from "@/lib/api";
import { useCloud } from "@/lib/cloud";
import { CloudChecks } from "./cloud-setup";
import { LocalAiChecks, SelfTest } from "./setup-checks";

const Group = ({ icon: Icon, title, description, children }: { icon: typeof Stars01; title: string; description: string; children: ReactNode }) => (
    <section className="flex flex-col gap-3 animate-in fade-in slide-in-from-bottom-2 duration-500 fill-mode-both">
        <div className="flex items-center gap-3">
            <div className="flex size-9 shrink-0 items-center justify-center rounded-xl bg-brand-primary">
                <Icon className="size-5 text-fg-brand-primary" />
            </div>
            <div className="min-w-0">
                <h2 className="text-md font-semibold text-primary">{title}</h2>
                <p className="text-sm text-tertiary">{description}</p>
            </div>
        </div>
        <Card className="overflow-hidden p-0">{children}</Card>
    </section>
);

export const Wizard = ({ onFinish }: { settings: Settings; save: (s: Settings) => Promise<void>; onFinish: () => void }) => {
    const { signedIn, check } = useCloud();
    const ready = signedIn && check.status === "ok";
    const finish = async () => {
        await api.setupComplete();
        onFinish();
    };
    // Nothing to click: once Cloud AI is connected Relay opens the chat by itself, and
    // downloads keep going in the background.
    useEffect(() => {
        if (!ready) return;
        const t = setTimeout(finish, 2500);
        return () => clearTimeout(t);
    }, [ready]);

    return (
        <div className="flex h-full min-h-0 flex-col bg-primary">
            <main className="min-h-0 flex-1 overflow-y-auto">
                <div className="mx-auto flex w-full max-w-3xl flex-col gap-10 px-4 py-10 sm:px-8 sm:py-16">
                    <header className="flex flex-col gap-2 animate-in fade-in duration-500">
                        <span className="text-sm font-medium text-brand-secondary">Set up Relay</span>
                        <h1 className="font-serif text-display-sm font-normal tracking-tight text-primary sm:text-display-md">Let's get Relay ready</h1>
                        <p className="max-w-xl text-md text-tertiary">
                            Relay checks everything on its own. You only need to step in where a button shows up, and you can change any of it later in
                            Settings.
                        </p>
                    </header>

                    <Group icon={Stars01} title="Cloud AI" description="Relay thinks with Cloud AI on your own subscription.">
                        <CloudChecks autoSignIn />
                    </Group>
                    <Group icon={CpuChip01} title="Local AI" description="A vision model on your GPU helps Relay find things on screen, and runs tasks on its own when Cloud AI can't.">
                        <LocalAiChecks autoDownload />
                    </Group>
                    <Group icon={ShieldTick} title="Self-test" description="The rest of Relay, checked on this PC.">
                        <SelfTest />
                    </Group>
                </div>
            </main>
            <footer className="border-t border-secondary bg-secondary">
                <div className="mx-auto flex w-full max-w-3xl flex-wrap items-center justify-between gap-3 px-4 py-4 sm:px-8">
                    <p className="text-sm text-tertiary">
                        {ready
                            ? "Cloud AI is connected. Opening Relay; downloads keep going in the background."
                            : "Relay works without Cloud AI: the local AI does screen tasks. Sign in any time for writing and judgment."}
                    </p>
                    <Button iconTrailing={ArrowRight} onClick={finish}>
                        Start using Relay
                    </Button>
                </div>
            </footer>
        </div>
    );
};
