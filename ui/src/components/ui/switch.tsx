import * as React from "react";
import { Switch as SwitchPrimitive } from "radix-ui";
import { cn } from "@/lib/utils";

/** Toggle switch: brand track when on; the off track has a 3:1 boundary (line-strong). */
function Switch({ className, ...props }: React.ComponentProps<typeof SwitchPrimitive.Root>) {
  return (
    <SwitchPrimitive.Root
      data-slot="switch"
      className={cn(
        "peer relative inline-flex h-5 w-9 shrink-0 items-center rounded-full border border-line-strong bg-sunk transition-colors duration-(--relay-dur-fast) ease-relay",
        "data-checked:border-brand data-checked:bg-brand disabled:cursor-not-allowed disabled:opacity-45",
        className,
      )}
      {...props}
    >
      <SwitchPrimitive.Thumb
        data-slot="switch-thumb"
        className={cn(
          "pointer-events-none block size-3.5 translate-x-[2px] rounded-full bg-fg-muted shadow-sm transition-transform duration-(--relay-dur-fast) ease-relay",
          "data-checked:translate-x-[18px] data-checked:bg-on-brand",
        )}
      />
    </SwitchPrimitive.Root>
  );
}

export { Switch };
