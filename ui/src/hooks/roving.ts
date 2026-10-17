import { useCallback, useEffect, type KeyboardEvent, type RefObject } from "react";

const SELECTOR = "[data-roving]";

/**
 * Roving tabindex for a list: one tab stop, Up/Down (and Home/End) move focus between items
 * marked with `data-roving`. The item with `data-roving-active` (or the first) gets tabindex 0.
 */
export function useRovingList(ref: RefObject<HTMLElement | null>) {
  useEffect(() => {
    const root = ref.current;
    if (!root) return;
    const items = Array.from(root.querySelectorAll<HTMLElement>(SELECTOR));
    if (!items.length) return;
    const focused = items.find((el) => el === document.activeElement);
    const active = focused ?? items.find((el) => el.dataset.rovingActive === "true") ?? items.find((el) => el.tabIndex === 0) ?? items[0]!;
    for (const el of items) el.tabIndex = el === active ? 0 : -1;
  });

  return useCallback(
    (e: KeyboardEvent<HTMLElement>) => {
      const root = ref.current;
      if (!root) return;
      const target = e.target as HTMLElement;
      if (!target.matches(SELECTOR)) return;
      const items = Array.from(root.querySelectorAll<HTMLElement>(SELECTOR));
      const i = items.indexOf(target);
      if (i < 0) return;
      let next = -1;
      if (e.key === "ArrowDown") next = Math.min(items.length - 1, i + 1);
      else if (e.key === "ArrowUp") next = Math.max(0, i - 1);
      else if (e.key === "Home") next = 0;
      else if (e.key === "End") next = items.length - 1;
      if (next < 0 || next === i) {
        if (next === i) e.preventDefault();
        return;
      }
      e.preventDefault();
      for (const el of items) el.tabIndex = -1;
      items[next]!.tabIndex = 0;
      items[next]!.focus();
    },
    [ref],
  );
}
