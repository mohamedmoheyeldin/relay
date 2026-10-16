import { memo, useEffect, useLayoutEffect, useRef, useState } from "react";
import { DropdownMenu } from "radix-ui";
import { ArrowUp, FileText, Paperclip, Plus, Square, X } from "lucide-react";
import { selectBusy, useApp } from "../store/app";
import { api } from "../lib/backend";
import { cn, FolderIcon, Tip } from "./ui";

const baseName = (p: string) => p.split(/[\\/]/).pop() || p;

type Props = {
  projectId?: string | null;
  onClearProject?: () => void;
  placeholder?: string;
  autoFocus?: boolean;
  maxHeight?: number;
};

export const Composer = memo(function Composer({ projectId, onClearProject, placeholder = "Ask Relay to do anything", autoFocus, maxHeight = 240 }: Props) {
  const [text, setText] = useState("");
  const [files, setFiles] = useState<string[]>([]);
  const [expanded, setExpanded] = useState(false);
  const ta = useRef<HTMLTextAreaElement>(null);
  const busy = useApp(selectBusy);
  const project = useApp((s) => (projectId ? s.projects.find((p) => p.id === projectId) : undefined));
  const fill = useApp((s) => s.composerFill);
  const send = useApp((s) => s.send);
  const stop = useApp((s) => s.stop);

  // Suggestion chips and other places can prefill the composer.
  useEffect(() => {
    if (!fill) return;
    useApp.setState({ composerFill: null }); // consume it, so a later composer does not reuse it
    setText(fill.text);
    requestAnimationFrame(() => {
      const el = ta.current;
      if (!el) return;
      el.focus();
      el.setSelectionRange(el.value.length, el.value.length);
    });
  }, [fill]);

  useEffect(() => {
    if (autoFocus) ta.current?.focus();
  }, [autoFocus]);

  // Auto-grow; switch to the two-row layout once the text wraps (and stay there until cleared).
  useLayoutEffect(() => {
    const el = ta.current;
    if (!el) return;
    el.style.height = "0px";
    const h = el.scrollHeight;
    el.style.height = `${Math.min(h, maxHeight)}px`;
    el.style.overflowY = h > maxHeight ? "auto" : "hidden";
    const cs = getComputedStyle(el);
    const lines = (h - parseFloat(cs.paddingTop) - parseFloat(cs.paddingBottom)) / (parseFloat(cs.lineHeight) || 24);
    if (!text) setExpanded(false);
    else if (!expanded && (lines > 1.5 || text.includes("\n"))) setExpanded(true);
  }, [text, expanded, maxHeight]);

  const canSend = text.trim().length > 0 && !busy;

  const submit = () => {
    if (!canSend) return;
    const body = text;
    const att = files;
    setText("");
    setFiles([]);
    void send(body, att, projectId ?? null);
  };

  const addFiles = async () => {
    try {
      const picked = await api.pickFiles();
      if (picked.length) setFiles((f) => [...f, ...picked.filter((p) => !f.includes(p))]);
    } catch (e) {
      useApp.getState().toast(`Could not open the file picker: ${e instanceof Error ? e.message : String(e)}`, "error");
    }
    ta.current?.focus();
  };

  const showChip = Boolean(project && onClearProject); // the project page already says where the chat goes
  const twoRow = expanded || files.length > 0 || showChip;

  const plus = (
    <DropdownMenu.Root>
      <Tip label="Add files and more" side="top">
        <DropdownMenu.Trigger asChild>
          <button
            type="button"
            aria-label="Add files and more"
            className="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-full text-fg hover:bg-hover data-[state=open]:bg-hover"
          >
            <Plus size={20} />
          </button>
        </DropdownMenu.Trigger>
      </Tip>
      <DropdownMenu.Portal>
        <DropdownMenu.Content className="menu pop-in" side="top" align="start" sideOffset={8}>
          <DropdownMenu.Item className="menu-item" onSelect={() => void addFiles()}>
            <Paperclip size={16} /> Add photos and files
          </DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );

  const action = busy ? (
    <Tip label="Stop" side="top">
      <button
        type="button"
        onClick={stop}
        aria-label="Stop"
        className="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-send text-send-fg transition-opacity hover:opacity-85"
      >
        <Square size={13} fill="currentColor" />
      </button>
    </Tip>
  ) : (
    <Tip label="Send" kbd="Enter" side="top">
      <button
        type="button"
        onClick={submit}
        disabled={!canSend}
        aria-label="Send message"
        className="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-send text-send-fg transition-[opacity,transform] duration-150 hover:opacity-85 active:scale-95 disabled:opacity-25"
      >
        <ArrowUp size={19} strokeWidth={2.4} />
      </button>
    </Tip>
  );

  const textarea = (
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
        }
      }}
      className={cn(
        "block w-full resize-none bg-transparent text-[16px] leading-6 outline-none placeholder:text-subtle",
        twoRow ? "px-1 pt-1" : "py-1.5",
      )}
    />
  );

  return (
    <div
      className={cn(
        "grid w-full cursor-text items-center rounded-[28px] border border-line bg-surface shadow-[0_1px_2px_rgba(0,0,0,0.04)] transition-[border-color] duration-150 focus-within:border-line-strong",
        twoRow
          ? "grid-cols-[auto_1fr_auto] gap-x-1 gap-y-2 p-2.5 pl-3 [grid-template-areas:'files_files_files''input_input_input''plus_extra_action']"
          : "grid-cols-[auto_1fr_auto] gap-x-1.5 p-2 [grid-template-areas:'plus_input_action']",
      )}
      onClick={(e) => {
        if (e.target === e.currentTarget) ta.current?.focus();
      }}
    >
      {files.length > 0 && (
        <ul className="flex flex-wrap gap-2 px-0.5 pt-0.5 [grid-area:files]" aria-label="Attached files">
          {files.map((f) => (
            <li key={f} className="group flex max-w-[240px] items-center gap-2 rounded-xl border border-line bg-bg py-1.5 pr-1.5 pl-2.5 text-[13px]">
              <FileText size={15} className="shrink-0 text-accent" aria-hidden="true" />
              <span className="truncate" title={f}>
                {baseName(f)}
              </span>
              <button
                type="button"
                aria-label={`Remove ${baseName(f)}`}
                onClick={() => setFiles((x) => x.filter((y) => y !== f))}
                className="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-muted hover:bg-hover hover:text-fg"
              >
                <X size={13} />
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="min-w-0 [grid-area:input]">{textarea}</div>
      <div className="[grid-area:plus]">{plus}</div>
      {twoRow && (
        <div className="min-w-0 [grid-area:extra]">
          {project && showChip && (
            <span className="ml-1 inline-flex h-8 max-w-[220px] items-center gap-1.5 rounded-full border border-line px-2.5 text-[13px] text-muted">
              <FolderIcon color={project.color} size={14} />
              <span className="truncate">{project.name}</span>
              {onClearProject && (
                <button type="button" aria-label="Remove project" onClick={onClearProject} className="-mr-1 inline-flex h-5 w-5 items-center justify-center rounded-full hover:bg-hover hover:text-fg">
                  <X size={12} />
                </button>
              )}
            </span>
          )}
        </div>
      )}
      <div className="justify-self-end [grid-area:action]">{action}</div>
    </div>
  );
});