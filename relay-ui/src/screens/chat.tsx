// The chat window: one conversation with Claude, with Relay's activity shown inline.
import { type ReactNode, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ArrowUp, CheckCircle, ChevronRight, CpuChip01, Stars02, StopSquare, Tool01, XCircle } from "@untitledui/icons";
import { TextAreaBase } from "@/components/base/textarea/textarea";
import { Markdown } from "@/components/relay/markdown";
import { Notice, Spinner } from "@/components/relay/ui";
import { type ChatEvent, api, errorText } from "@/lib/api";
import { describeTool, isHiddenTool } from "@/lib/describe-tool";
import { isAuthError, refreshClaude, useClaude } from "@/lib/claude";
import { SignIn } from "./sign-in";
import { cx } from "@/utils/cx";

type Item =
    | { kind: "user"; text: string }
    | { kind: "assistant"; text: string; streaming?: boolean }
    | { kind: "tool"; id: string; name: string; input: string; result?: { ok: boolean; text: string; image: string | null } }
    | { kind: "done"; ok: boolean; text: string; seconds: number | null }
    | { kind: "error"; text: string }
    | { kind: "fallback"; reason: string }
    | { kind: "note"; text: string }
    | { kind: "step"; step: number; thought: string; action: string; result: string };

const SUGGESTIONS = ["Summarise my unread email", "Open Notepad and write a shopping list", "Find a cheap flight to Cairo next month"];

const toolLabel = (name: string) => {
    const short = name.replace(/^mcp__/, "").replace(/__/g, " · ");
    return short.replace(/_/g, " ");
};

const pretty = (input: string) => {
    try {
        return JSON.stringify(JSON.parse(input), null, 2);
    } catch {
        return input;
    }
};

// A quiet, one-line activity row (a tool call or a local-AI step) that opens to show details.
const ActivityRow = ({
    icon,
    title,
    summary,
    mono,
    expandable,
    open,
    onToggle,
    children,
}: {
    icon: ReactNode;
    title: ReactNode;
    summary?: ReactNode;
    mono?: boolean;
    expandable: boolean;
    open: boolean;
    onToggle: () => void;
    children?: ReactNode;
}) => (
    <div className="-mx-2 -my-1.5 text-sm animate-in fade-in slide-in-from-bottom-1 duration-300">
        <button
            className={cx(
                "group flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left transition duration-150 outline-focus-ring focus-visible:outline-2",
                expandable ? "cursor-pointer hover:bg-tertiary/60" : "cursor-default",
            )}
            onClick={onToggle}
            aria-expanded={expandable ? open : undefined}
        >
            {icon}
            <span className="shrink-0 font-medium text-secondary">{title}</span>
            {summary && <span className={cx("min-w-0 truncate text-tertiary", mono && "font-mono text-xs")}>{summary}</span>}
            {expandable && (
                <ChevronRight
                    className={cx(
                        "ml-auto size-4 shrink-0 text-fg-quaternary opacity-0 transition duration-200 group-hover:opacity-100",
                        open && "rotate-90 opacity-100",
                    )}
                />
            )}
        </button>
        {children}
    </div>
);

const ToolItem = ({ item }: { item: Extract<Item, { kind: "tool" }> }) => {
    const [open, setOpen] = useState(false);
    return (
        <ActivityRow
            icon={
                item.result ? (
                    item.result.ok ? (
                        <CheckCircle className="size-4 shrink-0 text-fg-quaternary" />
                    ) : (
                        <XCircle className="size-4 shrink-0 text-fg-error-primary" />
                    )
                ) : (
                    <Tool01 className="size-4 shrink-0 animate-pulse text-fg-brand-primary" />
                )
            }
            title={<span className="font-normal">{describeTool(item.name, item.input, !!item.result)}</span>}
            expandable
            open={open}
            onToggle={() => setOpen(!open)}
        >
            {open && (
                <div className="mt-1 mb-1 ml-4 flex flex-col gap-2 border-l border-secondary pl-4 animate-in fade-in slide-in-from-top-1 duration-200">
                    <span className="text-xs text-quaternary">{toolLabel(item.name)}</span>
                    <pre className="max-h-48 overflow-auto font-mono text-xs whitespace-pre-wrap text-tertiary">{pretty(item.input)}</pre>
                    {item.result?.text && (
                        <pre className="max-h-48 overflow-auto font-mono text-xs whitespace-pre-wrap text-quaternary">{item.result.text}</pre>
                    )}
                </div>
            )}
            {item.result?.image && (
                <img src={item.result.image} alt="Screenshot from Relay" className="mt-1 mb-2 ml-2 max-h-72 rounded-xl ring-1 ring-secondary animate-in fade-in duration-300" />
            )}
        </ActivityRow>
    );
};

const LocalStep = ({ item }: { item: Extract<Item, { kind: "step" }> }) => {
    const failed = /^(error|failed|denied|blocked|not run)/i.test(item.result);
    const [open, setOpen] = useState(false);
    return (
        <ActivityRow
            icon={failed ? <XCircle className="size-4 shrink-0 text-fg-error-primary" /> : <CheckCircle className="size-4 shrink-0 text-fg-quaternary" />}
            title={`Step ${item.step}`}
            // The step in plain words; the raw action only when there are none.
            summary={item.thought || item.action}
            mono={!item.thought}
            expandable={!!(item.thought ? item.action || item.result : item.result)}
            open={open}
            onToggle={() => setOpen(!open)}
        >
            {open && (
                <div className="mt-1 mb-1 ml-4 flex flex-col gap-1 border-l border-secondary pl-4 animate-in fade-in slide-in-from-top-1 duration-200">
                    {item.thought && item.action && <p className="font-mono text-xs text-tertiary">{item.action}</p>}
                    {item.result && <p className="text-xs break-words text-quaternary">{item.result}</p>}
                </div>
            )}
        </ActivityRow>
    );
};

const LOCAL_MODEL_MISSING = /Settings > Local AI/;

const APPROVAL_NOTE: Record<string, string> = {
    risky: "Relay asks here only before steps that can't be undone.",
    ask: "Relay asks here before every click or keystroke.",
    off: "Relay acts without asking.",
};

/** Which conversation the screen shows; `fresh` ones have nothing saved yet. */
export type OpenChat = { id: string; fresh: boolean };

/** A chat's title: the start of its first message. */
const titleOf = (items: Item[]) => {
    const first = items.find((it) => it.kind === "user");
    const line = first?.kind === "user" ? first.text.replace(/\s+/g, " ").trim() : "";
    return line.length > 60 ? `${line.slice(0, 59)}…` : line || "New chat";
};

/** What gets saved: screenshots are left out so chat files stay small. */
const forSaving = (items: Item[]) =>
    items.map((it) => {
        if (it.kind === "tool" && it.result?.image) return { ...it, result: { ...it.result, image: null } };
        if (it.kind === "assistant" && it.streaming) return { kind: "assistant", text: it.text };
        return it;
    });

export const ChatScreen = ({
    ready,
    onSetup,
    onLocalAi,
    chat,
    onSaved,
    approvalMode = "risky",
}: {
    ready: boolean;
    /** Settings > Safety: what the line under the composer promises. */
    approvalMode?: string;
    onSetup: () => void;
    onLocalAi: () => void;
    chat: OpenChat;
    /** A chat was saved, so the sidebar list may have changed. */
    onSaved: () => void;
}) => {
    const [items, setItems] = useState<Item[]>([]);
    const [loadError, setLoadError] = useState("");
    const [input, setInput] = useState("");
    const [busy, setBusy] = useState(false);
    const [signingIn, setSigningIn] = useState(false);
    // Auth failures at or before this index were fixed by signing in again.
    const [signedInAt, setSignedInAt] = useState(-1);
    const bottom = useRef<HTMLDivElement>(null);
    const claude = useClaude();

    let lastAuthFailure = -1;
    items.forEach((it, i) => {
        if ((it.kind === "error" || (it.kind === "done" && !it.ok)) && isAuthError(it.text)) lastAuthFailure = i;
    });
    const signedOut = !!claude.state?.installed && !claude.state.auth.loggedIn;
    // Signed out is fine (the note at the top says so); an expired sign-in Claude just hit is worth a button.
    const needsSignIn = ready && !signedOut && lastAuthFailure > signedInAt;

    useEffect(() => {
        const un = listen<{ event: ChatEvent | null; finished: boolean }>("chat", ({ payload }) => {
            if (payload.finished) {
                setBusy(false);
                return;
            }
            const e = payload.event;
            if (!e) return;
            setItems((prev) => {
                switch (e.type) {
                    case "text_delta": {
                        const last = prev[prev.length - 1];
                        if (last?.kind === "assistant" && last.streaming)
                            return [...prev.slice(0, -1), { ...last, text: last.text + e.text }];
                        return [...prev, { kind: "assistant", text: e.text, streaming: true }];
                    }
                    case "text": {
                        // The finished text replaces what streamed in.
                        const last = prev[prev.length - 1];
                        const rest = last?.kind === "assistant" && last.streaming ? prev.slice(0, -1) : prev;
                        return e.text.trim() ? [...rest, { kind: "assistant", text: e.text }] : rest;
                    }
                    case "tool_use":
                        return [...prev, { kind: "tool", id: e.id, name: e.name, input: e.input }];
                    case "tool_result":
                        return prev.map((it) =>
                            it.kind === "tool" && it.id === e.tool_use_id ? { ...it, result: { ok: !e.is_error, text: e.text, image: e.image } } : it,
                        );
                    case "done":
                        if (e.is_error && isAuthError(e.result)) refreshClaude().catch(() => {});
                        return [...prev, { kind: "done", ok: !e.is_error, text: e.is_error ? e.result : "", seconds: e.duration_ms ? e.duration_ms / 1000 : null }];
                    case "error":
                        return [...prev, { kind: "error", text: e.message }];
                    case "fallback":
                        refreshClaude().catch(() => {});
                        return [...prev, { kind: "fallback", reason: e.reason }];
                    case "local_note":
                        return [...prev, { kind: "note", text: e.text }];
                    case "local_step":
                        return [...prev, { kind: "step", step: e.step, thought: e.thought, action: e.action, result: e.result }];
                    default:
                        return prev;
                }
            });
        });
        return () => {
            un.then((f) => f());
        };
    }, []);

    useEffect(() => {
        bottom.current?.scrollIntoView({ behavior: "smooth" });
    }, [items]);

    // Stop is instant in the chat; the run itself is told to stop in the background.
    const stop = () => {
        setBusy(false);
        setItems((p) => [...p, { kind: "note", text: "Stopping…" }]);
        api.chatStop().catch(() => {});
    };

    const send = async () => {
        const text = input.trim();
        if (!text || busy) return;
        setItems((p) => [...p, { kind: "user", text }]);
        setInput("");
        setBusy(true);
        try {
            await api.chatSend(text);
        } catch (e) {
            setItems((p) => [...p, { kind: "error", text: errorText(e) }]);
            setBusy(false);
        }
    };

    // Switching chats: a running task stops, then the chosen chat (or a fresh one) loads.
    // `shown` is the chat whose items are on screen, so saves never go to the wrong file.
    const shown = useRef<string | null>(null);
    const busyRef = useRef(busy);
    busyRef.current = busy;
    useEffect(() => {
        let cancelled = false;
        shown.current = null;
        setLoadError("");
        (async () => {
            if (busyRef.current) await api.chatStop().catch(() => {});
            setBusy(false);
            if (chat.fresh) {
                await api.chatNew();
                if (cancelled) return;
                setItems([]);
            } else {
                try {
                    const saved = await api.chatOpen(chat.id);
                    if (cancelled) return;
                    setItems(Array.isArray(saved.items) ? (saved.items as Item[]) : []);
                } catch (e) {
                    if (cancelled) return;
                    await api.chatNew();
                    setItems([]);
                    setLoadError(errorText(e));
                }
            }
            shown.current = chat.id;
        })();
        return () => {
            cancelled = true;
        };
    }, [chat.id]);

    // Save shortly after each change, and right away when the chat changes or closes.
    useEffect(() => {
        const id = shown.current;
        if (!id || !items.some((it) => it.kind === "user")) return;
        const save = () => {
            api.chatSave(id, titleOf(items), forSaving(items))
                .then(onSaved)
                .catch(() => {});
        };
        const timer = setTimeout(save, busy ? 1500 : 300);
        return () => {
            clearTimeout(timer);
            if (shown.current !== id) save();
        };
    }, [items, busy]);

    const empty = items.length === 0;
    const composer = (
        <div className="rounded-3xl bg-primary_alt p-2.5 shadow-[0_0.25rem_1.25rem_rgb(0_0_0/0.035)] ring-1 ring-secondary transition duration-200 focus-within:shadow-[0_0.25rem_1.5rem_rgb(0_0_0/0.07)] focus-within:ring-primary dark:shadow-[0_0.25rem_1.25rem_rgb(0_0_0/0.25)]">
            <TextAreaBase
                aria-label="Message"
                placeholder={empty ? "How can Relay help today?" : "Reply to Relay…"}
                rows={empty ? 2 : 1}
                value={input}
                onChange={(e) => setInput(e.target.value)}
                onKeyDown={(e) => {
                    if (e.key === "Enter" && !e.shiftKey) {
                        e.preventDefault();
                        send();
                    }
                }}
                className="max-h-60 min-h-11 resize-none rounded-none bg-transparent px-2.5 pt-2 pb-1 text-md shadow-none ring-0 focus:ring-0"
            />
            <div className="flex items-center justify-between gap-3 pl-2.5">
                <span className="truncate text-xs text-quaternary">{APPROVAL_NOTE[approvalMode] ?? APPROVAL_NOTE.risky}</span>
                {busy ? (
                    <button
                        aria-label="Stop"
                        title="Stop"
                        onClick={stop}
                        className="flex size-9 shrink-0 cursor-pointer items-center justify-center rounded-full bg-primary-solid text-white transition duration-150 outline-focus-ring animate-in fade-in zoom-in-75 hover:opacity-85 focus-visible:outline-2 focus-visible:outline-offset-2 dark:bg-neutral-100 dark:text-neutral-900"
                    >
                        <StopSquare className="size-4" />
                    </button>
                ) : (
                    <button
                        aria-label="Send"
                        title="Send"
                        onClick={send}
                        disabled={!input.trim()}
                        className="flex size-9 shrink-0 cursor-pointer items-center justify-center rounded-full bg-brand-solid text-white transition duration-150 outline-focus-ring animate-in fade-in zoom-in-75 hover:bg-brand-solid_hover focus-visible:outline-2 focus-visible:outline-offset-2 disabled:cursor-default disabled:opacity-40 disabled:hover:bg-brand-solid"
                    >
                        <ArrowUp className="size-[18px]" />
                    </button>
                )}
            </div>
        </div>
    );

    return (
        <div className="flex h-full min-h-0 flex-col">
            <div className="flex h-12 shrink-0 items-center gap-3 px-4 sm:px-6">
                <span className="truncate text-sm font-medium text-tertiary">{empty ? "" : titleOf(items)}</span>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-6 sm:px-6">
                <div className={cx("mx-auto flex w-full max-w-3xl flex-col gap-5", empty && "min-h-full justify-center pb-16")}>
                    {loadError && <Notice tone="error">{loadError}</Notice>}
                    {!ready && (
                        <p className="text-center text-xs text-tertiary">
                            Claude isn't signed in, so the local AI does screen tasks on its own. Writing and judgment need Claude:{" "}
                            <button className="cursor-pointer font-medium underline" onClick={onSetup}>
                                sign in
                            </button>
                            .
                        </p>
                    )}
                    {items.length === 0 && (
                        <div className="flex flex-col items-center text-center animate-in fade-in slide-in-from-bottom-3 duration-700">
                            <div className="flex items-center gap-3">
                                <Stars02 className="size-7 shrink-0 text-fg-brand-primary" />
                                <h2 className="font-serif text-display-sm font-normal tracking-tight text-primary sm:text-display-md">What should Relay do?</h2>
                            </div>
                            <p className="mt-3 max-w-lg text-md text-tertiary">Ask for a task on this PC, and Relay works it through on screen.</p>
                            <div className="mt-8 w-full text-left">{composer}</div>
                            <div className="mt-4 flex max-w-2xl flex-wrap justify-center gap-2">
                                {SUGGESTIONS.map((t, n) => (
                                    <button
                                        key={t}
                                        onClick={() => setInput(t)}
                                        style={{ animationDelay: `${150 + n * 60}ms` }}
                                        className="cursor-pointer rounded-xl bg-primary px-3 py-1.5 text-sm text-secondary ring-1 ring-secondary transition duration-150 ring-inset animate-in fade-in slide-in-from-bottom-1 fill-mode-both hover:bg-secondary hover:text-primary"
                                    >
                                        {t}
                                    </button>
                                ))}
                            </div>
                        </div>
                    )}
                    {items.map((it, i) => {
                        if (it.kind === "user")
                            return (
                                <div
                                    key={i}
                                    className="ml-auto max-w-[85%] rounded-2xl bg-tertiary px-4 py-2.5 text-md break-words whitespace-pre-wrap text-primary animate-in fade-in slide-in-from-bottom-2 duration-300 dark:bg-neutral-950"
                                >
                                    {it.text}
                                </div>
                            );
                        if (it.kind === "assistant" && isAuthError(it.text) && items[i + 1]?.kind === "done") return null;
                        if (it.kind === "assistant")
                            return (
                                <div key={i} className="font-serif text-[16.5px] leading-[1.7] break-words text-primary animate-in fade-in slide-in-from-bottom-1 duration-500">
                                    <Markdown text={it.text} />
                                </div>
                            );
                        if (it.kind === "tool")
                            return isHiddenTool(it.name) && it.result?.ok !== false ? null : <ToolItem key={i} item={it} />;
                        if (it.kind === "fallback")
                            return (
                                <div key={i} className="flex items-center gap-2 pt-1 text-sm font-medium text-brand-secondary animate-in fade-in duration-300">
                                    <CpuChip01 className="size-4 shrink-0" />
                                    Local AI (Claude unavailable)
                                    <span className="font-normal text-tertiary">· {it.reason}</span>
                                </div>
                            );
                        if (it.kind === "note")
                            return (
                                <div key={i} className="text-xs text-tertiary animate-in fade-in duration-300">
                                    {it.text}
                                </div>
                            );
                        if (it.kind === "step") return <LocalStep key={i} item={it} />;
                        if (it.kind === "done")
                            return (
                                <div key={i} className={cx("text-xs animate-in fade-in duration-300", it.ok ? "text-quaternary" : "text-error-primary")}>
                                    {it.ok ? `Done${it.seconds ? ` in ${it.seconds.toFixed(1)} s` : ""}` : it.text}
                                </div>
                            );
                        return (
                            <Notice key={i} tone="error">
                                {it.text}
                                {LOCAL_MODEL_MISSING.test(it.text) && (
                                    <>
                                        {" "}
                                        <button className="cursor-pointer font-semibold underline" onClick={onLocalAi}>
                                            Open Local AI
                                        </button>
                                    </>
                                )}
                            </Notice>
                        );
                    })}
                    {needsSignIn &&
                        !busy &&
                        (signingIn ? (
                            <SignIn
                                onDone={() => {
                                    setSigningIn(false);
                                    setSignedInAt(items.length);
                                }}
                                onCancel={() => setSigningIn(false)}
                            />
                        ) : (
                            <Notice tone="warning">
                                {signedOut ? "Claude Code isn't signed in." : "Your Claude sign-in has expired, so Claude can't answer."}{" "}
                                <button className="cursor-pointer font-semibold underline" onClick={() => setSigningIn(true)}>
                                    Sign in again
                                </button>
                                , then send your task again.
                            </Notice>
                        ))}
                    {busy && (
                        <div className="flex items-center gap-2.5 text-sm text-tertiary animate-in fade-in duration-500">
                            <Spinner />
                            <span className="animate-pulse">Working…</span>
                        </div>
                    )}
                    <div ref={bottom} />
                </div>
            </div>
            {!empty && (
                <div className="px-4 pt-1 pb-4 sm:px-6">
                    <div className="mx-auto max-w-3xl animate-in fade-in slide-in-from-bottom-2 duration-300">{composer}</div>
                </div>
            )}
        </div>
    );
};
