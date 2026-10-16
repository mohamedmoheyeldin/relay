// The chat window: one conversation with Cloud AI, with Relay's activity shown inline.
import { type ReactNode, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Button as AriaButton, Header, Menu, MenuItem, MenuSection, MenuTrigger, Popover } from "react-aria-components";
import { Check, CheckCircle, ChevronDown, ChevronRight, CornerDownLeft, CpuChip01, File06, Plus, StopSquare, Tool01, XCircle, XClose } from "@untitledui/icons";
import { TextAreaBase } from "@/components/base/textarea/textarea";
import { Markdown } from "@/components/relay/markdown";
import { Notice, Spinner } from "@/components/relay/ui";
import { type ChatEvent, type ChatSummary, type Project, type Settings, api, errorText } from "@/lib/api";
import { describeTool, isHiddenTool } from "@/lib/describe-tool";
import { isAuthError, refreshCloud, useCloud } from "@/lib/cloud";
import { ChatMenu, RelayMark } from "@/components/relay/sidebar";
import { SignIn } from "./sign-in";
import { cx } from "@/utils/cx";

type Item =
    | { kind: "user"; text: string; files?: string[] }
    | { kind: "assistant"; text: string; streaming?: boolean }
    | { kind: "tool"; id: string; name: string; input: string; result?: { ok: boolean; text: string; image: string | null } }
    | { kind: "done"; ok: boolean; text: string; seconds: number | null }
    | { kind: "error"; text: string }
    | { kind: "fallback"; reason: string }
    | { kind: "note"; text: string }
    | { kind: "step"; step: number; thought: string; action: string; result: string }
    | { kind: "plan"; goal: string; app: string; steps: string[]; doneWhen: string }
    | { kind: "question"; text: string; fallback: string };

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

type CloudSettings = Settings["cloud"];

const PlanCard = ({ item }: { item: Extract<Item, { kind: "plan" }> }) => (
    <div className="rounded-xl px-4 py-3 text-sm ring-1 ring-secondary animate-in fade-in duration-200">
        <p className="font-medium text-primary">
            {item.goal || "Plan"}
            {item.app && <span className="font-normal text-tertiary"> · {item.app}</span>}
        </p>
        {item.steps.length > 0 && (
            <ol className="mt-2 flex list-decimal flex-col gap-1 pl-5 text-secondary">
                {item.steps.map((s, n) => (
                    <li key={n}>{s}</li>
                ))}
            </ol>
        )}
        {item.doneWhen && <p className="mt-2 text-xs text-tertiary">Done when {item.doneWhen.replace(/^\w/, (c) => c.toLowerCase())}</p>}
    </div>
);

/** A task the local AI ran: its plan and everything it did, shown as one card in the
 * chat and in full in its own thread beside it. */
type TaskEntry = {
    type: "task";
    index: number;
    at: number;
    plan: Extract<Item, { kind: "plan" }>;
    children: { item: Item; at: number }[];
    status: "running" | "done" | "failed" | "stopped";
};
type Entry = TaskEntry | { type: "item"; item: Item; at: number };

const TASK_PARTS = new Set(["step", "note", "question", "done"]);

export const groupTasks = (items: Item[], busy: boolean): Entry[] => {
    const out: Entry[] = [];
    let task = null as TaskEntry | null;
    let count = 0;
    items.forEach((item, at) => {
        if (item.kind === "plan") {
            task = { type: "task", index: count++, at, plan: item, children: [], status: "stopped" };
            out.push(task);
            return;
        }
        if (task && TASK_PARTS.has(item.kind)) {
            task.children.push({ item, at });
            // A question also shows in the chat, where it is answered.
            if (item.kind === "question") out.push({ type: "item", item, at });
            if (item.kind === "done") {
                task.status = item.ok ? "done" : "failed";
                task = null;
            }
            return;
        }
        // A reply while the task runs (an answer, a change of plan) doesn't end it.
        if (item.kind !== "user" && task) {
            // Cloud AI went on after the task without a verdict from it: it ended unless stopped.
            const stopped = task.children.some((c) => c.item.kind === "note" && c.item.text.startsWith("Stopping"));
            if (!stopped) task.status = "done";
            task = null;
        }
        out.push({ type: "item", item, at });
    });
    const last = out.filter((e): e is TaskEntry => e.type === "task").pop();
    if (busy && last && last.status === "stopped" && task === last) last.status = "running";
    return out;
};


const TaskStatus = ({ status }: { status: TaskEntry["status"] }) =>
    status === "running" ? (
        <Spinner />
    ) : status === "done" ? (
        <CheckCircle className="size-4 shrink-0 text-fg-success-primary" />
    ) : status === "failed" ? (
        <XCircle className="size-4 shrink-0 text-fg-error-primary" />
    ) : (
        <StopSquare className="size-4 shrink-0 text-fg-quaternary" />
    );

const STATUS_TEXT = { running: "Working", done: "Done", failed: "Didn't finish", stopped: "Stopped" };

const TaskCard = ({ task, active, onOpen }: { task: TaskEntry; active: boolean; onOpen: () => void }) => {
    const steps = task.children.filter((c) => c.item.kind === "step").map((c) => c.item as Extract<Item, { kind: "step" }>);
    const latest = steps[steps.length - 1];
    const done = task.children.find((c) => c.item.kind === "done")?.item as Extract<Item, { kind: "done" }> | undefined;
    return (
        <button
            onClick={onOpen}
            aria-label={`Task: ${task.plan.goal || "Task"}`}
            aria-expanded={active}
            className={cx(
                "flex w-full cursor-pointer flex-col gap-1 rounded-xl border px-4 py-3 text-left transition duration-150 outline-focus-ring animate-in fade-in focus-visible:outline-2",
                active ? "border-primary bg-primary_alt" : "border-secondary hover:border-primary hover:bg-primary_hover",
            )}
        >
            <span className="flex items-center gap-2.5">
                <TaskStatus status={task.status} />
                <span className="min-w-0 flex-1 truncate text-sm font-medium text-primary">{task.plan.goal || "Task"}</span>
                <span className="shrink-0 text-xs text-tertiary">
                    {STATUS_TEXT[task.status]}
                    {steps.length > 0 && ` · ${steps.length} ${steps.length === 1 ? "step" : "steps"}`}
                </span>
                <ChevronRight className={cx("size-4 shrink-0 text-fg-quaternary transition", active && "rotate-180")} />
            </span>
            <span className="truncate pl-[26px] text-[13px] text-tertiary">
                {task.status === "failed" && done?.text
                    ? done.text
                    : latest
                      ? `Step ${latest.step} · ${latest.thought || latest.action}`
                      : task.plan.app
                        ? `In ${task.plan.app}`
                        : "Planning…"}
            </span>
        </button>
    );
};

/** A task's thread, open beside the chat: the plan, every step, and a box to steer it. */
const ThreadPane = ({ task, busy, onClose, onSteer }: { task: TaskEntry; busy: boolean; onClose: () => void; onSteer: (text: string) => void }) => {
    const [text, setText] = useState("");
    const end = useRef<HTMLDivElement>(null);
    useEffect(() => end.current?.scrollIntoView({ behavior: "smooth" }), [task.children.length]);
    const running = task.status === "running" && busy;
    return (
        <aside aria-label="Task thread" className="flex w-[400px] shrink-0 flex-col border-l border-secondary bg-primary animate-in fade-in slide-in-from-right-2 duration-200">
            <div className="h-9 shrink-0" />
            <div className="flex h-11 shrink-0 items-center gap-2 px-4">
                <span className="text-sm text-tertiary">Tasks</span>
                <ChevronRight className="size-3.5 shrink-0 text-fg-quaternary" />
                <span className="min-w-0 flex-1 truncate text-sm font-medium text-primary">{task.plan.goal || "Task"}</span>
                <button
                    aria-label="Close thread"
                    title="Close thread"
                    onClick={onClose}
                    className="flex size-7 cursor-pointer items-center justify-center rounded-md text-fg-tertiary hover:bg-primary_hover hover:text-fg-secondary"
                >
                    <XClose className="size-4" />
                </button>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
                <div className="flex flex-col gap-3">
                    <PlanCard item={task.plan} />
                    {task.children.map(({ item, at }) => {
                        if (item.kind === "step") return <LocalStep key={at} item={item} />;
                        if (item.kind === "note")
                            return (
                                <p key={at} className="text-xs text-tertiary">
                                    {item.text}
                                </p>
                            );
                        if (item.kind === "question")
                            return (
                                <p key={at} className="rounded-lg px-3 py-2 text-sm text-primary ring-1 ring-brand">
                                    {item.text}
                                </p>
                            );
                        if (item.kind === "done")
                            return (
                                <p key={at} className={cx("text-xs", item.ok ? "text-tertiary" : "text-error-primary")}>
                                    {item.ok ? `Done${item.seconds ? ` in ${item.seconds.toFixed(1)} s` : ""}` : item.text}
                                </p>
                            );
                        return null;
                    })}
                    {running && (
                        <div className="flex items-center gap-2 text-sm text-tertiary">
                            <Spinner />
                            <span className="animate-pulse">Working…</span>
                        </div>
                    )}
                    <div ref={end} />
                </div>
            </div>
            {running && (
                <div className="p-3">
                    <input
                        aria-label="Steer this task"
                        placeholder="Steer this task…"
                        value={text}
                        onChange={(e) => setText(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === "Enter" && text.trim()) {
                                onSteer(text.trim());
                                setText("");
                            }
                        }}
                        className="h-10 w-full rounded-xl border border-primary bg-primary_alt px-3 text-sm text-primary outline-hidden placeholder:text-placeholder focus:border-[#5c5c5b]"
                    />
                </div>
            )}
        </aside>
    );
};

const FileChip = ({ path, onRemove }: { path: string; onRemove?: () => void }) => (
    <span title={path} className="inline-flex max-w-60 items-center gap-1.5 rounded-lg bg-secondary px-2 py-1 text-xs text-secondary ring-1 ring-secondary">
        <File06 className="size-3.5 shrink-0 text-fg-quaternary" />
        <span className="truncate">{path.split(/[\\/]/).pop()}</span>
        {onRemove && (
            <button aria-label="Remove" onClick={onRemove} className="cursor-pointer text-fg-quaternary hover:text-fg-secondary">
                <XClose className="size-3" />
            </button>
        )}
    </span>
);

// Cloud AI takes these aliases for --model; empty means its own default.
const MODELS = [
    { id: "", label: "Default", hint: "Cloud AI's choice" },
    { id: "opus", label: "Opus", hint: "Most capable" },
    { id: "sonnet", label: "Sonnet", hint: "Fast and smart" },
    { id: "haiku", label: "Haiku", hint: "Fastest" },
];
const EFFORTS = [
    { id: "", label: "Default" },
    { id: "low", label: "Low" },
    { id: "medium", label: "Medium" },
    { id: "high", label: "High" },
    { id: "max", label: "Max" },
];
const pickerItem =
    "flex cursor-pointer items-center gap-2 rounded-md px-2.5 py-1.5 text-sm text-secondary outline-hidden data-[focused]:bg-tertiary data-[focused]:text-primary";

/** The model and effort picker next to Send, like the reference chat app's. */
const ModelPicker = ({ settings, onChange }: { settings: CloudSettings; onChange: (next: CloudSettings) => void }) => {
    const model = MODELS.find((m) => m.id === settings.model)?.label ?? settings.model;
    const effort = EFFORTS.find((e) => e.id === settings.effort)?.label ?? settings.effort;
    return (
        <MenuTrigger>
            <AriaButton
                aria-label="Model and effort"
                className="flex h-7 cursor-pointer items-center gap-3 rounded-md px-2 text-[13px] text-secondary outline-hidden transition duration-150 hover:bg-primary_hover hover:text-primary data-[focus-visible]:ring-2 data-[focus-visible]:ring-brand"
            >
                <span>{model}</span>
                {settings.effort && <span>{effort}</span>}
            </AriaButton>
            <Popover placement="top end" className="max-h-[70vh] w-64 overflow-y-auto rounded-xl bg-primary p-1 shadow-lg ring-1 ring-secondary outline-hidden animate-in fade-in zoom-in-95 duration-150">
                <Menu
                    className="outline-hidden"
                    onAction={(key) => {
                        const [kind, id] = String(key).split(":");
                        onChange(kind === "model" ? { ...settings, model: id } : { ...settings, effort: id });
                    }}
                >
                    <MenuSection>
                        <Header className="px-2.5 pt-1.5 pb-1 text-xs font-medium text-quaternary">Model</Header>
                        {MODELS.map((m) => (
                            <MenuItem key={m.id} id={`model:${m.id}`} className={pickerItem}>
                                <span className="flex-1">{m.label}</span>
                                <span className="text-xs text-quaternary">{m.hint}</span>
                                {settings.model === m.id && <Check className="size-4 text-fg-brand-primary" />}
                            </MenuItem>
                        ))}
                    </MenuSection>
                    <MenuSection className="mt-1 border-t border-secondary pt-1">
                        <Header className="px-2.5 pt-1.5 pb-1 text-xs font-medium text-quaternary">Effort</Header>
                        {EFFORTS.map((e) => (
                            <MenuItem key={e.id} id={`effort:${e.id}`} className={pickerItem}>
                                <span className="flex-1">{e.label}</span>
                                {settings.effort === e.id && <Check className="size-4 text-fg-brand-primary" />}
                            </MenuItem>
                        ))}
                    </MenuSection>
                </Menu>
            </Popover>
        </MenuTrigger>
    );
};

const greeting = () => {
    const h = new Date().getHours();
    return h < 5 ? "Working late?" : h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening";
};

const LOCAL_MODEL_MISSING = /Settings > Local AI/;

const APPROVAL_NOTE: Record<string, string> = {
    risky: "Relay asks here only before steps that can't be undone.",
    ask: "Relay asks here before every click or keystroke.",
    off: "Relay acts without asking.",
};

/** Chats deleted while open: their last save must not bring them back. */
const forgotten = new Set<string>();
export const forgetChat = (id: string) => forgotten.add(id);

/** Which conversation the screen shows; `fresh` ones have nothing saved yet. */
export type OpenChat = { id: string; fresh: boolean; project?: string | null; task?: number };

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
    saved,
    onRename,
    onPin,
    onDelete,
    cloudSettings,
    onCloudSettings,
    approvalMode = "risky",
    projects = [],
    onMove,
    onOpenProject,
}: {
    projects?: Project[];
    onMove?: (project: string | null) => void;
    onOpenProject?: (id: string) => void;
    ready: boolean;
    /** Settings > Safety: what the line under the composer promises. */
    approvalMode?: string;
    onSetup: () => void;
    onLocalAi: () => void;
    chat: OpenChat;
    /** A chat was saved, so the sidebar list may have changed. */
    onSaved: () => void;
    /** This chat as saved (for the title menu), once something was said in it. */
    saved: ChatSummary | null;
    onRename: () => void;
    onPin: () => void;
    onDelete: () => void;
    /** Settings > Cloud AI, for the model and effort picker in the composer. */
    cloudSettings: CloudSettings;
    onCloudSettings: (next: CloudSettings) => void;
}) => {
    const [files, setFiles] = useState<string[]>([]);
    const [items, setItems] = useState<Item[]>([]);
    const [loadError, setLoadError] = useState("");
    const [input, setInput] = useState("");
    const [busy, setBusy] = useState(false);
    const [signingIn, setSigningIn] = useState(false);
    // The task whose thread is open beside the chat, like a thread in the reference chat app.
    const [thread, setThread] = useState<number | null>(null);
    // Auth failures at or before this index were fixed by signing in again.
    const [signedInAt, setSignedInAt] = useState(-1);
    const bottom = useRef<HTMLDivElement>(null);
    const cloud = useCloud();

    let lastAuthFailure = -1;
    items.forEach((it, i) => {
        if ((it.kind === "error" || (it.kind === "done" && !it.ok)) && isAuthError(it.text)) lastAuthFailure = i;
    });
    const signedOut = !!cloud.state?.installed && !cloud.state.auth.loggedIn;
    // Signed out is fine (the note at the top says so); an expired sign-in Cloud AI just hit is worth a button.
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
                        if (e.is_error && isAuthError(e.result)) refreshCloud().catch(() => {});
                        return [...prev, { kind: "done", ok: !e.is_error, text: e.is_error ? e.result : "", seconds: e.duration_ms ? e.duration_ms / 1000 : null }];
                    case "error":
                        return [...prev, { kind: "error", text: e.message }];
                    case "fallback":
                        refreshCloud().catch(() => {});
                        return [...prev, { kind: "fallback", reason: e.reason }];
                    case "local_note": {
                        // The plan and the question also come as notes; they're shown once.
                        const last = prev[prev.length - 1];
                        if (last?.kind === "plan" && e.text.startsWith("Plan:")) return prev;
                        if (last?.kind === "question" && e.text === last.text) return prev;
                        return [...prev, { kind: "note", text: e.text }];
                    }
                    case "local_plan":
                        return [...prev, { kind: "plan", goal: e.goal, app: e.app, steps: e.steps, doneWhen: e.done_when }];
                    case "local_question":
                        return [...prev, { kind: "question", text: e.text, fallback: e.default }];
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

    const attach = async () => {
        const picked = await openDialog({ multiple: true, title: "Attach files" }).catch(() => null);
        const list = Array.isArray(picked) ? picked : picked ? [picked] : [];
        if (list.length) setFiles((f) => [...f, ...list.filter((p) => !f.includes(p))]);
    };

    const send = async () => {
        const text = input.trim();
        if (!text) return;
        // While Relay works, a message answers its question or changes the plan.
        if (busy) {
            setItems((p) => [...p, { kind: "user", text }]);
            setInput("");
            api.chatSay(text, project).catch((e) => setItems((p) => [...p, { kind: "error", text: errorText(e) }]));
            return;
        }
        const attached = files;
        setItems((p) => [...p, { kind: "user", text, files: attached.length ? attached : undefined }]);
        setInput("");
        setFiles([]);
        setBusy(true);
        // Files go along as paths on this PC; Cloud AI and the local AI open them from there.
        const prompt = attached.length ? `${text}\n\nAttached files:\n${attached.map((f) => `- ${f}`).join("\n")}` : text;
        try {
            await api.chatSend(prompt, project);
        } catch (e) {
            setItems((p) => [...p, { kind: "error", text: errorText(e) }]);
            setBusy(false);
        }
    };

    // Switching chats: a running task stops, then the chosen chat (or a fresh one) loads.
    // `shown` is the chat whose items are on screen, so saves never go to the wrong file.
    const shown = useRef<string | null>(null);
    const lastSaved = useRef<Record<string, string>>({});
    const busyRef = useRef(busy);
    busyRef.current = busy;
    useEffect(() => {
        let cancelled = false;
        shown.current = null;
        setThread(chat.task ?? null);
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
                    const loaded = Array.isArray(saved.items) ? (saved.items as Item[]) : [];
                    lastSaved.current[chat.id] = JSON.stringify(forSaving(loaded));
                    setItems(loaded);
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
            const kept = forSaving(items);
            const json = JSON.stringify(kept);
            // Opening a chat changes nothing, so it keeps its place in the list.
            if (forgotten.has(id) || lastSaved.current[id] === json) return;
            lastSaved.current[id] = json;
            api.chatSave(id, titleOf(items), kept, project)
                .then(onSaved)
                .catch(() => {});
        };
        const timer = setTimeout(save, busy ? 1500 : 300);
        return () => {
            clearTimeout(timer);
            if (shown.current !== id) save();
        };
    }, [items, busy]);

    const project = saved?.project ?? chat.project ?? null;
    const inProject = project ? projects.find((p) => p.id === project) : undefined;
    const entries = groupTasks(items, busy);
    const tasks = entries.filter((e): e is TaskEntry => e.type === "task");
    const openTask = thread === null ? undefined : tasks.find((t) => t.index === thread);
    const renderItem = (it: Item, i: number) => {
                    if (it.kind === "user")
                        return (
                            <div
                                key={i}
                                className="ml-auto max-w-[80%] rounded-xl bg-tertiary px-4 py-2.5 text-[15px] leading-6 break-words whitespace-pre-wrap text-primary animate-in fade-in duration-200 dark:bg-[#212121]"
                            >
                                {it.text}
                                {it.files && (
                                    <div className="mt-2 flex flex-wrap gap-1.5">
                                        {it.files.map((f) => (
                                            <FileChip key={f} path={f} />
                                        ))}
                                    </div>
                                )}
                            </div>
                        );
                    if (it.kind === "assistant" && isAuthError(it.text) && items[i + 1]?.kind === "done") return null;
                    if (it.kind === "assistant")
                        return (
                            <div key={i} className="text-[15px] leading-[1.7] break-words text-primary animate-in fade-in duration-300">
                                <Markdown text={it.text} />
                            </div>
                        );
                    if (it.kind === "tool")
                        return isHiddenTool(it.name) && it.result?.ok !== false ? null : <ToolItem key={i} item={it} />;
                    if (it.kind === "fallback")
                        return (
                            <div key={i} className="flex items-center gap-2 pt-1 text-sm font-medium text-brand-secondary animate-in fade-in duration-300">
                                <CpuChip01 className="size-4 shrink-0" />
                                Local AI (Cloud AI unavailable)
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
                    if (it.kind === "plan") return <PlanCard key={i} item={it} />;
                    if (it.kind === "question")
                        return (
                            <div key={i} className="rounded-xl px-4 py-3 text-sm text-primary ring-1 ring-brand animate-in fade-in duration-200">
                                <p className="font-medium">{it.text}</p>
                                <p className="mt-1 text-xs text-tertiary">
                                    {busy && i === items.length - 1 ? "Reply below; Relay waits for your answer" : "Asked while working"}
                                    {it.fallback ? `, or goes with "${it.fallback}".` : "."}
                                </p>
                            </div>
                        );
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
    };

    const empty = items.length === 0;
    const sendable = !!input.trim();
    const composer = (
        <div>
            <div className="rounded-xl border border-primary bg-primary_alt transition duration-150 focus-within:border-[#5c5c5b]">
                {files.length > 0 && (
                    <div className="flex flex-wrap gap-1.5 px-3 pt-3">
                        {files.map((f) => (
                            <FileChip key={f} path={f} onRemove={() => setFiles((all) => all.filter((x) => x !== f))} />
                        ))}
                    </div>
                )}
                <div className="flex items-end gap-2 py-2 pr-2.5 pl-1">
                    <TextAreaBase
                        aria-label="Message"
                        placeholder={busy ? "Reply to Relay, or answer its question…" : empty ? "Ask Relay a question or start a task…" : "Reply to Relay…"}
                        rows={empty ? 3 : 1}
                        value={input}
                        onChange={(e) => setInput(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === "Enter" && !e.shiftKey) {
                                e.preventDefault();
                                send();
                            }
                        }}
                        className="max-h-60 min-h-9 flex-1 resize-none rounded-none bg-transparent px-2.5 py-1.5 text-[15px] leading-6 shadow-none ring-0 placeholder:text-placeholder focus:ring-0"
                    />
                    {busy && !sendable ? (
                        <button
                            aria-label="Stop"
                            title="Stop"
                            onClick={stop}
                            className="mb-0.5 flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-lg bg-primary-solid text-white transition duration-100 outline-focus-ring hover:opacity-85 focus-visible:outline-2 dark:bg-[#ececec] dark:text-[#151515]"
                        >
                            <StopSquare className="size-3.5" />
                        </button>
                    ) : (
                        <button
                            aria-label="Send"
                            title="Send"
                            onClick={send}
                            disabled={!sendable}
                            className={cx(
                                "mb-0.5 flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-lg transition duration-100 outline-focus-ring focus-visible:outline-2 disabled:cursor-default",
                                sendable ? "bg-brand-solid text-white hover:bg-brand-solid_hover" : "text-fg-tertiary",
                            )}
                        >
                            <CornerDownLeft className="size-4" />
                        </button>
                    )}
                </div>
            </div>
            <div className="mt-1.5 flex items-center gap-1 px-0.5">
                <button
                    aria-label="Attach files"
                    title="Attach files"
                    onClick={attach}
                    className="flex size-7 shrink-0 cursor-pointer items-center justify-center rounded-md text-fg-tertiary transition duration-100 outline-focus-ring hover:bg-primary_hover hover:text-fg-secondary focus-visible:outline-2"
                >
                    <Plus className="size-4" />
                </button>
                <span className="ml-auto" />
                <ModelPicker settings={cloudSettings} onChange={onCloudSettings} />
            </div>
            <p className="mt-1 text-center text-[11px] text-quaternary">{APPROVAL_NOTE[approvalMode] ?? APPROVAL_NOTE.risky}</p>
        </div>
    );

    return (
        <div className="flex h-full min-h-0">
        <div className="flex h-full min-w-0 flex-1 flex-col">
            <div className="h-9 shrink-0" />
            <div className="flex h-11 shrink-0 items-center gap-1 px-5">
                {inProject && (
                    <>
                        <button
                            onClick={() => onOpenProject?.(inProject.id)}
                            className="relative z-40 max-w-[30%] cursor-pointer truncate rounded-md px-2 py-1 text-sm text-tertiary hover:bg-primary_hover hover:text-primary"
                        >
                            {inProject.name}
                        </button>
                        {!empty && <span className="text-sm text-quaternary">/</span>}
                    </>
                )}
                {!empty &&
                    (saved ? (
                        <ChatMenu
                            title={saved.title}
                            pinned={saved.pinned}
                            onPin={onPin}
                            onRename={onRename}
                            onDelete={onDelete}
                            projects={projects}
                            project={saved.project}
                            onMove={onMove}
                            className="relative z-40 flex h-7 max-w-[60%] cursor-pointer items-center gap-1 rounded-md px-2 text-sm font-medium text-primary outline-hidden hover:bg-primary_hover data-[focus-visible]:ring-2 data-[focus-visible]:ring-brand"
                        >
                            <span className="truncate">{saved.title}</span>
                            <ChevronDown className="size-3.5 shrink-0 text-fg-quaternary" />
                        </ChatMenu>
                    ) : (
                        <span className="truncate px-2 text-sm font-medium text-primary">{titleOf(items)}</span>
                    ))}
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-6 sm:px-6">
                <div className={cx("mx-auto flex w-full max-w-[720px] flex-col gap-4", empty && "min-h-full justify-center pb-24")}>
                    {loadError && <Notice tone="error">{loadError}</Notice>}
                    {!ready && (
                        <p className="text-center text-xs text-tertiary">
                            Cloud AI isn't signed in, so the local AI does screen tasks on its own. Writing and judgment need Cloud AI:{" "}
                            <button className="cursor-pointer font-medium underline" onClick={onSetup}>
                                sign in
                            </button>
                            .
                        </p>
                    )}
                    {items.length === 0 && (
                        <div className="flex flex-col items-center text-center animate-in fade-in slide-in-from-bottom-3 duration-700">
                            <div className="flex items-center gap-3">
                                <RelayMark className="size-8" />
                                <h2 className="font-serif text-[2.1rem] leading-tight font-normal tracking-tight text-primary">{inProject ? inProject.name : greeting()}</h2>
                            </div>
                            <div className="mt-7 w-full text-left">{composer}</div>
                            <div className="mt-5 flex max-w-2xl flex-wrap justify-center gap-2">
                                {SUGGESTIONS.map((t, n) => (
                                    <button
                                        key={t}
                                        onClick={() => setInput(t)}
                                        style={{ animationDelay: `${150 + n * 60}ms` }}
                                        className="cursor-pointer rounded-lg px-3 py-1.5 text-sm text-tertiary ring-1 ring-secondary transition duration-100 ring-inset animate-in fade-in fill-mode-both hover:bg-secondary hover:text-primary"
                                    >
                                        {t}
                                    </button>
                                ))}
                            </div>
                        </div>
                    )}
                    {entries.map((e) =>
                        e.type === "task" ? (
                            <TaskCard key={`t${e.at}`} task={e} active={thread === e.index} onOpen={() => setThread(thread === e.index ? null : e.index)} />
                        ) : (
                            renderItem(e.item, e.at)
                        ),
                    )}
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
                                {signedOut ? "Cloud AI isn't signed in." : "Your Cloud AI sign-in has expired, so Cloud AI can't answer."}{" "}
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
                    <div className="mx-auto max-w-[720px]">{composer}</div>
                </div>
            )}
        </div>
        {openTask && (
            <ThreadPane
                task={openTask}
                busy={busy}
                onClose={() => setThread(null)}
                onSteer={(text) => {
                    setItems((p) => [...p, { kind: "user", text }]);
                    api.chatSay(text, project).catch((e) => setItems((p) => [...p, { kind: "error", text: errorText(e) }]));
                }}
            />
        )}
        </div>
    );
};
