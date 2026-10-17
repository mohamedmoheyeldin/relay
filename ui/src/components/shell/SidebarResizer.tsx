// Drag handle on the sidebar's right edge. Pointer drag resizes live (only a CSS variable changes
// while dragging) and saves on release; arrow keys move it by 16px (Shift: 48px), Home/End jump to
// the limits, a double-click or Enter resets the default width. Exposed as a vertical separator
// with its value so screen readers announce the width.
import { useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { clampSidebarWidth, SIDEBAR_DEFAULT, SIDEBAR_MAX, SIDEBAR_MIN } from "@/lib/theme";
import { previewSidebarWidth, useAppearance } from "@/store/appearance";
import { Tip } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

export const RESIZE_STEP = 16;
export const RESIZE_STEP_LARGE = 48;

export function SidebarResizer({ className }: { className?: string }) {
  const width = useAppearance((s) => s.appearance.sidebarWidth);
  const set = useAppearance((s) => s.set);
  const [dragging, setDragging] = useState(false);
  const drag = useRef<{ startX: number; startW: number; w: number; id: number } | null>(null);

  const commit = (w: number) => set({ sidebarWidth: clampSidebarWidth(w) });

  const end = (el: HTMLElement) => {
    const d = drag.current;
    drag.current = null;
    setDragging(false);
    document.documentElement.classList.remove("is-resizing");
    if (!d) return;
    if (el.hasPointerCapture?.(d.id)) el.releasePointerCapture(d.id);
    commit(d.w);
  };

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    drag.current = { startX: e.clientX, startW: width, w: width, id: e.pointerId };
    e.currentTarget.setPointerCapture?.(e.pointerId);
    e.currentTarget.focus();
    setDragging(true);
    document.documentElement.classList.add("is-resizing");
  };
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    d.w = clampSidebarWidth(d.startW + (e.clientX - d.startX));
    previewSidebarWidth(d.w);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const step = e.shiftKey ? RESIZE_STEP_LARGE : RESIZE_STEP;
    if (e.key === "ArrowLeft") commit(width - step);
    else if (e.key === "ArrowRight") commit(width + step);
    else if (e.key === "Home") commit(SIDEBAR_MIN);
    else if (e.key === "End") commit(SIDEBAR_MAX);
    else if (e.key === "Enter") commit(SIDEBAR_DEFAULT);
    else return;
    e.preventDefault();
  };

  return (
    <Tip label="Drag to resize" side="right">
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize sidebar"
        aria-valuemin={SIDEBAR_MIN}
        aria-valuemax={SIDEBAR_MAX}
        aria-valuenow={width}
        aria-valuetext={`${width} pixels wide`}
        tabIndex={0}
        data-testid="sidebar-resizer"
        data-dragging={dragging ? "true" : undefined}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={(e) => end(e.currentTarget)}
        onPointerCancel={(e) => end(e.currentTarget)}
        onLostPointerCapture={(e) => drag.current && end(e.currentTarget)}
        onDoubleClick={() => commit(SIDEBAR_DEFAULT)}
        onKeyDown={onKeyDown}
        className={cn(
          "group absolute inset-y-0 -right-1 z-10 w-2 cursor-col-resize touch-none outline-none",
          className,
        )}
      >
        <span
          aria-hidden="true"
          className={cn(
            "absolute inset-y-0 left-1/2 w-0.5 -translate-x-1/2 bg-transparent transition-colors duration-(--relay-dur-fast)",
            "group-hover:bg-line-strong group-focus-visible:bg-focus",
            dragging && "bg-focus",
          )}
        />
      </div>
    </Tip>
  );
}
