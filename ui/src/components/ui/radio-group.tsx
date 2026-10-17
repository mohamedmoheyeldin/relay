import * as React from "react";
import { RadioGroup as RadioGroupPrimitive } from "radix-ui";
import { cn } from "@/lib/utils";

/** Radio group (roving focus + arrow keys from Radix). */
function RadioGroup({ className, ...props }: React.ComponentProps<typeof RadioGroupPrimitive.Root>) {
  return <RadioGroupPrimitive.Root data-slot="radio-group" className={cn("grid gap-2", className)} {...props} />;
}

function RadioGroupItem({ className, ...props }: React.ComponentProps<typeof RadioGroupPrimitive.Item>) {
  return (
    <RadioGroupPrimitive.Item
      data-slot="radio-group-item"
      className={cn(
        "relative inline-flex size-4 shrink-0 items-center justify-center rounded-full border border-line-strong bg-surface outline-none transition-colors",
        "data-checked:border-brand disabled:cursor-not-allowed disabled:opacity-45",
        className,
      )}
      {...props}
    >
      <RadioGroupPrimitive.Indicator data-slot="radio-group-indicator" className="size-2 rounded-full bg-brand" />
    </RadioGroupPrimitive.Item>
  );
}

/** A large selectable card that behaves as a radio item (used for privacy modes). */
function RadioCard({ className, children, ...props }: React.ComponentProps<typeof RadioGroupPrimitive.Item>) {
  return (
    <RadioGroupPrimitive.Item
      data-slot="radio-card"
      className={cn(
        "group relative flex w-full flex-col items-start gap-1 rounded-lg border border-line bg-surface p-4 text-left outline-none transition-[border-color,background-color,box-shadow] duration-(--relay-dur-fast)",
        "hover:border-line-strong/70 data-checked:border-brand data-checked:bg-brand-soft/40 data-checked:shadow-[inset_0_0_0_1px_var(--relay-accent)] disabled:opacity-50",
        className,
      )}
      {...props}
    >
      {children}
    </RadioGroupPrimitive.Item>
  );
}

export { RadioGroup, RadioGroupItem, RadioCard };
