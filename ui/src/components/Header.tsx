import { useState } from "react";
import { DropdownMenu } from "radix-ui";
import { Check, ChevronDown, Cloud, Cpu, Ellipsis, PanelLeft, Route, Settings2, Square, SquarePen } from "lucide-react";
import { selectBusy, useApp } from "../store/app";
import type { RouteMode } from "../lib/types";
import { Button, cn, IconButton, MOD, Tip } from "./ui";
import { ChatMenuItems } from "./menus";
import { useNarrow } from "./hooks";

const MODES: { id: RouteMode; label: string; desc: string; icon: typeof Route }[] = [
  { id: "auto", label: "Auto", desc: "Local AI for PC chores, Claude for the web, code and writing", icon: Route },
  { id: "local_only", label: "Local only", desc: "Everything stays on this PC", icon: Cpu },
  { id: "cloud_only", label: "Claude only", desc: "Send every task to Claude", icon: Cloud },
];
export const modeLabel = (m: RouteMode | undefined) => MODES.find((x) => x.id === m)?.label ?? "Auto";

export function Header() {
  const sidebarOpen = useApp((s) => s.sidebarOpen);
  const narrow = useNarrow();
  const { toggleSidebar, newChat } = useApp.getState();
  const showLeft = narrow || !sidebarOpen;
  const chatId = useApp((s) => (s.view.kind === "chat" && !s.view.id.startsWith("tmp-") ? s.view.id : null));
  const busy = useApp(selectBusy);

  return (
    <header className="flex h-[52px] shrink-0 items-center gap-1 px-2">
      {showLeft && (
        <div className="flex items-center">
          <IconButton label="Open sidebar" kbd={`${MOD}+Shift+S`} onClick={toggleSidebar}>
            <PanelLeft size={19} />
          </IconButton>
          <IconButton label="New chat" kbd={`${MOD}+Shift+O`} onClick={() => newChat()}>
            <SquarePen size={18} />
          </IconButton>
        </div>
      )}
      <EnginePicker />
      <div className="ml-auto flex items-center gap-1">
        {busy && (
          <Tip label="Stop the running task">
            <Button size="sm" variant="secondary" onClick={() => useApp.getState().stop()} className="gap-1.5">
              <Square size={11} fill="currentColor" /> Stop
            </Button>
          </Tip>
        )}
        {chatId && <ChatOverflow id={chatId} />}
      </div>
    </header>
  );
}

function ChatOverflow({ id }: { id: string }) {
  const conv = useApp((s) => s.convs.find((c) => c.id === id) ?? s.archived?.find((c) => c.id === id));
  const setRenameDialog = useApp((s) => s.setRenameDialog);
  const [open, setOpen] = useState(false);
  if (!conv) return null;
  return (
    <DropdownMenu.Root open={open} onOpenChange={setOpen}>
      <Tip label="Chat options">
        <DropdownMenu.Trigger asChild>
          <button type="button" aria-label="Chat options" className="inline-flex h-9 w-9 items-center justify-center rounded-lg text-muted hover:bg-hover hover:text-fg data-[state=open]:bg-hover">
            <Ellipsis size={19} />
          </button>
        </DropdownMenu.Trigger>
      </Tip>
      <DropdownMenu.Portal>
        <DropdownMenu.Content className="menu pop-in" align="end" sideOffset={6}>
          <ChatMenuItems conv={conv} onRename={() => setRenameDialog({ id: conv.id, title: conv.title || "New chat" })} />
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}

function EnginePicker() {
  const engines = useApp((s) => s.engines);
  const status = useApp((s) => s.status);
  const mode = engines?.route_mode ?? status?.route_mode;
  const { setRouteMode, openSettings, loadEngines } = useApp.getState();
  const localName = engines?.local.active?.name ?? status?.local_model ?? null;
  const localReady = engines ? engines.local.available && engines.local.models.length > 0 : Boolean(status?.local_model);

  return (
    <DropdownMenu.Root onOpenChange={(o) => o && void loadEngines()}>
      <DropdownMenu.Trigger asChild>
        <button
          type="button"
          className="flex h-9 items-center gap-1.5 rounded-lg px-2.5 text-[17px] font-medium hover:bg-hover data-[state=open]:bg-hover"
          aria-label={`Engine: ${modeLabel(mode)}`}
        >
          <span>Relay</span>
          <span className="text-muted">{mode ? modeLabel(mode) : ""}</span>
          <ChevronDown size={16} className="text-subtle" />
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content className="menu pop-in w-[320px] max-w-[calc(100vw-16px)]" align="start" sideOffset={6}>
          <div className="menu-label">Engine</div>
          {MODES.map((m) => (
            <DropdownMenu.Item key={m.id} className="menu-item items-start" onSelect={() => setRouteMode(m.id)}>
              <m.icon size={17} className="mt-0.5 shrink-0 text-muted" />
              <span className="min-w-0 flex-1">
                <span className="block">{m.label}</span>
                <span className="block text-[12.5px] leading-[17px] text-subtle">{m.desc}</span>
              </span>
              <Check size={16} className={cn("mt-0.5 shrink-0", mode === m.id ? "opacity-100" : "opacity-0")} />
            </DropdownMenu.Item>
          ))}
          <DropdownMenu.Separator className="menu-sep" />
          <div className="space-y-1.5 px-2.5 py-1.5 text-[12.5px]">
            <EngineLine
              label="Relay Local AI"
              ok={localReady}
              loading={!engines && !status}
              text={localReady ? (localName ?? "Automatic") : engines?.setup.state === "running" ? "Setting up…" : "Not set up"}
            />
            <EngineLine
              label="Claude"
              ok={engines ? engines.cloud.installed && engines.cloud.logged_in : null}
              loading={!engines}
              text={!engines ? "Checking…" : !engines.cloud.installed ? "Not installed" : engines.cloud.logged_in ? "Signed in" : "Not signed in"}
            />
          </div>
          <DropdownMenu.Separator className="menu-sep" />
          <DropdownMenu.Item className="menu-item" onSelect={() => openSettings("engines")}>
            <Settings2 size={16} /> Engine settings
          </DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}

function EngineLine({ label, ok, text, loading }: { label: string; ok: boolean | null; text: string; loading?: boolean }) {
  return (
    <div className="flex items-center gap-2">
      <span
        aria-hidden="true"
        className={cn("h-1.5 w-1.5 rounded-full", loading || ok === null ? "bg-subtle" : ok ? "bg-success" : "bg-danger")}
      />
      <span className="text-muted">{label}</span>
      <span className="ml-auto truncate text-subtle">{text}</span>
    </div>
  );
}
