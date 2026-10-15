// The chat window: one conversation with Claude, with Relay's activity shown inline.
import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { CheckCircle, CpuChip01, MessageChatCircle, Plus, Send01, StopCircle, Tool01, XCircle } from "@untitledui/icons";
import { Button } from "@/components/base/buttons/button";
import { TextAreaBase } from "@/components/base/textarea/textarea";
import { FeaturedIcon } from "@/components/foundations/featured-icon/featured-icon";
import { Markdown } from "@/components/relay/markdown";
import { Notice, Spinner } from "@/components/relay/ui";
import { type ChatEvent, api, errorText } from "@/lib/api";
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

const ToolItem = ({ item }: { item: Extract<Item, { kind: "tool" }> }) => {
    const [open, setOpen] = useState(false);
    return (
        <div className="rounded-lg bg-secondary px-3 py-2 text-sm animate-in fade-in duration-200">
            <button className="flex w-full cursor-pointer items-center gap-2 text-left" onClick={() => setOpen(!open)}>
                {item.result ? (
                    item.result.ok ? (
                        <CheckCircle className="size-4 shrink-0 text-fg-success-primary" />
                    ) : (
                        <XCircle className="size-4 shrink-0 text-fg-error-primary" />
                    )
                ) : (
                    <Tool01 className="size-4 shrink-0 animate-pulse text-fg-quaternary" />
                )}
                <span className="font-medium text-secondary">{toolLabel(item.name)}</span>
                <span className="truncate text-tertiary">{item.input}</span>
            </button>
            {open && item.result?.text && <pre className="mt-2 max-h-48 overflow-auto font-mono text-xs whitespace-pre-wrap text-tertiary">{item.result.text}</pre>}
            {item.result?.image && <img src={item.result.image} alt="Screenshot from Relay" className="mt-2 max-h-72 rounded-md ring-1 ring-secondary" />}
        </div>
    );
};

const LocalStep = ({ item }: { item: Extract<Item, { kind: "step" }> }) => {
    const failed = /^(error|failed|denied|blocked|not run)/i.test(item.result);
    return (
        <div className="rounded-lg bg-secondary px-3 py-2 text-sm animate-in fade-in duration-200">
            <div className="flex items-center gap-2">
                {failed ? <XCircle className="size-4 shrink-0 text-fg-error-primary" /> : <CheckCircle className="size-4 shrink-0 text-fg-success-primary" />}
                <span className="font-medium text-secondary">Step {item.step}</span>
                <span className="truncate font-mono text-xs text-tertiary">{item.action}</span>
            </div>
            {item.thought && <p className="mt-1 text-tertiary">{item.thought}</p>}
            {item.result && <p className="mt-1 truncate text-xs text-quaternary">{item.result}</p>}
        </div>
    );
};

const LOCAL_MODEL_MISSING = /Settings > Local AI/;

export const ChatScreen = ({ ready, onSetup, onLocalAi }: { ready: boolean; onSetup: () => void; onLocalAi: () => void }) => {
    const [items, setItems] = useState<Item[]>([]);
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
    const needsSignIn = ready && (signedOut || lastAuthFailure > signedInAt);

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

    const newChat = async () => {
        await api.chatNew();
        setItems([]);
    };

    return (
        <div className="flex h-full min-h-0 flex-col">
            <div className="flex items-center justify-between gap-3 border-b border-secondary px-4 py-3 sm:px-6">
                <span className="text-md font-semibold text-primary">Chat</span>
                <Button size="sm" color="secondary" iconLeading={Plus} onClick={newChat} isDisabled={busy}>
                    New chat
                </Button>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto px-4 py-6 sm:px-6">
                <div className="mx-auto flex max-w-3xl flex-col gap-4">
                    {!ready && (
                        <Notice>
                            Claude isn't signed in, so tasks run on the local AI alone.{" "}
                            <button className="cursor-pointer font-semibold underline" onClick={onSetup}>
                                Finish setup
                            </button>{" "}
                            to let Claude handle them.
                        </Notice>
                    )}
                    {items.length === 0 && (
                        <div className="flex flex-col items-center py-12 text-center animate-in fade-in slide-in-from-bottom-2 duration-500 sm:py-20">
                            <FeaturedIcon icon={MessageChatCircle} color="brand" theme="light" size="lg" />
                            <h2 className="mt-5 text-display-xs font-semibold text-primary">What should Relay do?</h2>
                            <p className="mt-2 max-w-md text-md text-tertiary">Ask for a task on this PC. Relay asks before it clicks or types.</p>
                            <div className="mt-6 flex max-w-xl flex-wrap justify-center gap-2">
                                {SUGGESTIONS.map((t) => (
                                    <button
                                        key={t}
                                        onClick={() => setInput(t)}
                                        className="cursor-pointer rounded-full bg-primary px-3.5 py-2 text-sm font-medium text-secondary shadow-xs ring-1 ring-primary transition duration-150 ring-inset hover:bg-primary_hover hover:text-secondary_hover"
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
                                    className="ml-auto max-w-[85%] rounded-2xl rounded-tr-sm bg-brand-solid px-4 py-2.5 text-md break-words whitespace-pre-wrap text-white animate-in fade-in slide-in-from-bottom-1 duration-200"
                                >
                                    {it.text}
                                </div>
                            );
                        if (it.kind === "assistant" && isAuthError(it.text) && items[i + 1]?.kind === "done") return null;
                        if (it.kind === "assistant")
                            return (
                                <div key={i} className="max-w-[92%] text-md break-words text-primary animate-in fade-in duration-300">
                                    <Markdown text={it.text} />
                                </div>
                            );
                        if (it.kind === "tool") return <ToolItem key={i} item={it} />;
                        if (it.kind === "fallback")
                            return (
                                <div key={i} className="flex items-center gap-2 pt-2 text-sm font-semibold text-brand-secondary animate-in fade-in duration-300">
                                    <CpuChip01 className="size-4" />
                                    Local AI (Claude unavailable)
                                    <span className="font-normal text-tertiary">· {it.reason}</span>
                                </div>
                            );
                        if (it.kind === "note")
                            return (
                                <div key={i} className="text-xs text-tertiary">
                                    {it.text}
                                </div>
                            );
                        if (it.kind === "step") return <LocalStep key={i} item={it} />;
                        if (it.kind === "done")
                            return (
                                <div key={i} className={cx("text-xs", it.ok ? "text-quaternary" : "text-error-primary")}>
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
                        <div className="flex items-center gap-2 text-sm text-tertiary animate-in fade-in duration-300">
                            <Spinner />
                            Working…
                        </div>
                    )}
                    <div ref={bottom} />
                </div>
            </div>
            <div className="px-4 pt-2 pb-4 sm:px-6">
                <div className="mx-auto flex max-w-3xl items-end gap-2 rounded-2xl bg-primary p-2 shadow-sm ring-1 ring-primary transition ring-inset focus-within:ring-2 focus-within:ring-brand">
                    <TextAreaBase
                        aria-label="Message"
                        placeholder="Ask Relay to do something…"
                        rows={2}
                        value={input}
                        onChange={(e) => setInput(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === "Enter" && !e.shiftKey) {
                                e.preventDefault();
                                send();
                            }
                        }}
                        className="max-h-48 min-h-11 flex-1 resize-none bg-transparent shadow-none ring-0 focus:ring-0"
                    />
                    {busy ? (
                        <Button color="secondary" iconLeading={StopCircle} onClick={() => api.chatStop()}>
                            Stop
                        </Button>
                    ) : (
                        <Button iconLeading={Send01} onClick={send} isDisabled={!input.trim()}>
                            Send
                        </Button>
                    )}
                </div>
            </div>
        </div>
    );
};
