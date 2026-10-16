// The floating Relay popup (popup.rs): what Relay is doing right now, Stop, and a box
// to tell it something new while it works. It lives in its own small window.
import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ArrowUp, Maximize02, StopSquare, XClose } from "@untitledui/icons";
import { type ChatEvent, api, errorText } from "@/lib/api";
import { describeTool, isHiddenTool } from "@/lib/describe-tool";
import { cx } from "@/utils/cx";

type Status = { text: string; state: "working" | "done" | "failed" | "idle" };

const oneLine = (text: string, max = 140) => {
    const line = text.replace(/\s+/g, " ").trim();
    return line.length > max ? `${line.slice(0, max - 1)}…` : line;
};

/** The line the popup shows for one chat event, or null to keep the last one. */
export const statusFor = (e: ChatEvent): Status | null => {
    switch (e.type) {
        case "tool_use":
            return isHiddenTool(e.name) ? null : { text: describeTool(e.name, e.input, false), state: "working" };
        case "local_step":
            return { text: oneLine(e.thought || e.action), state: "working" };
        case "local_note":
            return { text: oneLine(e.text), state: "working" };
        case "text":
            return e.text.trim() ? { text: oneLine(e.text), state: "working" } : null;
        case "fallback":
            return { text: "Working on it with the local AI", state: "working" };
        case "done":
            return e.is_error ? { text: oneLine(e.result || "Relay couldn't finish that."), state: "failed" } : { text: "Done", state: "done" };
        case "error":
            return { text: oneLine(e.message), state: "failed" };
        default:
            return null;
    }
};

export const Popup = () => {
    const [status, setStatus] = useState<Status>({ text: "Ask Relay anything", state: "idle" });
    const [busy, setBusy] = useState(false);
    const [input, setInput] = useState("");
    const [error, setError] = useState("");
    const field = useRef<HTMLInputElement>(null);

    useEffect(() => {
        document.documentElement.classList.add("popup");
        const chat = listen<{ event: ChatEvent | null; finished: boolean }>("chat", ({ payload }) => {
            if (payload.finished) {
                setBusy(false);
                return;
            }
            setBusy(true);
            const next = payload.event && statusFor(payload.event);
            if (next) setStatus(next);
        });
        // The shortcut opened the popup to type into.
        const focus = listen("popup-focus", () => field.current?.focus());
        return () => {
            chat.then((f) => f());
            focus.then((f) => f());
        };
    }, []);

    const say = async () => {
        const text = input.trim();
        if (!text) return;
        setInput("");
        setError("");
        setStatus({ text: oneLine(text), state: "working" });
        try {
            await api.popupSay(text);
        } catch (e) {
            setError(errorText(e));
        } finally {
            api.popupRelease().catch(() => {});
        }
    };

    const dot = {
        working: "bg-brand-solid animate-pulse",
        done: "bg-success-solid",
        failed: "bg-error-solid",
        idle: "bg-quaternary",
    }[status.state];

    return (
        <div className="flex h-screen w-screen items-end p-2">
            <div className="flex w-full flex-col gap-2 rounded-2xl bg-primary_alt p-3 shadow-[0_0.5rem_2rem_rgb(0_0_0/0.18)] ring-1 ring-secondary">
                <div className="flex items-center gap-2.5" data-tauri-drag-region>
                    <span className={cx("size-2 shrink-0 rounded-full", dot)} />
                    <span className="min-w-0 flex-1 truncate text-sm font-medium text-primary" title={status.text} data-tauri-drag-region>
                        {status.text}
                    </span>
                    {busy && (
                        <button
                            aria-label="Stop"
                            title="Stop"
                            onClick={() => api.chatStop()}
                            className="flex size-7 shrink-0 cursor-pointer items-center justify-center rounded-full bg-primary-solid text-white hover:opacity-85 dark:bg-neutral-100 dark:text-neutral-900"
                        >
                            <StopSquare className="size-3.5" />
                        </button>
                    )}
                    <button
                        aria-label="Open Relay"
                        title="Open Relay"
                        onClick={() => api.popupOpenMain()}
                        className="flex size-7 shrink-0 cursor-pointer items-center justify-center rounded-full text-fg-quaternary hover:bg-primary_hover"
                    >
                        <Maximize02 className="size-3.5" />
                    </button>
                    <button
                        aria-label="Hide"
                        title="Hide"
                        onClick={() => api.popupHide()}
                        className="flex size-7 shrink-0 cursor-pointer items-center justify-center rounded-full text-fg-quaternary hover:bg-primary_hover"
                    >
                        <XClose className="size-3.5" />
                    </button>
                </div>
                <div className="flex items-center gap-2 rounded-xl bg-primary px-3 py-1.5 ring-1 ring-secondary focus-within:ring-primary">
                    <input
                        ref={field}
                        aria-label="Tell Relay"
                        placeholder={busy ? "Tell Relay something new…" : "Ask Relay…"}
                        value={input}
                        onMouseDown={() => api.popupGrab().catch(() => {})}
                        onFocus={() => api.popupGrab().catch(() => {})}
                        onBlur={() => api.popupRelease().catch(() => {})}
                        onChange={(e) => setInput(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") {
                                e.preventDefault();
                                say();
                            } else if (e.key === "Escape") {
                                field.current?.blur();
                            }
                        }}
                        className="min-w-0 flex-1 bg-transparent text-sm text-primary outline-none placeholder:text-placeholder"
                    />
                    <button
                        aria-label="Send"
                        title="Send"
                        onClick={say}
                        disabled={!input.trim()}
                        className="flex size-6 shrink-0 cursor-pointer items-center justify-center rounded-full bg-brand-solid text-white disabled:cursor-default disabled:opacity-40"
                    >
                        <ArrowUp className="size-3.5" />
                    </button>
                </div>
                {error && <p className="truncate px-1 text-xs text-error-primary">{error}</p>}
            </div>
        </div>
    );
};
