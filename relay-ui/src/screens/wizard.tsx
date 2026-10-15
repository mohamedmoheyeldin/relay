// First-run setup (D-044): one page of checks that run on their own. Buttons only
// appear where something needs the owner: installing Claude Code, signing in,
// downloading the suggested model.
import type { ReactNode } from "react";
import { ArrowRight, CpuChip01, Globe01, ShieldTick, Stars01 } from "@untitledui/icons";
import { Button } from "@/components/base/buttons/button";
import { Card } from "@/components/relay/ui";
import { type Settings, api } from "@/lib/api";
import { useClaude } from "@/lib/claude";
import { ClaudeChecks } from "./claude-setup";
import { BrowserCheck, LocalAiChecks, SelfTest } from "./setup-checks";

const Group = ({ icon: Icon, title, description, children }: { icon: typeof Stars01; title: string; description: string; children: ReactNode }) => (
    <section className="flex flex-col gap-3 animate-in fade-in slide-in-from-bottom-2 duration-500 fill-mode-both">
        <div className="flex items-center gap-3">
            <div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-primary shadow-xs ring-1 ring-secondary ring-inset">
                <Icon className="size-5 text-fg-secondary" />
            </div>
            <div className="min-w-0">
                <h2 className="text-md font-semibold text-primary">{title}</h2>
                <p className="text-sm text-tertiary">{description}</p>
            </div>
        </div>
        <Card className="overflow-hidden p-0">{children}</Card>
    </section>
);

export const Wizard = ({ settings, save, onFinish }: { settings: Settings; save: (s: Settings) => Promise<void>; onFinish: () => void }) => {
    const { signedIn, check } = useClaude();
    const ready = signedIn && check.status === "ok";
    const finish = async () => {
        await api.setupComplete();
        onFinish();
    };

    return (
        <div className="flex h-full min-h-0 flex-col bg-secondary_alt">
            <main className="min-h-0 flex-1 overflow-y-auto">
                <div className="mx-auto flex w-full max-w-3xl flex-col gap-8 px-4 py-8 sm:px-8 sm:py-12">
                    <header className="flex flex-col gap-2 animate-in fade-in duration-500">
                        <span className="text-sm font-semibold text-brand-secondary">Set up Relay</span>
                        <h1 className="text-display-xs font-semibold text-primary sm:text-display-sm">Let's get Relay ready</h1>
                        <p className="max-w-xl text-md text-tertiary">
                            Relay checks everything on its own. You only need to step in where a button shows up, and you can change any of it later in
                            Settings.
                        </p>
                    </header>

                    <Group icon={Stars01} title="Claude" description="Relay thinks with Claude Code and your own Claude subscription.">
                        <ClaudeChecks autoSignIn />
                    </Group>
                    <Group icon={CpuChip01} title="Local AI" description="A small vision model on your GPU helps Relay find things on screen quickly.">
                        <LocalAiChecks />
                    </Group>
                    <Group icon={Globe01} title="Browser" description="For web tasks Claude drives your browser through its page structure.">
                        <BrowserCheck settings={settings} save={save} />
                    </Group>
                    <Group icon={ShieldTick} title="Self-test" description="The rest of Relay, checked on this PC.">
                        <SelfTest />
                    </Group>
                </div>
            </main>
            <footer className="border-t border-secondary bg-primary">
                <div className="mx-auto flex w-full max-w-3xl flex-wrap items-center justify-between gap-3 px-4 py-4 sm:px-8">
                    <p className="text-sm text-tertiary">
                        {ready ? "Claude is connected. Downloads can keep going in the background." : "Relay needs Claude before it can do tasks."}
                    </p>
                    <div className="flex gap-3">
                        {!ready && (
                            <Button color="link-gray" onClick={finish}>
                                Finish later
                            </Button>
                        )}
                        <Button iconTrailing={ArrowRight} onClick={finish} isDisabled={!ready}>
                            Start using Relay
                        </Button>
                    </div>
                </div>
            </footer>
        </div>
    );
};
