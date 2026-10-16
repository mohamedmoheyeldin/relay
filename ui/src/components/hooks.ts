import { useEffect, useState, useSyncExternalStore } from "react";

const mq = typeof window !== "undefined" ? window.matchMedia("(max-width: 767px)") : null;

/** True below 768px: the sidebar becomes an overlay. */
export function useNarrow(): boolean {
  return useSyncExternalStore(
    (cb) => {
      mq?.addEventListener("change", cb);
      return () => mq?.removeEventListener("change", cb);
    },
    () => mq?.matches ?? false,
  );
}

/** Current time, ticking every `ms` while `enabled`. */
export function useNow(enabled: boolean, ms = 1000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!enabled) return;
    setNow(Date.now());
    const t = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(t);
  }, [enabled, ms]);
  return now;
}

export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}
