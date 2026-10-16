import { forwardRef, memo, type ButtonHTMLAttributes, type ReactNode } from "react";
import { Tooltip as T, Switch as SW } from "radix-ui";
import { Folder } from "lucide-react";
import { folderColor } from "../lib/theme";
import { initials } from "../lib/grouping";

export function cn(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(" ");
}

export const isMac = typeof navigator !== "undefined" && /Mac/.test(navigator.platform);
export const MOD = isMac ? "⌘" : "Ctrl";

export function Tip({ label, kbd, children, side = "bottom" }: { label: string; kbd?: string; children: ReactNode; side?: "top" | "bottom" | "left" | "right" }) {
  return (
    <T.Root>
      <T.Trigger asChild>{children}</T.Trigger>
      <T.Portal>
        <T.Content
          side={side}
          sideOffset={6}
          className="fade-in z-[70] flex items-center gap-2 rounded-lg bg-[#0d0d0d] px-2.5 py-1.5 text-[12.5px] font-medium text-white shadow-lg select-none"
        >
          {label}
          {kbd && <span className="text-white/55">{kbd}</span>}
        </T.Content>
      </T.Portal>
    </T.Root>
  );
}

type IconButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & { label: string; kbd?: string; tipSide?: "top" | "bottom" | "left" | "right"; noTip?: boolean };

/** Square icon button with an accessible label and a tooltip. */
export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  { label, kbd, tipSide, className, children, noTip, ...rest },
  ref,
) {
  const btn = (
    <button
      ref={ref}
      type="button"
      aria-label={label}
      className={cn(
        "inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-lg text-muted transition-colors duration-150 hover:bg-hover hover:text-fg disabled:opacity-40 disabled:hover:bg-transparent",
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  );
  return noTip ? btn : (
    <Tip label={label} kbd={kbd} side={tipSide}>
      {btn}
    </Tip>
  );
});

export const Button = forwardRef<
  HTMLButtonElement,
  ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "primary" | "secondary" | "ghost" | "danger"; size?: "sm" | "md" }
>(function Button({ variant = "secondary", size = "md", className, ...rest }, ref) {
  return (
    <button
      ref={ref}
      type="button"
      className={cn(
        "inline-flex shrink-0 items-center justify-center gap-2 rounded-full font-medium whitespace-nowrap transition-[background-color,opacity,color] duration-150 disabled:pointer-events-none disabled:opacity-45",
        size === "sm" ? "h-8 px-3 text-[13px]" : "h-9 px-4 text-sm",
        variant === "primary" && "bg-send text-send-fg hover:opacity-90",
        variant === "secondary" && "border border-line-strong bg-transparent text-fg hover:bg-hover",
        variant === "ghost" && "text-muted hover:bg-hover hover:text-fg",
        variant === "danger" && "bg-[#cf3328] text-white hover:bg-[#b92b21]",
        className,
      )}
      {...rest}
    />
  );
});

/** Relay's own mark: three crossing strokes forming a spark. */
export const RelayMark = memo(function RelayMark({ size = 22, className }: { size?: number; className?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" className={className}>
      <g stroke="var(--accent)" strokeWidth="2.6" strokeLinecap="round">
        <path d="M12 3.5v17" />
        <path d="M4.6 7.75l14.8 8.5" />
        <path d="M4.6 16.25l14.8-8.5" />
      </g>
    </svg>
  );
});

export function FolderIcon({ color, size = 18 }: { color: string; size?: number }) {
  return <Folder size={size} strokeWidth={1.8} style={{ color: folderColor(color) }} aria-hidden="true" className="shrink-0" />;
}

export function Avatar({ name, size = 28 }: { name: string; size?: number }) {
  return (
    <span
      aria-hidden="true"
      className="inline-flex shrink-0 items-center justify-center rounded-full font-semibold text-white select-none"
      style={{
        width: size,
        height: size,
        fontSize: size * 0.38,
        background: "linear-gradient(135deg, color-mix(in srgb, var(--accent) 85%, #fff 0%), color-mix(in srgb, var(--accent) 55%, #000))",
      }}
    >
      {initials(name)}
    </span>
  );
}

export function Toggle({ checked, onChange, label, disabled }: { checked: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean }) {
  return (
    <SW.Root
      checked={checked}
      onCheckedChange={onChange}
      aria-label={label}
      disabled={disabled}
      className="relative inline-flex h-[22px] w-[38px] shrink-0 items-center rounded-full bg-active transition-colors duration-150 data-[state=checked]:bg-accent disabled:opacity-50"
    >
      <SW.Thumb className="block h-[18px] w-[18px] translate-x-[2px] rounded-full bg-white shadow transition-transform duration-150 data-[state=checked]:translate-x-[18px]" />
    </SW.Root>
  );
}

export function Spinner({ size = 16, className }: { size?: number; className?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" className={cn("animate-spin", className)} aria-hidden="true">
      <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" strokeOpacity="0.2" strokeWidth="2.5" />
      <path d="M21 12a9 9 0 0 0-9-9" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" />
    </svg>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return <span className="kbd">{children}</span>;
}
