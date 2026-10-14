// Claude Code install, sign-in and check (D-045). Used by the wizard and Settings > Claude.
import { useCallback, useEffect, useState } from "react";
import { CheckCircle, Download01, RefreshCw01, Terminal as TerminalIcon } from "@untitledui/icons";
import { Badge } from "@/components/base/badges/badges";
import { Button } from "@/components/base/buttons/button";
import { Terminal } from "@/components/relay/terminal";
import { Card, JobProgress, KeyValue, LogLines, Notice } from "@/components/relay/ui";
import { type ClaudeState, api, errorText } from "@/lib/api";
import { clearJob, useJobs, waitForJob } from "@/lib/jobs";

export function useClaudeState() {
    const [state, setState] = useState<ClaudeState | null>(null);
    const refresh = useCallback(async () => {
        setState(await api.claudeState());
    }, []);
    useEffect(() => {
        refresh();
    }, [refresh]);
    return { state, refresh };
}

export const ClaudeSetup = ({ onReady }: { onReady?: (ready: boolean) => void }) => {
    const { state, refresh } = useClaudeState();
    const jobs = useJobs();
    const [signingIn, setSigningIn] = useState(false);
    const [verify, setVerify] = useState<{ ok: boolean; text: string } | null>(null);
    const [verifying, setVerifying] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const ready = !!state?.installed && !!state.auth.loggedIn;
    useEffect(() => onReady?.(ready && verify?.ok !== false), [ready, verify, onReady]);

    const install = async () => {
        setError(null);
        clearJob("claude-install");
        try {
            await api.claudeInstall();
            await waitForJob("claude-install");
        } catch (e) {
            setError(errorText(e));
        }
        refresh();
    };

    const update = async () => {
        setError(null);
        clearJob("claude-update");
        try {
            await api.claudeUpdate();
            await waitForJob("claude-update");
        } catch (e) {
            setError(errorText(e));
        }
        refresh();
    };

    const runVerify = async () => {
        setVerifying(true);
        setVerify(null);
        try {
            const text = await api.claudeVerify();
            setVerify({ ok: true, text });
        } catch (e) {
            setVerify({ ok: false, text: errorText(e) });
        }
        setVerifying(false);
    };

    if (!state) return <Card>Checking for Claude Code…</Card>;
    const installJob = jobs["claude-install"];
    const updateJob = jobs["claude-update"];

    return (
        <div className="flex flex-col gap-4">
            <Card className="flex flex-col gap-3">
                <div className="flex items-center justify-between">
                    <span className="text-md font-semibold text-primary">Claude Code</span>
                    {!state.installed ? (
                        <Badge color="warning">Not installed</Badge>
                    ) : state.auth.loggedIn ? (
                        <Badge color="success">Signed in</Badge>
                    ) : (
                        <Badge color="warning">Not signed in</Badge>
                    )}
                </div>
                {state.installed && (
                    <>
                        <KeyValue label="Version" value={state.version ?? "unknown"} />
                        {state.auth.loggedIn && (
                            <>
                                <KeyValue label="Account" value={state.auth.email ?? "signed in"} />
                                {state.auth.subscriptionType && <KeyValue label="Plan" value={state.auth.subscriptionType} />}
                            </>
                        )}
                    </>
                )}
                {!state.installed && (
                    <p className="text-sm text-tertiary">
                        Relay runs Anthropic's official Claude Code installer for you. Nothing else is needed, and Relay never sees your
                        Claude password or token.
                    </p>
                )}
                <div className="flex flex-wrap gap-3">
                    {!state.installed && (
                        <Button iconLeading={Download01} onClick={install} isLoading={installJob?.state === "running"} showTextWhileLoading>
                            Install Claude Code
                        </Button>
                    )}
                    {state.installed && (
                        <Button
                            color={state.auth.loggedIn ? "secondary" : "primary"}
                            iconLeading={TerminalIcon}
                            onClick={() => setSigningIn(true)}
                            isDisabled={signingIn}
                        >
                            {state.auth.loggedIn ? "Sign in again" : "Sign in"}
                        </Button>
                    )}
                    {state.installed && (
                        <Button color="secondary" iconLeading={RefreshCw01} onClick={update} isLoading={updateJob?.state === "running"} showTextWhileLoading>
                            Update
                        </Button>
                    )}
                    {state.auth.loggedIn && (
                        <Button color="secondary" iconLeading={CheckCircle} onClick={runVerify} isLoading={verifying} showTextWhileLoading>
                            Check it works
                        </Button>
                    )}
                </div>
                <JobProgress job={installJob} />
                {installJob && <LogLines lines={installJob.lines} />}
                {updateJob && <LogLines lines={updateJob.lines} />}
                {error && <Notice tone="error">{error}</Notice>}
                {verify && (
                    <Notice tone={verify.ok ? "success" : "error"}>
                        {verify.ok ? `Claude answered: ${verify.text || "OK"}` : verify.text}
                    </Notice>
                )}
            </Card>
            {signingIn && (
                <Card className="flex flex-col gap-3">
                    <div className="flex items-center justify-between">
                        <span className="text-sm font-semibold text-secondary">Sign in to Claude</span>
                        <Button
                            size="sm"
                            color="secondary"
                            onClick={() => {
                                setSigningIn(false);
                                refresh();
                            }}
                        >
                            Done
                        </Button>
                    </div>
                    <p className="text-sm text-tertiary">
                        Claude Code opens your browser to sign in with your Claude account. If it asks for a code, paste it here and press Enter.
                    </p>
                    <Terminal
                        kind="login"
                        onExit={() => {
                            setSigningIn(false);
                            refresh();
                        }}
                        onError={(m) => {
                            setError(m);
                            setSigningIn(false);
                        }}
                    />
                </Card>
            )}
        </div>
    );
};
