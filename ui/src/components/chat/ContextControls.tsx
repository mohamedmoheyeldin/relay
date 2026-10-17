// Context controls under the composer: privacy mode and cloud provider. Changes apply at once
// through set_hybrid (no restart). Unavailable providers stay visible with the reason.
import { Check, ChevronDown, Cloud, Route, Settings2, ShieldCheck } from "lucide-react";
import type { PrivacyMode, ProviderInfo } from "@/lib/types";
import { useHybrid, useSetHybrid } from "@/hooks/queries";
import { cn } from "@/lib/utils";
import { go, paths } from "@/lib/nav";
import { Segmented } from "@/components/ui/toggle-group";
import { Skeleton } from "@/components/ui/skeleton";
import { Tip } from "@/components/ui/tooltip";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";

export const PRIVACY_OPTIONS: { value: PrivacyMode; label: string; short: string; explain: string }[] = [
  {
    value: "local_only",
    label: "Local only",
    short: "Local",
    explain: "Nothing about your task leaves this PC. Relay Local AI plans everything; some tasks may be beyond it.",
  },
  {
    value: "hybrid_auto",
    label: "Hybrid",
    short: "Hybrid",
    explain: "Relay decides per task: simple or sensitive work stays local, harder planning may use your cloud provider with only the context you allow.",
  },
  {
    value: "cloud_preferred",
    label: "Cloud preferred",
    short: "Cloud",
    explain: "Your cloud provider plans most tasks for the best results. Relay still runs every step on this PC.",
  },
];

/** Why a provider cannot be used right now, or null when it is ready. */
export function providerProblem(p: ProviderInfo): string | null {
  if (!p.installed) return "Not installed";
  if (!p.logged_in) return "Not signed in";
  return null;
}

export function providerStatusLine(p: ProviderInfo): string {
  const problem = providerProblem(p);
  if (problem) return problem;
  return ["Signed in", p.version ? `v${p.version}` : null].filter(Boolean).join(" · ");
}

export function PrivacySelector({ size = "md" }: { size?: "sm" | "md" }) {
  const hybrid = useHybrid();
  const set = useSetHybrid();
  if (!hybrid.data) return <Skeleton className={cn("rounded-lg", size === "sm" ? "h-7 w-[218px]" : "h-8 w-[262px]")} />;
  return (
    <Segmented<PrivacyMode>
      label="Privacy mode"
      size={size}
      value={hybrid.data.privacy_mode}
      onValueChange={(v) => set.mutate({ privacyMode: v })}
      options={PRIVACY_OPTIONS.map((o) => ({
        value: o.value,
        label: o.label,
        title: o.explain,
        icon: o.value === "local_only" ? <ShieldCheck aria-hidden="true" /> : o.value === "hybrid_auto" ? <Route aria-hidden="true" /> : <Cloud aria-hidden="true" />,
      }))}
    />
  );
}

export function ProviderPicker({ size = "md" }: { size?: "sm" | "md" }) {
  const hybrid = useHybrid();
  const set = useSetHybrid();
  if (!hybrid.data) return <Skeleton className={cn("rounded-lg", size === "sm" ? "h-7 w-[132px]" : "h-8 w-[150px]")} />;
  const h = hybrid.data;
  const current = h.providers.find((p) => p.id === h.default_provider) ?? h.providers[0];
  const localOnly = h.privacy_mode === "local_only";
  const problem = current ? providerProblem(current) : "Not available";
  return (
    <DropdownMenu>
      <Tip label={localOnly ? "Not used in Local only mode" : problem ? `${current?.label}: ${problem}` : "Cloud provider for planning"} side="top">
        <DropdownMenuTrigger
          className={cn(
            "inline-flex items-center gap-1.5 rounded-lg border border-line bg-sunk font-medium text-fg-2 transition-colors hover:text-fg data-[state=open]:text-fg",
            size === "sm" ? "h-7 px-2 text-caption" : "h-8 px-2.5 text-ui",
          )}
          aria-label={`Cloud provider: ${current?.label ?? "none"}${problem ? `, ${problem.toLowerCase()}` : ""}${localOnly ? ", not used in Local only mode" : ""}`}
          data-testid="provider-picker"
        >
          <span className={cn("size-1.5 rounded-full", localOnly ? "bg-fg-muted/50" : problem ? "bg-warning" : "bg-success")} aria-hidden="true" />
          <span className={cn(localOnly && "text-fg-muted line-through decoration-fg-muted/40")}>{current?.label ?? "No provider"}</span>
          <ChevronDown className="size-3.5 text-fg-muted" aria-hidden="true" />
        </DropdownMenuTrigger>
      </Tip>
      <DropdownMenuContent align="start" side="top" className="w-[300px]">
        <DropdownMenuLabel>Cloud provider for planning</DropdownMenuLabel>
        {h.providers.map((p) => {
          const why = providerProblem(p);
          const selected = p.id === h.default_provider;
          return (
            <DropdownMenuItem
              key={p.id}
              aria-disabled={why ? true : undefined}
              data-testid={`provider-option-${p.id}`}
              className={cn("h-auto items-start py-2", why && "opacity-70")}
              onSelect={(e) => {
                if (why) {
                  e.preventDefault();
                  return;
                }
                if (!selected) set.mutate({ defaultProvider: p.id });
              }}
            >
              <Cloud className="mt-0.5" aria-hidden="true" />
              <span className="min-w-0 flex-1">
                <span className="block text-ui text-fg">{p.label}</span>
                <span className={cn("block text-caption", why ? "text-warning" : "text-fg-muted")}>
                  {providerStatusLine(p)}
                  {why === "Not signed in" && <> · run {p.id === "claude" ? "claude auth login" : "codex login"}</>}
                  {why === "Not installed" && <> · see Settings</>}
                </span>
              </span>
              {selected && <Check className="mt-0.5 text-fg" aria-hidden="true" />}
              {selected && <span className="sr-only">(selected)</span>}
            </DropdownMenuItem>
          );
        })}
        {localOnly && <p className="px-2 py-1.5 text-caption text-fg-muted">Local only is on, so no cloud provider is used right now.</p>}
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={() => go(paths.settings("privacy"))}>
          <Settings2 aria-hidden="true" /> AI &amp; Privacy settings…
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** The bar under the composer. */
export function ContextControls({ size = "md", className }: { size?: "sm" | "md"; className?: string }) {
  const hybrid = useHybrid();
  if (hybrid.isError && !hybrid.data) {
    return (
      <div className={cn("flex items-center justify-center gap-2 text-caption text-fg-muted", className)} role="status">
        Privacy and provider settings are unavailable.
        <button type="button" className="text-brand-text underline-offset-2 hover:underline" onClick={() => void hybrid.refetch()}>
          Try again
        </button>
      </div>
    );
  }
  return (
    <div className={cn("flex flex-wrap items-center justify-center gap-2", className)} role="group" aria-label="Task context">
      <PrivacySelector size={size} />
      <ProviderPicker size={size} />
    </div>
  );
}
