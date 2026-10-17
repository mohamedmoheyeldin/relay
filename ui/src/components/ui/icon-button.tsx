import { Button, type ButtonProps } from "./button";
import { Tip } from "./tooltip";

type IconButtonProps = Omit<ButtonProps, "size"> & {
  /** Accessible name and tooltip text. */
  label: string;
  shortcut?: string;
  tipSide?: "top" | "bottom" | "left" | "right";
  noTip?: boolean;
  size?: "icon" | "icon-sm" | "icon-lg";
};

/** Square icon button with an accessible label and a tooltip. */
function IconButton({ label, shortcut, tipSide, noTip, variant = "ghost", size = "icon", ...rest }: IconButtonProps) {
  const btn = <Button aria-label={label} variant={variant} size={size} {...rest} />;
  if (noTip) return btn;
  return (
    <Tip label={label} shortcut={shortcut} side={tipSide}>
      {btn}
    </Tip>
  );
}

export { IconButton, type IconButtonProps };
