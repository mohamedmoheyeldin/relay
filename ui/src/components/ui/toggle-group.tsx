import * as React from "react";
import { ToggleGroup as ToggleGroupPrimitive } from "radix-ui";
import { cn } from "@/lib/utils";

// Segmented control built on Radix ToggleGroup (single): one tab stop, arrow keys move between
// options (roving tabindex). Exposed as a radio group to assistive technology.
function Segmented<T extends string>({
  value,
  onValueChange,
  options,
  label,
  size = "md",
  className,
  disabled,
}: {
  value: T | undefined;
  onValueChange: (v: T) => void;
  options: { value: T; label: React.ReactNode; icon?: React.ReactNode; disabled?: boolean; title?: string }[];
  label: string;
  size?: "sm" | "md";
  className?: string;
  disabled?: boolean;
}) {
  return (
    <ToggleGroupPrimitive.Root
      type="single"
      role="radiogroup"
      aria-label={label}
      value={value ?? ""}
      disabled={disabled}
      onValueChange={(v) => v && onValueChange(v as T)}
      className={cn("inline-flex items-center gap-0.5 rounded-lg border border-line bg-sunk p-0.5", className)}
    >
      {options.map((o) => (
        <ToggleGroupPrimitive.Item
          key={o.value}
          value={o.value}
          role="radio"
          aria-checked={value === o.value}
          disabled={o.disabled}
          title={o.title}
          className={cn(
            "inline-flex items-center justify-center gap-1.5 rounded-md font-medium whitespace-nowrap text-fg-2 outline-none transition-[background-color,color,box-shadow] duration-(--relay-dur-fast)",
            "hover:text-fg data-[state=on]:bg-surface data-[state=on]:text-fg data-[state=on]:shadow-sm disabled:opacity-45",
            "focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-focus [&_svg]:size-3.5 [&_svg]:shrink-0",
            size === "sm" ? "h-6 px-2 text-caption" : "h-7 px-3 text-ui",
          )}
        >
          {o.icon}
          {o.label}
        </ToggleGroupPrimitive.Item>
      ))}
    </ToggleGroupPrimitive.Root>
  );
}

export { Segmented };
