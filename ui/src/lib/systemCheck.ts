// System check helpers shared by Settings > System check and the Home banner.
import { api, errorText } from "./backend";
import { openExternal } from "./links";
import { queryClient, qk } from "./query";
import { useApp } from "../store/app";
import type { SystemCheck, SystemCheckAction, SystemCheckItem } from "./schemas";

/** Items whose problems the Home banner points out (the cloud providers are optional). */
export const BANNER_IDS = ["local_ai", "node", "browser"] as const;

export function attentionItems(check: SystemCheck | undefined): SystemCheckItem[] {
  return (check?.items ?? []).filter((i) => i.status !== "ok" && (BANNER_IDS as readonly string[]).includes(i.id));
}

export function attentionText(n: number): string {
  return n === 1 ? "1 thing needs attention" : `${n} things need attention`;
}

/** Commands an action may run; anything else is not offered. */
const COMMANDS: Record<string, () => Promise<void>> = {
  local_ai_setup: async () => {
    await api.localAiSetup("start");
    useApp.getState().toast("Setting up Relay Local AI. Progress shows in Settings › Relay Local AI.");
  },
};

export function isRunnable(a: SystemCheckAction): boolean {
  return a.kind === "url" ? /^https?:\/\//i.test(a.value) : a.value in COMMANDS;
}

export async function runSystemAction(a: SystemCheckAction): Promise<void> {
  if (a.kind === "url") return openExternal(a.value);
  const run = COMMANDS[a.value];
  if (!run) return;
  try {
    await run();
  } catch (e) {
    useApp.getState().toast(`Couldn't start it: ${errorText(e)}`, "error");
  }
}

export function recheck(): Promise<unknown> {
  return queryClient.refetchQueries({ queryKey: qk.systemCheck });
}

// ---- banner dismissal: remembered per set of problems, so a new problem shows the banner again --

const LS_DISMISSED = "relay.systemCheckDismissed";

export function problemKey(items: SystemCheckItem[]): string {
  return items
    .map((i) => `${i.id}:${i.status}`)
    .sort()
    .join(",");
}

export function dismissedKey(): string | null {
  try {
    return localStorage.getItem(LS_DISMISSED);
  } catch {
    return null;
  }
}

export function dismissBanner(key: string) {
  try {
    localStorage.setItem(LS_DISMISSED, key);
  } catch {
    /* storage unavailable: dismissed for this session only */
  }
}
