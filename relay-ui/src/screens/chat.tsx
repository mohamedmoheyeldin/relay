// The chat window: one conversation with Claude, with Relay's activity shown inline.
import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { CheckCircle, Plus, Send01, StopCircle, Tool01, XCircle } from "@untitledui/icons";
import { Button } from "@/components/base/buttons/button";
import { TextAreaBase } from "@/components/base/textarea/textarea";
import { Notice } from "@/components/relay/ui";
import { type ChatEvent, api, errorText } from "@/lib/api";
import { cx } from "@/utils/cx";

type Item =
    | { kind: "user"; text: string }
    | { kind: "assistant"; text: string }
    | { kind: "tool"; id: string; name: string; input: string; result?: { ok: boolean; text: string; image: string | null } }
    | { kind: "done"; ok: boolean; text: string; seconds: number | null }
    | { kind: "error"; text: string };

const toolLabel = (name: string) => {
    const short = name.replace(/^mcp__/, "").replace(/__/g, " · ");
    return short.replace(/_/g, " ");
};

const ToolItem = ({ item }: { item: Extract<Item, { kind: "tool" }> }) => {
    const [open, setOpen] = useState(false);
    return (
        <div className="rounded-lg bg-secondary px-3 py-2 text-sm">
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

export const ChatScreen = ({ ready, onSetup }: { ready: boolean; onSetup: () => void }) => {
    const [items, setItems] = useState<Item[]>([]);
    const [input, setInput] = useState("");
    const [busy, setBusy] = useState(false);
    const bottom = useRef<HTMLDivElement>(null);

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
                    case "text":
                        return e.text.trim() ? [...prev, { kind: "assistant", text: e.text }] : prev;
                    case "tool_use":
                        return [...prev, { kind: "tool", id: e.id, name: e.name, input: e.input }];
                    case "tool_result":
                        return prev.map((it) =>
                            it.kind === "tool" && it.id === e.tool_use_id ? { ...it, result: { ok: !e.is_error, text: e.text, image: e.image } } : it,
                        );
                    case "done":
                        return [...prev, { kind: "done", ok: !e.is_error, text: e.is_error ? e.result : "", seconds: e.duration_ms ? e.duration_ms / 1000 : null }];
                    case "error":
                        return [...prev, { kind: "error", text: e.message }];
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
            <div className="flex items-center justify-between border-b border-secondary px-6 py-3">
                <span className="text-md font-semibold text-primary">Chat</span>
                <Button size="sm" color="secondary" iconLeading={Plus} onClick={newChat} isDisabled={busy}>
                    New chat
                </Button>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto px-6 py-6">
                <div className="mx-auto flex max-w-3xl flex-col gap-4">
                    {!ready && (
                        <Notice>
                            Claude Code isn't set up yet.{" "}
                            <button className="cursor-pointer font-semibold underline" onClick={onSetup}>
                                Finish setup
                            </button>{" "}
                            to start a task.
                        </Notice>
                    )}
                    {items.length === 0 && ready && (
                        <div className="py-16 text-center">
                            <h2 className="text-display-xs font-semibold text-primary">What should Relay do?</h2>
                            <p className="mt-2 text-md text-tertiary">Ask for a task on this PC, like "fill in the form in Brave with my details" or "summarise my unread email".</p>
                        </div>
                    )}
                    {items.map((it, i) => {
                        if (it.kind === "user")
                            return (
                                <div key={i} className="ml-auto max-w-[80%] rounded-2xl rounded-tr-sm bg-brand-solid px-4 py-2.5 text-md whitespace-pre-wrap text-white">
                                    {it.text}
                                </div>
                            );
                        if (it.kind === "assistant") return <div key={i} className="max-w-[90%] text-md whitespace-pre-wrap text-primary">{it.text}</div>;
                        if (it.kind === "tool") return <ToolItem key={i} item={it} />;
                        if (it.kind === "done")
                            return (
                                <div key={i} className={cx("text-xs", it.ok ? "text-quaternary" : "text-error-primary")}>
                                    {it.ok ? `Done${it.seconds ? ` in ${it.seconds.toFixed(1)} s` : ""}` : it.text}
                                </div>
                            );
                        return (
                            <Notice key={i} tone="error">
                                {it.text}
                            </Notice>
                        );
                    })}
                    {busy && <div className="text-sm text-tertiary">Working…</div>}
                    <div ref={bottom} />
                </div>
            </div>
            <div className="border-t border-secondary px-6 py-4">
                <div className="mx-auto flex max-w-3xl items-end gap-3">
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
                        className="flex-1 resize-none"
                        disabled={!ready}
                    />
                    {busy ? (
                        <Button color="secondary" iconLeading={StopCircle} onClick={() => api.chatStop()}>
                            Stop
                        </Button>
                    ) : (
                        <Button iconLeading={Send01} onClick={send} isDisabled={!ready || !input.trim()}>
                            Send
                        </Button>
                    )}
                </div>
            </div>
        </div>
    );
};
