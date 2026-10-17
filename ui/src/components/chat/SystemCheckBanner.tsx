// Home: a small banner when Relay Local AI, Node.js or browser control needs attention, linking to
// Settings > System check. Dismissing it hides it until the set of problems changes.
import { useState } from "react";
import { TriangleAlert, X } from "lucide-react";
import { useSystemCheck } from "@/hooks/queries";
import { attentionItems, attentionText, dismissBanner, dismissedKey, problemKey } from "@/lib/systemCheck";
import { go, paths } from "@/lib/nav";
import { IconButton } from "@/components/ui/icon-button";

export function SystemCheckBanner() {
  const q = useSystemCheck();
  const items = attentionItems(q.data);
  const key = problemKey(items);
  const [dismissed, setDismissed] = useState<string | null>(() => dismissedKey());
  if (!items.length || dismissed === key) return null;
  const text = attentionText(items.length);
  return (
    <div role="status" className="mb-6 flex w-full max-w-[560px] items-center gap-2 rounded-lg bg-warning-soft py-1.5 pr-1.5 pl-3 text-caption text-fg" data-testid="system-check-banner">
      <TriangleAlert className="size-4 shrink-0 text-warning" aria-hidden="true" />
      <span className="min-w-0 flex-1">
        <span className="font-medium">{text}</span>
        <span className="text-fg-2"> · {items.map((i) => i.label).join(", ")}</span>
      </span>
      <button type="button" onClick={() => go(paths.settings("system-check"))} className="h-7 shrink-0 rounded-md px-2 font-medium text-fg underline underline-offset-2 hover:bg-hover">
        Review
      </button>
      <IconButton
        label="Dismiss"
        size="icon-sm"
        onClick={() => {
          dismissBanner(key);
          setDismissed(key);
        }}
      >
        <X aria-hidden="true" />
      </IconButton>
    </div>
  );
}
