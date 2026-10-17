import { memo, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { ArrowUp, FileText, Paperclip, Square, X } from "lucide-react";
import { useApp } from "@/store/app";
import { api, errorText } from "@/lib/backend";
import { useBusy, useProjects } from "@/hooks/queries";
import { cn } from "@/lib/utils";
import { FolderIcon } from "@/components/common";
import { IconButton } from "@/components/ui/icon-button";
import { Tip } from "@/components/ui/tooltip";

const baseName = (p: string) => p.split(/[\\/]/).pop() || p;

type Props = {
  convId?: string | null;
  projectId?: string | null;
  onClearProject?: () => void;
  placeholder?: string;
  autoFocus?: boolean;
  maxHeight?: number;
  /** Extra controls in the bottom-left of the card (e.g. the project picker on Home). */
  leading?: ReactNode;
};

export const Composer = memo(function Composer({ convId, projectId, onClearProject, placeholder = "Ask Relay to do something on this PC", autoFocus, maxHeight = 240, leading }: Props) {
  const [text, setText] = useState("");
  const [files, setFiles] = useState<string[]>([]);
  const ta = useRef<HTMLTextAreaElement>(null);
  const busy = useBusy();
  const projects = useProjects();
  const project = projectId ? projects.data?.find((p) => p.id === projectId) : undefined;
  const fill = useApp((s) => s.composerFill);
  const focusNonce = useApp((s) => s.composerFocus);
  const send = useApp((s) => s.send);
  const stop = useApp((s) => s.stop);

  // Suggestion chips and other places can prefill the composer.
  useEffect(() => {
    if (!fill) return;
    useApp.setState({ composerFill: null });
    setText(fill.text);
    requestAnimationFrame(() => {
      const el = ta.current;
      if (!el) return;
      el.focus();
      el.setSelectionRange(el.value.length, el.value.length);
    });
  }, [fill]);

  useEffect(() => {
    if (autoFocus || focusNonce) ta.current?.focus();
  }, [autoFocus, focusNonce]);

  // Auto-grow up to maxHeight.
  useLayoutEffect(() => {
    const el = ta.current;
    if (!el) return;
    el.style.height = "0px";
    const h = el.scrollHeight;
    el.style.height = `${Math.min(Math.max(h, 24), maxHeight)}px`;
    el.style.overflowY = h > maxHeight ? "auto" : "hidden";
  }, [text, maxHeight]);

  const canSend = text.trim().length > 0 && !busy;

  const submit = () => {
    if (!canSend) return;
    const body = text;
    const att = files;
    setText("");
    setFiles([]);
    void send(body, att, { projectId: projectId ?? null, convId: convId ?? null });
  };

  const addFiles = async () => {
    try {
      const picked = await api.pickFiles();
      if (picked.length) setFiles((f) => [...f, ...picked.filter((p) => !f.includes(p))]);
    } catch (e) {
      useApp.getState().toast(`Could not open the file picker: ${errorText(e)}`, "error");
    }
    ta.current?.focus();
  };

  return (
    <div
      className="w-full cursor-text rounded-composer border border-line bg-surface shadow-card transition-[border-color,box-shadow] duration-(--relay-dur) focus-within:border-line-strong/60"
      onClick={(e) => {
        if (e.target === e.currentTarget) ta.current?.focus();
      }}
      data-testid="composer"
    >
      {files.length > 0 && (
        <ul className="flex flex-wrap gap-2 px-3 pt-3" aria-label="Attached files">
          {files.map((f) => (
            <li key={f} className="flex max-w-[240px] items-center gap-2 rounded-md border border-line bg-sunk py-1 pr-1 pl-2.5 text-caption">
              <FileText className="size-3.5 shrink-0 text-fg-muted" aria-hidden="true" />
              <span className="truncate" title={f}>
                {baseName(f)}
              </span>
              <button
                type="button"
                aria-label={`Remove ${baseName(f)}`}
                onClick={() => setFiles((x) => x.filter((y) => y !== f))}
                className="inline-flex size-5 shrink-0 items-center justify-center rounded-xs text-fg-muted hover:bg-hover hover:text-fg"
              >
                <X className="size-3" aria-hidden="true" />
              </button>
            </li>
          ))}
        </ul>
      )}
      <textarea
        ref={ta}
        rows={1}
        value={text}
        placeholder={placeholder}
        aria-label="Message Relay"
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing && e.keyCode !== 229) {
            e.preventDefault();
            submit();
          } else if (e.key === "Escape" && busy) {
            e.preventDefault();
            stop();
          }
        }}
        className="block w-full resize-none bg-transparent px-4 pt-3.5 pb-1 text-body text-fg outline-none placeholder:text-fg-muted"
        style={{ minHeight: 24 }}
      />
      <div className="flex items-center gap-1.5 px-2.5 pt-1 pb-2.5">
        <IconButton label="Attach files" size="icon-lg" className="rounded-full" tipSide="top" onClick={() => void addFiles()}>
          <Paperclip aria-hidden="true" />
        </IconButton>
        {leading}
        {project && (
          <span className="inline-flex h-8 max-w-[220px] items-center gap-1.5 rounded-full border border-line px-2.5 text-caption text-fg-2" data-testid="project-chip">
            <FolderIcon color={project.color} size={14} />
            <span className="truncate">{project.name}</span>
            {onClearProject && (
              <button
                type="button"
                aria-label={`Don't put this chat in ${project.name}`}
                onClick={onClearProject}
                className="-mr-1 inline-flex size-5 items-center justify-center rounded-full text-fg-muted hover:bg-hover hover:text-fg"
              >
                <X className="size-3" aria-hidden="true" />
              </button>
            )}
          </span>
        )}
        <span className="flex-1" />
        {busy ? (
          <Tip label="Stop the task" shortcut="Esc" side="top">
            <button
              type="button"
              onClick={stop}
              aria-label="Stop the task"
              className="inline-flex size-9 shrink-0 items-center justify-center rounded-full bg-fg text-bg transition-opacity hover:opacity-85"
            >
              <Square className="size-3.5 fill-current" aria-hidden="true" />
            </button>
          </Tip>
        ) : (
          <Tip label="Send" shortcut="Enter" side="top">
            <button
              type="button"
              onClick={submit}
              disabled={!canSend}
              aria-label="Send message"
              className={cn(
                "inline-flex size-9 shrink-0 items-center justify-center rounded-full bg-brand text-on-brand transition-[background-color,opacity] duration-(--relay-dur-fast) hover:bg-brand-hover",
                "disabled:bg-active disabled:text-fg-muted",
              )}
            >
              <ArrowUp className="size-[18px]" strokeWidth={2.4} aria-hidden="true" />
            </button>
          </Tip>
        )}
      </div>
    </div>
  );
});
