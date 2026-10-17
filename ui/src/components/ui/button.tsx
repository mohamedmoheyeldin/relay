import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { Slot } from "radix-ui";
import { cn } from "@/lib/utils";

// shadcn Button adapted to Relay tokens: brand fill only for the one primary action per view,
// hairline secondary, quiet ghost; 8px radius; visible 2px focus ring from the base layer.
const buttonVariants = cva(
  "inline-flex shrink-0 items-center justify-center gap-1.5 rounded-md border border-transparent font-medium whitespace-nowrap select-none transition-[background-color,border-color,color,opacity] duration-(--relay-dur-fast) ease-relay disabled:pointer-events-none disabled:opacity-45 aria-invalid:border-danger [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
  {
    variants: {
      variant: {
        primary: "bg-brand text-on-brand hover:bg-brand-hover",
        secondary: "border-line bg-surface text-fg shadow-sm hover:bg-hover hover:border-line-strong/60 aria-expanded:bg-hover",
        outline: "border-line-strong/70 bg-transparent text-fg hover:bg-hover aria-expanded:bg-hover",
        ghost: "text-fg-2 hover:bg-hover hover:text-fg aria-expanded:bg-hover aria-expanded:text-fg",
        danger: "bg-danger text-on-danger hover:opacity-90",
        "danger-ghost": "text-danger hover:bg-danger-soft",
        link: "h-auto px-0 text-brand-text underline-offset-4 hover:underline",
      },
      size: {
        sm: "h-7 px-2.5 text-caption",
        md: "h-8 px-3 text-ui",
        lg: "h-10 px-4 text-ui",
        icon: "size-8",
        "icon-sm": "size-7 [&_svg:not([class*='size-'])]:size-3.5",
        "icon-lg": "size-9",
      },
    },
    defaultVariants: { variant: "secondary", size: "md" },
  },
);

type ButtonProps = React.ComponentProps<"button"> & VariantProps<typeof buttonVariants> & { asChild?: boolean };

function Button({ className, variant, size, asChild = false, type, ...props }: ButtonProps) {
  const Comp = asChild ? Slot.Root : "button";
  return (
    <Comp
      data-slot="button"
      data-variant={variant ?? "secondary"}
      type={asChild ? undefined : (type ?? "button")}
      className={cn(buttonVariants({ variant, size }), className)}
      {...props}
    />
  );
}

export { Button, buttonVariants, type ButtonProps };
