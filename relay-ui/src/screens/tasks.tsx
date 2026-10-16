// Every task Relay ran on screen, newest first, from the saved chats. Opening one
// shows its chat with the task's thread open beside it.
import { useEffect, useState } from "react";
import { CheckCircle, SearchLg, XCircle } from "@untitledui/icons";
import { type TaskSummary, api } from "@/lib/api";
import { Spinner } from "@/components/relay/ui";
import { cx } from "@/utils/cx";

type Filter = "all" | "done" | "failed" | "open";

const when = (ms: number) =>
    new Date(ms).toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });

export const TasksScreen = ({ onOpen, refreshKey }: { onOpen: (chatId: string, index: number) => void; refreshKey: number }) => {
    const [tasks, setTasks] = useState<TaskSummary[] | null>(null);
    const [filter, setFilter] = useState<Filter>("all");
    const [query, setQuery] = useState("");
    useEffect(() => {
        api.tasksList()
            .then(setTasks)
            .catch(() => setTasks([]));
    }, [refreshKey]);
    const q = query.trim().toLowerCase();
    const shown = (tasks ?? []).filter(
        (t) =>
            (filter === "all" || (filter === "done" ? t.ok === true : filter === "failed" ? t.ok === false : t.ok === null)) &&
            (!q || t.goal.toLowerCase().includes(q) || t.chat_title.toLowerCase().includes(q) || t.app.toLowerCase().includes(q)),
    );
    const tabs: [Filter, string][] = [
        ["all", "All"],
        ["done", "Done"],
        ["failed", "Didn't finish"],
        ["open", "Running"],
    ];
    return (
        <div className="h-full overflow-y-auto px-6 pt-12 pb-10">
            <div className="mx-auto flex max-w-[860px] flex-col gap-5">
                <div>
                    <h1 className="font-serif text-[1.9rem] text-primary">Tasks</h1>
                    <p className="mt-1 text-sm text-tertiary">Everything Relay did on this PC, with every step it took.</p>
                </div>
                <div className="flex flex-wrap items-center gap-3">
                    <div className="flex rounded-lg bg-secondary p-0.5" role="tablist" aria-label="Show tasks">
                        {tabs.map(([id, label]) => (
                            <button
                                key={id}
                                role="tab"
                                aria-selected={filter === id}
                                onClick={() => setFilter(id)}
                                className={cx(
                                    "cursor-pointer rounded-md px-3 py-1 text-sm transition",
                                    filter === id ? "bg-quaternary text-primary" : "text-tertiary hover:text-primary",
                                )}
                            >
                                {label}
                            </button>
                        ))}
                    </div>
                    <div className="flex h-9 min-w-56 flex-1 items-center gap-2.5 rounded-xl border border-primary bg-primary_alt px-3 focus-within:border-[#5c5c5b]">
                        <SearchLg className="size-4 shrink-0 text-fg-tertiary" />
                        <input
                            aria-label="Search tasks"
                            placeholder="Search tasks…"
                            value={query}
                            onChange={(e) => setQuery(e.target.value)}
                            className="min-w-0 flex-1 bg-transparent text-sm text-primary outline-hidden placeholder:text-placeholder"
                        />
                    </div>
                </div>
                <div className="flex flex-col">
                    {tasks === null && <p className="py-6 text-sm text-tertiary">Loading tasks…</p>}
                    {tasks !== null && shown.length === 0 && (
                        <p className="py-10 text-center text-sm text-tertiary">
                            {tasks.length === 0 ? "Tasks show up here once Relay works on screen for you." : "No tasks match."}
                        </p>
                    )}
                    {shown.map((t) => (
                        <button
                            key={`${t.chat_id}-${t.index}`}
                            onClick={() => onOpen(t.chat_id, t.index)}
                            className="flex cursor-pointer items-center gap-3 border-b border-secondary px-2 py-3 text-left last:border-b-0 hover:bg-primary_hover"
                        >
                            {t.ok === null ? (
                                <Spinner />
                            ) : t.ok ? (
                                <CheckCircle className="size-4 shrink-0 text-fg-success-primary" />
                            ) : (
                                <XCircle className="size-4 shrink-0 text-fg-error-primary" />
                            )}
                            <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                                <span className="truncate text-sm font-medium text-primary">{t.goal || "Task"}</span>
                                <span className="truncate text-xs text-tertiary">
                                    {[t.app, `${t.steps} ${t.steps === 1 ? "step" : "steps"}`, t.chat_title].filter(Boolean).join(" · ")}
                                    {t.ok === false && t.result ? ` · ${t.result}` : ""}
                                </span>
                            </span>
                            <span className="shrink-0 text-xs text-quaternary">{when(t.updated)}</span>
                        </button>
                    ))}
                </div>
            </div>
        </div>
    );
};
