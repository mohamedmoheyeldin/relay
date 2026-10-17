import type { ConvSummary } from "./types";

export type DateBucket = "Today" | "Yesterday" | "Previous 7 days" | "Previous 30 days" | "Older";
const ORDER: DateBucket[] = ["Today", "Yesterday", "Previous 7 days", "Previous 30 days", "Older"];
const DAY = 86_400_000;

function startOfDay(t: number): number {
  const d = new Date(t);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

export function bucketOf(updatedAt: number, now: number): DateBucket {
  const today = startOfDay(now);
  if (updatedAt >= today) return "Today";
  if (updatedAt >= today - DAY) return "Yesterday";
  if (updatedAt >= today - 7 * DAY) return "Previous 7 days";
  if (updatedAt >= today - 30 * DAY) return "Previous 30 days";
  return "Older";
}

/** Groups conversations (any order in) into date buckets, newest first within each. */
export function groupByDate<T extends Pick<ConvSummary, "updated_at">>(
  convs: T[],
  now: number,
): { label: DateBucket; items: T[] }[] {
  const map = new Map<DateBucket, T[]>();
  for (const c of [...convs].sort((a, b) => b.updated_at - a.updated_at)) {
    const b = bucketOf(c.updated_at, now);
    const arr = map.get(b);
    if (arr) arr.push(c);
    else map.set(b, [c]);
  }
  return ORDER.filter((b) => map.has(b)).map((label) => ({ label, items: map.get(label)! }));
}

/** Splits the sidebar list: pinned (any project), recents (no project, not pinned, not archived). */
export function sidebarSections(convs: ConvSummary[]): { pinned: ConvSummary[]; recents: ConvSummary[] } {
  const live = convs.filter((c) => !c.archived);
  return {
    pinned: live.filter((c) => c.pinned).sort((a, b) => b.updated_at - a.updated_at),
    recents: live.filter((c) => !c.pinned && !c.project_id),
  };
}

export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "R";
  if (parts.length === 1) return parts[0]!.slice(0, 2).toUpperCase();
  return (parts[0]![0]! + parts[parts.length - 1]![0]!).toUpperCase();
}

export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 10 || i === 0 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`;
}

export function formatDuration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const r = s % 60;
  if (m < 60) return r ? `${m}m ${r}s` : `${m}m`;
  const h = Math.floor(m / 60);
  const mins = m % 60;
  return mins ? `${h}h ${mins}m` : `${h}h`;
}
