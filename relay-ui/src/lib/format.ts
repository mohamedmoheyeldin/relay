export function bytes(n: number): string {
    if (!n) return "0 B";
    const units = ["B", "KB", "MB", "GB", "TB"];
    let i = 0;
    let v = n;
    while (v >= 1024 && i < units.length - 1) {
        v /= 1024;
        i++;
    }
    return `${v.toFixed(v >= 10 || i === 0 ? 0 : 1)} ${units[i]}`;
}

export function gb(mb: number): string {
    return `${(mb / 1024).toFixed(1)} GB`;
}

export function percent(done: number, total: number): number {
    return total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0;
}

export const roleLabel: Record<string, string> = {
    grounding: "Where is it",
    describe: "Check / describe",
    both: "Both roles",
};
