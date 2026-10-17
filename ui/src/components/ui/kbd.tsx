import * as React from "react";
import { cn } from "@/lib/utils";
import { usePlatform } from "@/hooks/queries";
import { shortcutLabel, shortcutParts, type Keys } from "@/lib/platform";

function Kbd({ className, ...props }: React.ComponentProps<"kbd">) {
  return (
    <kbd
      data-slot="kbd"
      className={cn(
        "inline-flex h-5 min-w-5 items-center justify-center rounded-xs border border-line bg-surface px-1 font-sans text-micro font-medium text-fg-2 shadow-[0_1px_0_var(--relay-border)] select-none",
        className,
      )}
      {...props}
    />
  );
}

/** Platform-aware shortcut keys: Ctrl on Windows, the Command symbol on macOS. */
function Shortcut({ keys, className }: { keys: Keys; className?: string }) {
  const { os } = usePlatform();
  return (
    <span className={cn("inline-flex items-center gap-0.5", className)}>
      <span className="sr-only">{shortcutLabel(keys, os)}</span>
      {shortcutParts(keys, os).map((k, i) => (
        <Kbd key={`${k}-${i}`} aria-hidden="true">
          {k}
        </Kbd>
      ))}
    </span>
  );
}

/** Hook returning a string label for tooltips ("Ctrl+K" / "⌘K"). */
function useShortcutLabel(keys: Keys): string {
  const { os } = usePlatform();
  return shortcutLabel(keys, os);
}

export { Kbd, Shortcut, useShortcutLabel };
