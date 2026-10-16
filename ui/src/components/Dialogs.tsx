import { useEffect, useState, type ReactNode } from "react";
import { AlertDialog, Dialog } from "radix-ui";
import { Check, ShieldAlert, X } from "lucide-react";
import { useApp } from "../store/app";
import { FOLDER_COLORS, PROJECT_COLORS } from "../lib/theme";
import { useNow } from "./hooks";
import { Button, cn, FolderIcon } from "./ui";

const panel =
  "dialog-in fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 z-[91] w-[min(480px,calc(100vw-24px))] max-h-[calc(100vh-32px)] overflow-y-auto rounded-[20px] border border-line bg-elevated p-6 shadow-[var(--shadow)] focus:outline-none";
const overlay = "fade-in fixed inset-0 z-[90] bg-[var(--scrim)]";

export function DialogShell({
  open,
  onOpenChange,
  title,
  description,
  children,
  wide,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  title: string;
  description?: string;
  children: ReactNode;
  wide?: boolean;
}) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className={overlay} />
        <Dialog.Content className={cn(panel, wide && "w-[min(640px,calc(100vw-24px))]")} {...(description ? {} : { "aria-describedby": undefined })}>
          <div className="mb-4 flex items-start justify-between gap-4">
            <div>
              <Dialog.Title className="text-[18px] leading-7 font-semibold">{title}</Dialog.Title>
              {description && <Dialog.Description className="mt-1 text-[14px] text-muted">{description}</Dialog.Description>}
            </div>
            <Dialog.Close className="-mt-1 -mr-2 inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-muted hover:bg-hover hover:text-fg" aria-label="Close">
              <X size={18} />
            </Dialog.Close>
          </div>
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export const fieldClass =
  "w-full rounded-xl border border-line-strong bg-transparent px-3 py-2 text-[14.5px] outline-none transition-colors focus:border-accent";

export function ProjectDialog() {
  const state = useApp((s) => s.projectDialog);
  const close = () => useApp.getState().setProjectDialog(null);
  const editing = state?.mode === "edit" ? state.project : null;
  const [name, setName] = useState("");
  const [color, setColor] = useState("blue");
  const [instructions, setInstructions] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!state) return;
    setName(editing?.name ?? "");
    setColor(editing?.color ?? PROJECT_COLORS[Math.floor(Math.random() * PROJECT_COLORS.length)]!);
    setInstructions(editing?.instructions ?? "");
    setSaving(false);
  }, [state, editing]);

  const submit = async () => {
    const n = name.trim();
    if (!n || saving) return;
    setSaving(true);
    const s = useApp.getState();
    if (editing) {
      await s.updateProject(editing.id, { name: n, color, instructions });
      close();
    } else {
      const p = await s.createProject(n, color, instructions);
      close();
      if (p) s.go({ kind: "project", id: p.id });
    }
  };

  return (
    <DialogShell
      open={state !== null}
      onOpenChange={(o) => !o && close()}
      title={editing ? "Edit project" : "New project"}
      description={editing ? undefined : "Projects keep chats, and instructions for Relay, in one place."}
    >
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
        className="space-y-5"
      >
        <label className="block">
          <span className="mb-1.5 block text-[13px] font-medium text-muted">Name</span>
          <div className="relative">
            <span className="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2">
              <FolderIcon color={color} size={17} />
            </span>
            <input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. Home PC" className={cn(fieldClass, "pl-10")} maxLength={80} />
          </div>
        </label>
        <fieldset>
          <legend className="mb-2 text-[13px] font-medium text-muted">Colour</legend>
          <div className="flex flex-wrap gap-2.5" role="radiogroup" aria-label="Colour">
            {PROJECT_COLORS.map((c) => (
              <button
                key={c}
                type="button"
                role="radio"
                aria-checked={color === c}
                aria-label={c}
                onClick={() => setColor(c)}
                className={cn(
                  "inline-flex h-8 w-8 items-center justify-center rounded-full transition-transform duration-150 hover:scale-105",
                  color === c && "ring-2 ring-fg ring-offset-2 ring-offset-[var(--elevated)]",
                )}
                style={{ background: FOLDER_COLORS[c] }}
              >
                {color === c && <Check size={15} className="text-white" strokeWidth={3} />}
              </button>
            ))}
          </div>
        </fieldset>
        <label className="block">
          <span className="mb-1.5 block text-[13px] font-medium text-muted">Instructions</span>
          <textarea
            value={instructions}
            onChange={(e) => setInstructions(e.target.value)}
            rows={5}
            placeholder="How should Relay work in this project? e.g. Use D:\Photos, never delete originals."
            className={cn(fieldClass, "scroll-thin resize-y leading-6")}
          />
        </label>
        <div className="flex justify-end gap-2 pt-1">
          <Button variant="ghost" onClick={close}>
            Cancel
          </Button>
          <Button variant="primary" type="submit" disabled={!name.trim() || saving}>
            {editing ? "Save" : "Create project"}
          </Button>
        </div>
      </form>
    </DialogShell>
  );
}

export function RenameDialog() {
  const state = useApp((s) => s.renameDialog);
  const [title, setTitle] = useState("");
  useEffect(() => {
    if (state) setTitle(state.title);
  }, [state]);
  const close = () => useApp.getState().setRenameDialog(null);
  return (
    <DialogShell open={state !== null} onOpenChange={(o) => !o && close()} title="Rename chat">
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (state && title.trim()) useApp.getState().renameConv(state.id, title);
          close();
        }}
        className="space-y-5"
      >
        <input autoFocus value={title} onChange={(e) => setTitle(e.target.value)} onFocus={(e) => e.currentTarget.select()} className={fieldClass} aria-label="Title" maxLength={120} />
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={close}>
            Cancel
          </Button>
          <Button variant="primary" type="submit" disabled={!title.trim()}>
            Rename
          </Button>
        </div>
      </form>
    </DialogShell>
  );
}

export function ConfirmDialog() {
  const d = useApp((s) => s.confirmDialog);
  const close = () => useApp.getState().setConfirmDialog(null);
  return (
    <AlertDialog.Root open={d !== null} onOpenChange={(o) => !o && close()}>
      <AlertDialog.Portal>
        <AlertDialog.Overlay className={overlay} />
        <AlertDialog.Content className={cn(panel, "w-[min(420px,calc(100vw-24px))]")}>
          <AlertDialog.Title className="text-[18px] leading-7 font-semibold">{d?.title}</AlertDialog.Title>
          <AlertDialog.Description className="mt-2 text-[14.5px] leading-6 text-muted">{d?.body}</AlertDialog.Description>
          <div className="mt-6 flex justify-end gap-2">
            <AlertDialog.Cancel asChild>
              <Button variant="secondary">Cancel</Button>
            </AlertDialog.Cancel>
            <AlertDialog.Action asChild>
              <Button
                variant={d?.destructive ? "danger" : "primary"}
                onClick={() => {
                  d?.onConfirm();
                  close();
                }}
              >
                {d?.action ?? "OK"}
              </Button>
            </AlertDialog.Action>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}

/** Approval request from a running task (`confirm` event). Blocks until answered or timed out. */
export function ApprovalDialog() {
  const req = useApp((s) => s.confirms[0] ?? null);
  const more = useApp((s) => Math.max(0, s.confirms.length - 1));
  const now = useNow(req !== null, 1000);
  const answer = useApp((s) => s.answerConfirm);
  const left = req ? Math.max(0, 120 - Math.floor((now - req.at) / 1000)) : 0;
  const destructive = req?.risk === "destructive";
  return (
    <AlertDialog.Root open={req !== null}>
      <AlertDialog.Portal>
        <AlertDialog.Overlay className={overlay} />
        <AlertDialog.Content
          className={cn(panel, "w-[min(460px,calc(100vw-24px))]", destructive && "border-[color-mix(in_srgb,var(--danger)_45%,transparent)]")}
          onEscapeKeyDown={(e) => {
            e.preventDefault();
            if (req) answer(req.id, false);
          }}
        >
          <div className="flex gap-3.5">
            <span
              className={cn("inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-full", destructive ? "bg-danger-soft text-danger" : "bg-accent-soft text-accent")}
              aria-hidden="true"
            >
              <ShieldAlert size={20} />
            </span>
            <div className="min-w-0">
              <div className={cn("text-[12px] font-medium tracking-wide uppercase", destructive ? "text-danger" : "text-accent")}>
                {destructive ? "Needs your approval · can't be undone" : "Needs your approval"}
              </div>
              <AlertDialog.Title className="mt-0.5 text-[17px] leading-6 font-semibold break-words">{req?.title}</AlertDialog.Title>
              {req?.detail && (
                <AlertDialog.Description className="mt-2 max-h-48 overflow-y-auto rounded-xl bg-hover px-3 py-2 font-mono text-[12.5px] leading-5 break-words whitespace-pre-wrap text-muted scroll-thin">
                  {req.detail}
                </AlertDialog.Description>
              )}
            </div>
          </div>
          <div className="mt-6 flex items-center gap-2">
            <span className="text-[12.5px] text-subtle">
              Declines automatically in {left}s{more > 0 ? ` · ${more} more waiting` : ""}
            </span>
            <span className="flex-1" />
            <AlertDialog.Cancel asChild>
              <Button variant="secondary" onClick={() => req && answer(req.id, false)}>
                Decline
              </Button>
            </AlertDialog.Cancel>
            <AlertDialog.Action asChild>
              <Button variant={destructive ? "danger" : "primary"} onClick={() => req && answer(req.id, true)}>
                Approve
              </Button>
            </AlertDialog.Action>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
