// Claude Code install, sign-in and check (D-045). Used by the wizard and Settings > Claude.
import { useEffect, useRef, useState } from "react";
import { Download01, LogIn01, RefreshCw01 } from "@untitledui/icons";
import { Button } from "@/components/base/buttons/button";
import { CheckRow, type CheckStatus, JobProgress, LogLines, Notice } from "@/components/relay/ui";
import { api, errorText } from "@/lib/api";
import { useClaude } from "@/lib/claude";
import { clearJob, useJobs, waitForJob } from "@/lib/jobs";
import { SignIn } from "./sign-in";

/**
 * The three Claude checks as checklist rows: installed, signed in, answers.
 * They run on their own; a button only shows where something needs the owner.
 * With `autoSignIn`, the sign-in starts by itself when it is the next thing to do.
 */
export const ClaudeChecks = ({ autoSignIn = false }: { autoSignIn?: boolean }) => {
    const { state, loading, check, signedIn, refresh, runCheck } = useClaude({ autoCheck: true });
    const jobs = useJobs();
    const [signingIn, setSigningIn] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const autoStarted = useRef(false);

    useEffect(() => {
        if (autoSignIn && !autoStarted.current && state?.installed && !state.auth.loggedIn) {
            autoStarted.current = true;
            setSigningIn(true);
        }
    }, [autoSignIn, state]);

    const runJob = async (job: "claude-install" | "claude-update", start: () => Promise<void>) => {
        setError(null);
        clearJob(job);
        try {
            await start();
            await waitForJob(job);
        } catch (e) {
            setError(errorText(e));
        }
        await refresh();
    };

    const installJob = jobs["claude-install"];
    const updateJob = jobs["claude-update"];
    const installing = installJob?.state === "running";
    const updating = updateJob?.state === "running";

    const installed: CheckStatus = !state ? "running" : installing ? "running" : state.installed ? "ok" : "error";
    const auth: CheckStatus = !state?.installed ? "waiting" : signingIn ? "running" : state.auth.loggedIn ? "ok" : "warn";
    const answers: CheckStatus = !signedIn
        ? "waiting"
        : check.status === "running" || check.status === "idle"
          ? "running"
          : check.status === "ok"
            ? "ok"
            : "error";

    return (
        <>
            <CheckRow
                status={installed}
                title="Claude Code installed"
                detail={
                    !state
                        ? "Looking for Claude Code…"
                        : installing
                          ? "Installing with Anthropic's official installer…"
                          : state.installed
                            ? `Version ${state.version ?? "unknown"}`
                            : "Not found on this PC. Relay installs it with Anthropic's official installer."
                }
                action={
                    state &&
                    (!state.installed ? (
                        <Button size="sm" iconLeading={Download01} isLoading={installing} showTextWhileLoading onClick={() => runJob("claude-install", api.claudeInstall)}>
                            Install Claude Code
                        </Button>
                    ) : (
                        <Button size="sm" color="secondary" iconLeading={RefreshCw01} isLoading={updating} showTextWhileLoading onClick={() => runJob("claude-update", api.claudeUpdate)}>
                            Update
                        </Button>
                    ))
                }
            >
                {(installJob?.state === "running" || installJob?.state === "error") && (
                    <div className="flex flex-col gap-2">
                        <JobProgress job={installJob} />
                        <LogLines lines={installJob.lines} />
                    </div>
                )}
                {updateJob && updateJob.state !== "done" && <LogLines lines={updateJob.lines} />}
                {error && <Notice tone="error">{error}</Notice>}
            </CheckRow>
            <CheckRow
                status={auth}
                title="Signed in to Claude"
                detail={
                    !state?.installed
                        ? "After Claude Code is installed."
                        : state.auth.loggedIn
                          ? [state.auth.email, state.auth.subscriptionType && `${state.auth.subscriptionType} plan`].filter(Boolean).join(" · ") || "Signed in"
                          : signingIn
                            ? "Waiting for you in your browser…"
                            : "Use your Claude subscription. Relay never sees your password or token."
                }
                action={
                    state?.installed &&
                    !signingIn && (
                        <Button size="sm" color={state.auth.loggedIn ? "secondary" : "primary"} iconLeading={LogIn01} onClick={() => setSigningIn(true)}>
                            {state.auth.loggedIn ? "Switch account" : "Sign in"}
                        </Button>
                    )
                }
            >
                {signingIn && <SignIn onDone={() => setSigningIn(false)} onCancel={() => setSigningIn(false)} />}
            </CheckRow>
            <CheckRow
                status={answers}
                title="Claude answers"
                detail={
                    !signedIn
                        ? "After you sign in."
                        : check.status === "ok"
                          ? `Claude replied "${check.text}".`
                          : check.status === "error"
                            ? check.text
                            : "Sending Claude a one-word test message…"
                }
                action={
                    signedIn &&
                    (check.status === "ok" || check.status === "error") && (
                        <Button size="sm" color="secondary" iconLeading={RefreshCw01} onClick={runCheck} isDisabled={loading}>
                            Check again
                        </Button>
                    )
                }
            />
        </>
    );
};
