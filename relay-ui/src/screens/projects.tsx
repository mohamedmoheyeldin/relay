// Projects, laid out like the reference chat app: a page of project cards, and a page
// per project with its chats and the instructions Cloud AI reads in each of them.
import { useEffect, useState } from "react";
import { ArrowLeft, Edit03, Folder, Plus, SearchLg, Trash01 } from "@untitledui/icons";
import { Button } from "@/components/base/buttons/button";
import { type ChatSummary, type Project, api, errorText } from "@/lib/api";
import { Notice } from "@/components/relay/ui";
import { cx } from "@/utils/cx";

const ago = (ms: number) => {
    const s = Math.max(0, (Date.now() - ms) / 1000);
    if (s < 60) return "just now";
    if (s < 3600) return `${Math.round(s / 60)} min ago`;
    if (s < 86400) return `${Math.round(s / 3600)} h ago`;
    const d = Math.round(s / 86400);
    return d === 1 ? "yesterday" : `${d} days ago`;
};

const field =
    "w-full rounded-xl border border-primary bg-primary_alt px-3 py-2 text-sm text-primary outline-hidden placeholder:text-placeholder focus:border-[#5c5c5b]";

/** Name and instructions, for a new project or an existing one. */
const ProjectForm = ({
    project,
    onSaved,
    onCancel,
}: {
    project?: Project;
    onSaved: (p: Project) => void;
    onCancel: () => void;
}) => {
    const [name, setName] = useState(project?.name ?? "");
    const [instructions, setInstructions] = useState(project?.instructions ?? "");
    const [error, setError] = useState("");
    const save = async () => {
        try {
            onSaved(await api.projectSave(project?.id ?? "", name, instructions));
        } catch (e) {
            setError(errorText(e));
        }
    };
    return (
        <div className="flex flex-col gap-3 rounded-2xl border border-secondary bg-primary_alt p-5 animate-in fade-in duration-200">
            <label className="flex flex-col gap-1.5 text-sm text-secondary">
                Project name
                <input aria-label="Project name" autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="Job hunt" className={field} />
            </label>
            <label className="flex flex-col gap-1.5 text-sm text-secondary">
                Instructions
                <textarea
                    aria-label="Project instructions"
                    rows={5}
                    value={instructions}
                    onChange={(e) => setInstructions(e.target.value)}
                    placeholder="What Relay should know in every chat here, e.g. use my CV in Documents and the Brave profile I'm signed in to."
                    className={cx(field, "resize-y leading-6")}
                />
            </label>
            {error && <Notice tone="error">{error}</Notice>}
            <div className="flex justify-end gap-2">
                <Button color="secondary" size="sm" onClick={onCancel}>
                    Cancel
                </Button>
                <Button size="sm" onClick={save} isDisabled={!name.trim()}>
                    {project ? "Save" : "Create project"}
                </Button>
            </div>
        </div>
    );
};

export const ProjectsScreen = ({
    projects,
    chats,
    onOpen,
    onChanged,
}: {
    projects: Project[];
    chats: ChatSummary[];
    onOpen: (id: string) => void;
    onChanged: () => void;
}) => {
    const [creating, setCreating] = useState(false);
    const [query, setQuery] = useState("");
    const q = query.trim().toLowerCase();
    const shown = projects.filter((p) => !q || p.name.toLowerCase().includes(q) || p.instructions.toLowerCase().includes(q));
    const lastUsed = (id: string) => Math.max(0, ...chats.filter((c) => c.project === id).map((c) => c.updated));
    return (
        <div className="h-full overflow-y-auto px-6 pt-12 pb-10">
            <div className="mx-auto flex max-w-[860px] flex-col gap-6">
                <div className="flex items-center justify-between">
                    <h1 className="font-serif text-[1.9rem] text-primary">Projects</h1>
                    <Button iconLeading={Plus} size="sm" onClick={() => setCreating(true)}>
                        New project
                    </Button>
                </div>
                <div className="flex h-10 items-center gap-2.5 rounded-xl border border-primary bg-primary_alt px-3 focus-within:border-[#5c5c5b]">
                    <SearchLg className="size-4 shrink-0 text-fg-tertiary" />
                    <input
                        aria-label="Search projects"
                        placeholder="Search projects…"
                        value={query}
                        onChange={(e) => setQuery(e.target.value)}
                        className="min-w-0 flex-1 bg-transparent text-sm text-primary outline-hidden placeholder:text-placeholder"
                    />
                </div>
                {creating && (
                    <ProjectForm
                        onCancel={() => setCreating(false)}
                        onSaved={(p) => {
                            setCreating(false);
                            onChanged();
                            onOpen(p.id);
                        }}
                    />
                )}
                {projects.length === 0 && !creating && (
                    <div className="flex flex-col items-center gap-2 rounded-2xl border border-dashed border-secondary px-6 py-14 text-center">
                        <Folder className="size-6 text-fg-quaternary" />
                        <p className="text-sm font-medium text-primary">No projects yet</p>
                        <p className="max-w-sm text-sm text-tertiary">
                            A project keeps related chats together, with instructions Relay follows in every one of them.
                        </p>
                    </div>
                )}
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                    {shown.map((p) => {
                        const count = chats.filter((c) => c.project === p.id).length;
                        const used = lastUsed(p.id) || p.created;
                        return (
                            <button
                                key={p.id}
                                onClick={() => onOpen(p.id)}
                                className="flex min-h-32 cursor-pointer flex-col gap-2 rounded-2xl border border-secondary bg-primary px-5 py-4 text-left transition duration-150 outline-focus-ring hover:border-primary hover:bg-primary_hover focus-visible:outline-2"
                            >
                                <span className="truncate text-[15px] font-medium text-primary">{p.name}</span>
                                <span className="line-clamp-2 flex-1 text-sm text-tertiary">{p.instructions || "No instructions"}</span>
                                <span className="text-xs text-quaternary">
                                    {count} {count === 1 ? "chat" : "chats"} · Updated {ago(used)}
                                </span>
                            </button>
                        );
                    })}
                </div>
            </div>
        </div>
    );
};

export const ProjectScreen = ({
    project,
    chats,
    onBack,
    onNewChat,
    onOpenChat,
    onChanged,
}: {
    project: Project;
    chats: ChatSummary[];
    onBack: () => void;
    onNewChat: () => void;
    onOpenChat: (id: string) => void;
    onChanged: () => void;
}) => {
    const [editing, setEditing] = useState(false);
    const [confirm, setConfirm] = useState(false);
    useEffect(() => {
        setEditing(false);
        setConfirm(false);
    }, [project.id]);
    const mine = chats.filter((c) => c.project === project.id);
    return (
        <div className="h-full overflow-y-auto px-6 pt-10 pb-10">
            <div className="mx-auto flex max-w-[960px] flex-col gap-6">
                <button onClick={onBack} className="flex w-fit cursor-pointer items-center gap-1.5 text-sm text-tertiary hover:text-primary">
                    <ArrowLeft className="size-4" />
                    All projects
                </button>
                <div className="grid grid-cols-1 gap-8 lg:grid-cols-[1fr_300px]">
                    <div className="flex min-w-0 flex-col gap-5">
                        <h1 className="truncate font-serif text-[1.9rem] text-primary">{project.name}</h1>
                        <button
                            onClick={onNewChat}
                            className="flex h-14 cursor-pointer items-center gap-3 rounded-2xl border border-primary bg-primary_alt px-4 text-left text-[15px] text-placeholder transition hover:border-[#5c5c5b]"
                        >
                            <Plus className="size-4 text-fg-tertiary" />
                            Start a chat in this project
                        </button>
                        <div className="flex flex-col">
                            {mine.length === 0 && <p className="py-6 text-sm text-tertiary">Chats you start here, or move here from their menu, show up in this list.</p>}
                            {mine.map((c) => (
                                <button
                                    key={c.id}
                                    onClick={() => onOpenChat(c.id)}
                                    className="flex cursor-pointer flex-col gap-0.5 border-b border-secondary py-3 text-left last:border-b-0 hover:bg-primary_hover"
                                >
                                    <span className="truncate px-2 text-sm font-medium text-primary">{c.title}</span>
                                    <span className="px-2 text-xs text-quaternary">Last message {ago(c.updated)}</span>
                                </button>
                            ))}
                        </div>
                    </div>
                    <div className="flex flex-col gap-3">
                        {editing ? (
                            <ProjectForm
                                project={project}
                                onCancel={() => setEditing(false)}
                                onSaved={() => {
                                    setEditing(false);
                                    onChanged();
                                }}
                            />
                        ) : (
                            <div className="flex flex-col gap-2 rounded-2xl border border-secondary p-4">
                                <div className="flex items-center justify-between">
                                    <span className="text-sm font-medium text-primary">Instructions</span>
                                    <button
                                        aria-label="Edit project"
                                        onClick={() => setEditing(true)}
                                        className="flex size-7 cursor-pointer items-center justify-center rounded-md text-fg-tertiary hover:bg-primary_hover hover:text-fg-secondary"
                                    >
                                        <Edit03 className="size-4" />
                                    </button>
                                </div>
                                <p className="text-sm whitespace-pre-wrap text-tertiary">
                                    {project.instructions || "Add instructions to tailor Relay's answers and tasks in this project."}
                                </p>
                            </div>
                        )}
                        {confirm ? (
                            <div className="flex flex-col gap-2 rounded-2xl border border-error p-4 text-sm">
                                <span className="text-primary">Delete "{project.name}"? Its chats stay, outside any project.</span>
                                <div className="flex justify-end gap-2">
                                    <Button size="sm" color="secondary" onClick={() => setConfirm(false)}>
                                        Cancel
                                    </Button>
                                    <Button
                                        size="sm"
                                        color="primary-destructive"
                                        onClick={async () => {
                                            await api.projectDelete(project.id).catch(() => {});
                                            onChanged();
                                            onBack();
                                        }}
                                    >
                                        Delete project
                                    </Button>
                                </div>
                            </div>
                        ) : (
                            <button
                                onClick={() => setConfirm(true)}
                                className="flex w-fit cursor-pointer items-center gap-1.5 px-1 text-sm text-tertiary hover:text-error-primary"
                            >
                                <Trash01 className="size-4" />
                                Delete project
                            </button>
                        )}
                    </div>
                </div>
            </div>
        </div>
    );
};
