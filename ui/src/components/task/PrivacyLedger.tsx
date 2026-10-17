import { useState } from "react";
import { ChevronRight, CloudUpload, ShieldCheck } from "lucide-react";
import type { TaskShare } from "@/lib/chat";
import { categoryLabel, formatSize, ledgerSummary, providerLabel } from "@/lib/task";
import { cn } from "@/lib/utils";
import { go, paths } from "@/lib/nav";

/** "Shared with <provider>" row: what local context went into a cloud request (expandable). */
export function PrivacyLedger({ shares, staysLocal }: { shares: TaskShare[]; staysLocal: boolean }) {
  const [open, setOpen] = useState(false);
  if (!shares.length) {
    if (!staysLocal) return null;
    return (
      <div className="flex items-center gap-2 px-4 py-2.5 text-caption text-fg-2">
        <ShieldCheck className="size-4 text-success" aria-hidden="true" />
        <span>Nothing was shared: this task stayed on this PC.</span>
      </div>
    );
  }
  const providers = [...new Set(shares.map((s) => providerLabel(s.provider)))].join(" and ");
  const categories = mergeCategories(shares);
  const bytes = shares.reduce((n, s) => n + s.bytes, 0);
  const id = `ledger-${shares.length}-${bytes}`;
  return (
    <div>
      <button
        type="button"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen((o) => !o)}
        className="flex w-full items-center gap-2 px-4 py-2.5 text-left text-caption text-fg-2 transition-colors hover:bg-hover"
      >
        <CloudUpload className="size-4 shrink-0 text-info" aria-hidden="true" />
        <span className="min-w-0 flex-1 truncate">
          <span className="font-medium text-fg">Shared with {providers}</span>
          <span className="text-fg-muted"> · {ledgerSummary(categories, bytes)}</span>
        </span>
        <ChevronRight aria-hidden="true" className={cn("size-4 shrink-0 text-fg-muted transition-transform duration-(--relay-dur-fast)", open && "rotate-90")} />
      </button>
      {open && (
        <div id={id} className="rise-in px-4 pb-3">
          <table className="w-full text-caption">
            <caption className="sr-only">Local information included in the cloud request</caption>
            <thead>
              <tr className="text-left text-fg-muted">
                <th scope="col" className="py-1 font-medium">
                  What
                </th>
                <th scope="col" className="py-1 text-right font-medium">
                  Items
                </th>
              </tr>
            </thead>
            <tbody>
              {categories.map((c) => (
                <tr key={c.category} className="border-t border-line">
                  <td className="py-1.5 text-fg-2">{categoryLabel(c.category)}</td>
                  <td className="py-1.5 text-right text-fg-2 tabular-nums">{c.items}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="mt-2 text-micro leading-4 text-fg-muted">
            Only these categories left this PC ({formatSize(bytes)} in total). Choose what may be shared in{" "}
            <button type="button" className="text-brand-text underline underline-offset-2" onClick={() => go(paths.settings("privacy"))}>
              Settings › AI &amp; Privacy
            </button>
            .
          </p>
        </div>
      )}
    </div>
  );
}

function mergeCategories(shares: TaskShare[]) {
  const map = new Map<string, number>();
  for (const s of shares) for (const c of s.categories) map.set(c.category, (map.get(c.category) ?? 0) + c.items);
  return [...map].map(([category, items]) => ({ category, items }));
}
