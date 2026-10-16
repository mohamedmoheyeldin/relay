import { X } from "lucide-react";
import { useApp } from "../store/app";
import { cn } from "./ui";

export function Toasts() {
  const toasts = useApp((s) => s.toasts);
  const dismiss = useApp((s) => s.dismissToast);
  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-24 z-[100] flex flex-col items-center gap-2 px-4" aria-live="polite">
      {toasts.map((t) => (
        <div
          key={t.id}
          role={t.tone === "error" ? "alert" : "status"}
          className={cn(
            "toast-in pointer-events-auto flex max-w-[min(520px,100%)] items-center gap-3 rounded-xl px-4 py-2.5 text-[14px] shadow-[var(--shadow)]",
            t.tone === "error" ? "bg-[#3a1d1d] text-[#ffd9d9]" : "bg-elevated text-fg",
          )}
        >
          <span className="min-w-0 flex-1 break-words">{t.text}</span>
          <button type="button" aria-label="Dismiss" onClick={() => dismiss(t.id)} className="-mr-1.5 inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md opacity-70 hover:opacity-100">
            <X size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
