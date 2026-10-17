import { useEffect, useState } from "react";
import { Check } from "lucide-react";
import { useApp } from "@/store/app";
import { FOLDER_COLORS, PROJECT_COLORS } from "@/lib/theme";
import { useCreateProject, useUpdateProject } from "@/hooks/queries";
import { go, paths } from "@/lib/nav";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { FolderIcon } from "@/components/common";

export function ProjectDialog() {
  const state = useApp((s) => s.projectDialog);
  const close = () => useApp.getState().setProjectDialog(null);
  const editing = state?.mode === "edit" ? state.project : null;
  const [name, setName] = useState("");
  const [color, setColor] = useState("blue");
  const [instructions, setInstructions] = useState("");
  const create = useCreateProject();
  const update = useUpdateProject();

  useEffect(() => {
    if (!state) return;
    setName(editing?.name ?? "");
    setColor(editing?.color ?? PROJECT_COLORS[Math.floor(Math.random() * PROJECT_COLORS.length)]!);
    setInstructions(editing?.instructions ?? "");
  }, [state, editing]);

  const saving = create.isPending || update.isPending;
  const submit = () => {
    const n = name.trim();
    if (!n || saving) return;
    if (editing) {
      update.mutate({ id: editing.id, patch: { name: n, color, instructions } });
      close();
    } else {
      create.mutate(
        { name: n, color, instructions },
        {
          onSuccess: (p) => {
            close();
            go(paths.project(p.id));
          },
        },
      );
    }
  };

  return (
    <Dialog open={state !== null} onOpenChange={(o) => !o && close()}>
      <DialogContent className="w-[min(500px,calc(100vw-24px))]">
        <DialogHeader>
          <DialogTitle>{editing ? "Edit project" : "New project"}</DialogTitle>
          <DialogDescription>Projects keep related chats together, with instructions Relay follows in each of them.</DialogDescription>
        </DialogHeader>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            submit();
          }}
          className="space-y-5"
        >
          <div className="space-y-1.5">
            <Label htmlFor="project-name">Name</Label>
            <div className="relative">
              <span className="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2">
                <FolderIcon color={color} size={16} />
              </span>
              <Input id="project-name" autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. Home PC" className="pl-9" maxLength={80} />
            </div>
          </div>
          <fieldset>
            <legend className="mb-2 text-ui font-medium">Colour</legend>
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
                    "inline-flex size-7 items-center justify-center rounded-full transition-transform duration-(--relay-dur-fast) hover:scale-105",
                    color === c && "ring-2 ring-fg ring-offset-2 ring-offset-raised",
                  )}
                  style={{ background: FOLDER_COLORS[c] }}
                >
                  {color === c && <Check className="size-3.5 text-white" strokeWidth={3} aria-hidden="true" />}
                </button>
              ))}
            </div>
          </fieldset>
          <div className="space-y-1.5">
            <Label htmlFor="project-instructions">Instructions</Label>
            <Textarea
              id="project-instructions"
              value={instructions}
              onChange={(e) => setInstructions(e.target.value)}
              rows={5}
              placeholder="How should Relay work in this project? e.g. Use D:\Photos, never delete originals."
            />
          </div>
          <DialogFooter>
            <Button variant="ghost" onClick={close}>
              Cancel
            </Button>
            <Button variant="primary" type="submit" disabled={!name.trim() || saving}>
              {editing ? "Save" : "Create project"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function RenameDialog({ open, initial, onClose, onRename }: { open: boolean; initial: string; onClose: () => void; onRename: (t: string) => void }) {
  const [title, setTitle] = useState(initial);
  useEffect(() => {
    if (open) setTitle(initial);
  }, [open, initial]);
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="w-[min(420px,calc(100vw-24px))]" aria-describedby={undefined}>
        <DialogHeader>
          <DialogTitle>Rename chat</DialogTitle>
        </DialogHeader>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            if (title.trim()) onRename(title.trim());
            onClose();
          }}
          className="space-y-5"
        >
          <Input autoFocus value={title} onChange={(e) => setTitle(e.target.value)} onFocus={(e) => e.currentTarget.select()} aria-label="Title" maxLength={120} />
          <DialogFooter>
            <Button variant="ghost" onClick={onClose}>
              Cancel
            </Button>
            <Button variant="primary" type="submit" disabled={!title.trim()}>
              Rename
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function ConfirmDialog() {
  const d = useApp((s) => s.confirmDialog);
  const close = () => useApp.getState().setConfirmDialog(null);
  return (
    <AlertDialog open={d !== null} onOpenChange={(o) => !o && close()}>
      <AlertDialogContent data-testid="confirm-dialog">
        <AlertDialogHeader>
          <AlertDialogTitle>{d?.title}</AlertDialogTitle>
          <AlertDialogDescription>{d?.body}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel asChild>
            <Button variant="secondary">Cancel</Button>
          </AlertDialogCancel>
          <AlertDialogAction asChild>
            <Button
              variant={d?.destructive ? "danger" : "primary"}
              onClick={() => {
                d?.onConfirm();
                close();
              }}
            >
              {d?.action ?? "OK"}
            </Button>
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
