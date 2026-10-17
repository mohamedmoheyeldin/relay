import { describe, expect, it } from "vitest";
import { bucketOf, formatBytes, formatDuration, groupByDate, initials, sidebarSections } from "./grouping";
import type { ConvSummary } from "./types";

const now = new Date(2026, 9, 16, 15, 0, 0).getTime(); // 16 Oct 2026, 15:00 local
const at = (d: number, h = 12) => new Date(2026, 9, d, h, 0, 0).getTime();

function conv(id: string, updated: number, p: Partial<ConvSummary> = {}): ConvSummary {
  return { id, title: id, project_id: null, pinned: false, archived: false, created_at: updated, updated_at: updated, ...p };
}

describe("date buckets", () => {
  it("assigns buckets by local calendar day", () => {
    expect(bucketOf(at(16, 0), now)).toBe("Today");
    expect(bucketOf(at(15, 23), now)).toBe("Yesterday");
    expect(bucketOf(at(15, 0), now)).toBe("Yesterday");
    expect(bucketOf(at(14, 23), now)).toBe("Previous 7 days");
    expect(bucketOf(at(9), now)).toBe("Previous 7 days");
    expect(bucketOf(at(1), now)).toBe("Previous 30 days");
    expect(bucketOf(new Date(2026, 6, 1).getTime(), now)).toBe("Older");
  });

  it("groups in fixed order, newest first inside a group", () => {
    const g = groupByDate([conv("old", at(1)), conv("t1", at(16, 9)), conv("y", at(15)), conv("t2", at(16, 14))], now);
    expect(g.map((x) => x.label)).toEqual(["Today", "Yesterday", "Previous 30 days"]);
    expect(g[0]!.items.map((c) => c.id)).toEqual(["t2", "t1"]);
  });

  it("returns no groups for no chats", () => {
    expect(groupByDate([], now)).toEqual([]);
  });
});

describe("sidebar sections", () => {
  it("splits pinned and recents and hides archived and project chats from recents", () => {
    const s = sidebarSections([
      conv("a", 5, { pinned: true }),
      conv("b", 4),
      conv("c", 3, { project_id: "p" }),
      conv("d", 2, { archived: true }),
      conv("e", 1, { pinned: true, project_id: "p" }),
    ]);
    expect(s.pinned.map((c) => c.id)).toEqual(["a", "e"]);
    expect(s.recents.map((c) => c.id)).toEqual(["b"]);
  });
});

describe("formatting", () => {
  it("initials", () => {
    expect(initials("Mohamed Moheyeldin")).toBe("MM");
    expect(initials("ada")).toBe("AD");
    expect(initials("  ")).toBe("R");
    expect(initials("Mary Ann Smith")).toBe("MS");
  });
  it("bytes", () => {
    expect(formatBytes(0)).toBe("—");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(9.3e9)).toBe("8.7 GB");
    expect(formatBytes(1536)).toBe("1.5 KB");
  });
  it("durations", () => {
    expect(formatDuration(400)).toBe("0s");
    expect(formatDuration(12_400)).toBe("12s");
    expect(formatDuration(60_000)).toBe("1m");
    expect(formatDuration(95_000)).toBe("1m 35s");
    expect(formatDuration(3_900_000)).toBe("1h 5m");
    expect(formatDuration(7_200_000)).toBe("2h");
  });
});
