// "What did the work" chips: Local AI (green), Cloud AI (blue), Native tools (neutral) and MCP servers
// such as the Playwright browser (violet). Never colour only: every chip has an icon and words.
import { memo } from "react";
import { Cloud, Cpu, Globe, Monitor, Plug } from "lucide-react";
import type { UsedTag } from "@/lib/chat";
import type { Via } from "@/lib/types";
import { mcpServerLabel, USED_META, usedChipText } from "@/lib/task";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";

function UsedIcon({ tag }: { tag: UsedTag }) {
  if (tag.kind === "local_ai") return <Cpu aria-hidden="true" />;
  if (tag.kind === "cloud_ai") return <Cloud aria-hidden="true" />;
  if (tag.kind === "native") return <Monitor aria-hidden="true" />;
  return /browser|playwright/i.test(tag.label) ? <Globe aria-hidden="true" /> : <Plug aria-hidden="true" />;
}

export function UsedChip({ tag, className }: { tag: UsedTag; className?: string }) {
  const meta = USED_META[tag.kind];
  const text = usedChipText(tag);
  return (
    <Badge tone={meta.tone} className={className} title={[meta.description, tag.detail].filter(Boolean).join(" ")} data-testid="used-chip" data-kind={tag.kind}>
      <UsedIcon tag={tag} />
      <span className="sr-only">Used </span>
      {text}
    </Badge>
  );
}

/** A row of chips; renders nothing when there are no tags. */
export const UsedChips = memo(function UsedChips({ tags, className, label = "What did the work" }: { tags: UsedTag[]; className?: string; label?: string }) {
  if (!tags.length) return null;
  return (
    <ul className={cn("flex flex-wrap items-center gap-1.5", className)} aria-label={label} data-testid="used-chips">
      {tags.map((t) => (
        <li key={`${t.kind}:${t.label}`} className="inline-flex">
          <UsedChip tag={t} />
        </li>
      ))}
    </ul>
  );
});

/** Small tag on a step row: how that step ran (Relay's own tools, or an MCP server). */
export function ViaChip({ via, server }: { via: Via | null | undefined; server?: string | null }) {
  if (!via) return null;
  if (via === "mcp") {
    const label = mcpServerLabel(server);
    return (
      <Badge tone="violet" className="h-[18px] px-1.5" title={`Ran through ${label} (a connected MCP tool)`} data-testid="via-chip" data-via="mcp">
        {/browser|playwright/i.test(label) ? <Globe aria-hidden="true" /> : <Plug aria-hidden="true" />}
        {label}
      </Badge>
    );
  }
  return (
    <Badge tone="outline" className="h-[18px] px-1.5" title="Ran with Relay's own tools on this PC" data-testid="via-chip" data-via="native">
      <Monitor aria-hidden="true" />
      Native
    </Badge>
  );
}
