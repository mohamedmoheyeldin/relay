// Signing in to Cloud AI without a terminal (D-045). Relay runs `cloud-cli auth login`
// out of sight, opens the sign-in page, and watches `cloud-cli auth status` until the
// owner is signed in. Relay never sees the token; Cloud AI keeps it.
import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ChevronDown, LinkExternal01, LogIn01 } from "@untitledui/icons";
import { Button } from "@/components/base/buttons/button";
import { Input } from "@/components/base/input/input";
import { FeaturedIcon } from "@/components/foundations/featured-icon/featured-icon";
import { Card, Notice, Spinner } from "@/components/relay/ui";
import { api, errorText } from "@/lib/api";
import { refreshCloud } from "@/lib/cloud";
import { cx } from "@/utils/cx";

type Kind = "login" | "login-console";

// Colour, cursor and hyperlink escape codes from the terminal.
// eslint-disable-next-line no-control-regex
const ANSI = /\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b\[[0-?]*[ -/]*[@-~]|\x1b[@-Z\\-_]/g;

export const plainText = (raw: string) => raw.replace(ANSI, "").replace(/\r(?!\n)/g, "\n");

/** The sign-in link Cloud AI prints, if it has printed one. */
export const findSignInUrl = (text: string): string | null => {
    const urls = text.match(/https:\/\/[^\s"'<>]+/g) ?? [];
    return urls.find((u) => /oauth|authorize|login/i.test(u)) ?? urls[0] ?? null;
};

export const SignIn = ({ onDone, onCancel }: { onDone: () => void; onCancel: () => void }) => {
    const [kind, setKind] = useState<Kind>("login");
    const [attempt, setAttempt] = useState(0);
    const [output, setOutput] = useState("");
    const [failed, setFailed] = useState<string | null>(null);
    const [code, setCode] = useState("");
    const [details, setDetails] = useState(false);
    const idRef = useRef<number | null>(null);
    const doneRef = useRef(onDone);
    doneRef.current = onDone;

    useEffect(() => {
        let raw = "";
        let cancelled = false;
        let finished = false;
        const decoder = new TextDecoder();
        setOutput("");
        setFailed(null);
        idRef.current = null;

        // When Cloud AI already counts as signed in (switching account, or a session
        // that expired but is still on disk), polling can't tell a new sign-in apart,
        // so only the login finishing counts.
        let wasIn: boolean | null = null;
        refreshCloud()
            .then((s) => (wasIn = s.auth.loggedIn))
            .catch(() => (wasIn = false));
        const finish = async (exited = false) => {
            if (finished) return true;
            if (!exited && wasIn !== false) return false;
            const state = await refreshCloud().catch(() => null);
            if (!state?.auth.loggedIn || cancelled) return false;
            finished = true;
            if (idRef.current !== null) api.ptyKill(idRef.current).catch(() => {});
            doneRef.current();
            return true;
        };

        // Output can arrive before ptyStart has told us the terminal's id, so it is
        // held per id until then instead of being dropped.
        const early = new Map<number, number[][]>();
        const exited = new Set<number>();
        const take = (data: number[]) => {
            raw += decoder.decode(new Uint8Array(data), { stream: true });
            setOutput(plainText(raw));
        };
        const onExit = async () => {
            if (cancelled) return;
            if (!(await finish(true)) && !cancelled) setFailed("The sign-in stopped before you were signed in.");
        };
        const subs = [
            listen<{ id: number; data: number[] }>("pty-data", ({ payload }) => {
                if (idRef.current === null) early.set(payload.id, [...(early.get(payload.id) ?? []), payload.data]);
                else if (payload.id === idRef.current) take(payload.data);
            }),
            listen<number>("pty-exit", ({ payload }) => {
                if (idRef.current === null) exited.add(payload);
                else if (payload === idRef.current) onExit();
            }),
        ];
        // Wide enough that the long sign-in link is never wrapped across lines.
        Promise.all(subs)
            .then(() => api.ptyStart(kind, 2000, 40))
            .then((id) => {
                if (cancelled) return void api.ptyKill(id).catch(() => {});
                idRef.current = id;
                early.get(id)?.forEach(take);
                early.clear();
                if (exited.has(id)) onExit();
            })
            .catch((e) => !cancelled && setFailed(errorText(e)));
        const poll = window.setInterval(() => finish(), 2000);

        return () => {
            cancelled = true;
            window.clearInterval(poll);
            subs.forEach((p) => p.then((f) => f()));
            if (idRef.current !== null) api.ptyKill(idRef.current).catch(() => {});
        };
    }, [kind, attempt]);

    const url = findSignInUrl(output);
    const send = (text: string) => api.ptyWrite(text + "\r").catch((e) => setFailed(errorText(e)));

    return (
        <Card className="flex flex-col gap-5 p-5 animate-in fade-in slide-in-from-bottom-1 duration-300">
            <div className="flex items-start gap-4">
                <FeaturedIcon icon={LogIn01} color="brand" theme="light" size="md" />
                <div className="min-w-0 flex-1">
                    <h3 className="text-md font-semibold text-primary">
                        {failed ? "Sign-in didn't finish" : url ? "Finish signing in in your browser" : "Opening the sign-in page…"}
                    </h3>
                    <p className="mt-1 text-sm text-tertiary">
                        {kind === "login"
                            ? "Sign in with your Cloud AI account (Pro or Max). Relay notices on its own when you're done."
                            : "Sign in with your developer console account. Relay notices on its own when you're done."}
                    </p>
                </div>
            </div>

            {failed ? (
                <Notice tone="error">{failed}</Notice>
            ) : (
                <div className="flex items-center gap-3 rounded-lg bg-secondary px-4 py-3 text-sm text-secondary">
                    <Spinner />
                    Waiting for you to sign in…
                </div>
            )}

            <div className="flex flex-wrap gap-3">
                {failed ? (
                    <Button size="sm" onClick={() => setAttempt(attempt + 1)}>
                        Try again
                    </Button>
                ) : (
                    url && (
                        <Button size="sm" color="secondary" iconTrailing={LinkExternal01} onClick={() => api.openPath(url)}>
                            Open the sign-in page again
                        </Button>
                    )
                )}
                <Button size="sm" color="link-gray" onClick={onCancel}>
                    Cancel
                </Button>
            </div>

            {!failed && (
                <form
                    className="flex flex-col gap-2 border-t border-secondary pt-4"
                    onSubmit={(e) => {
                        e.preventDefault();
                        if (code.trim()) send(code.trim()).then(() => setCode(""));
                    }}
                >
                    <span className="text-sm font-medium text-secondary">Did the page give you a code?</span>
                    <div className="flex gap-2">
                        <div className="min-w-0 flex-1">
                            <Input aria-label="Sign-in code" placeholder="Paste the code here" size="sm" value={code} onChange={setCode} />
                        </div>
                        <Button size="sm" color="secondary" type="submit" isDisabled={!code.trim()}>
                            Submit
                        </Button>
                    </div>
                </form>
            )}

            <div className="flex flex-wrap items-center justify-between gap-2 text-sm">
                <button
                    className="cursor-pointer font-semibold text-tertiary transition hover:text-tertiary_hover"
                    onClick={() => setKind(kind === "login" ? "login-console" : "login")}
                >
                    {kind === "login" ? "Use a developer console account instead" : "Use a Cloud AI account instead"}
                </button>
                <button
                    className="flex cursor-pointer items-center gap-1 font-semibold text-tertiary transition hover:text-tertiary_hover"
                    onClick={() => setDetails(!details)}
                    aria-expanded={details}
                >
                    Details
                    <ChevronDown className={cx("size-4 transition-transform duration-200", details && "rotate-180")} />
                </button>
            </div>
            {details && (
                <div className="flex flex-col gap-2 animate-in fade-in duration-200">
                    <pre className="max-h-48 overflow-auto rounded-lg bg-secondary p-3 font-mono text-xs whitespace-pre-wrap break-all text-secondary">
                        {output.trim() || "Nothing yet."}
                    </pre>
                    <div>
                        <Button size="sm" color="secondary" onClick={() => send("")}>
                            Press Enter
                        </Button>
                    </div>
                </div>
            )}
        </Card>
    );
};
