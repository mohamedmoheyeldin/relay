import { useEffect } from "react";
import { useApp } from "@/store/app";
import { usePlatform } from "@/hooks/queries";
import { getCurrentPath, go, paths } from "@/lib/nav";
import { matchesShortcut, shortcutKeys } from "@/lib/platform";

/** Global keyboard shortcuts (one helper decides Ctrl vs Command; see lib/platform.ts). */
export function useGlobalShortcuts() {
  const { os } = usePlatform();
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const s = useApp.getState();
      const narrow = window.matchMedia?.("(max-width: 767px)").matches ?? false;
      if (matchesShortcut(e, shortcutKeys("search"), os)) {
        e.preventDefault();
        s.setSearchOpen(!s.searchOpen);
      } else if (matchesShortcut(e, shortcutKeys("newChat"), os) || matchesShortcut(e, ["Mod", "Shift", "O"], os)) {
        e.preventDefault();
        s.setSearchOpen(false);
        go(paths.home());
        s.focusComposer();
      } else if (matchesShortcut(e, shortcutKeys("toggleSidebar"), os) || matchesShortcut(e, ["Mod", "Shift", "S"], os)) {
        e.preventDefault();
        s.toggleSidebar(narrow);
      } else if (matchesShortcut(e, shortcutKeys("settings"), os)) {
        e.preventDefault();
        if (getCurrentPath().startsWith("/settings")) go(s.lastAppPath || "/");
        else go(paths.settings());
      } else if (matchesShortcut(e, shortcutKeys("shortcuts"), os)) {
        e.preventDefault();
        go(paths.settings("keyboard"));
      } else if (matchesShortcut(e, shortcutKeys("tasks"), os)) {
        e.preventDefault();
        go(paths.tasks());
      } else if (matchesShortcut(e, shortcutKeys("focusComposer"), os)) {
        e.preventDefault();
        s.focusComposer();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [os]);
}
