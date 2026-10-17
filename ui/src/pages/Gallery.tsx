// Component gallery (/gallery, development builds only): every component and state on one page,
// in the current theme. Used for visual review and as living documentation.
import { useState, type ReactNode } from "react";
import { Bell, Plus, Settings2, Trash2 } from "lucide-react";
import { taskFixture, type FixtureName } from "@/lib/fixtures";
import { applyEvent, emptyChat, type ToolsItem } from "@/lib/chat";
import { contrast } from "@/lib/theme";
import { useApp } from "@/store/app";
import { useAppearance } from "@/store/appearance";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { IconButton } from "@/components/ui/icon-button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Switch } from "@/components/ui/switch";
import { Segmented } from "@/components/ui/toggle-group";
import { Shortcut, Kbd } from "@/components/ui/kbd";
import { Skeleton, SkeletonLines } from "@/components/ui/skeleton";
import { ProgressRow } from "@/components/ui/progress";
import { RadioGroup, RadioGroupItem, RadioCard } from "@/components/ui/radio-group";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuShortcut, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Avatar, EmptyState, ErrorNote, FolderIcon, RelayMark, StatusDot } from "@/components/common";
import { TaskTimeline } from "@/components/task/TaskTimeline";
import { StepIcon } from "@/components/task/StepIcon";
import { ToolGroup } from "@/components/task/ToolGroup";
import { PermissionDialogView } from "@/components/task/PermissionDialog";
import { ContextControls } from "@/components/chat/ContextControls";
import { Thinking } from "@/components/chat/Messages";

function Block({ title, children, note }: { title: string; children: ReactNode; note?: string }) {
  return (
    <section className="mb-12">
      <h2 className="mb-1 font-display text-[22px] leading-7">{title}</h2>
      {note && <p className="mb-4 text-caption text-fg-muted">{note}</p>}
      <div className={note ? "" : "mt-4"}>{children}</div>
    </section>
  );
}

const PAIRS: [string, string][] = [
  ["--relay-text", "--relay-bg"],
  ["--relay-text-muted", "--relay-bg"],
  ["--relay-text-muted", "--relay-sidebar"],
  ["--relay-text-2", "--relay-surface"],
  ["--relay-on-accent", "--relay-accent"],
  ["--relay-accent-text", "--relay-accent-soft"],
  ["--relay-danger", "--relay-danger-soft"],
  ["--relay-success", "--relay-success-soft"],
  ["--relay-warning", "--relay-warning-soft"],
  ["--relay-info", "--relay-info-soft"],
];

function resolveColor(name: string): string {
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return v.startsWith("#") ? v : "#000000";
}

function agentGroup(running: boolean): ToolsItem {
  let s = emptyChat();
  const ev = [
    { type: "run_started", run_id: 1, conv_id: "g" },
    { type: "tool_started", run_id: 1, tool_id: "t1", name: "screenshot", summary: "Take a screenshot" },
    { type: "tool_finished", run_id: 1, tool_id: "t1", ok: true, summary: "1568×882" },
    { type: "tool_started", run_id: 1, tool_id: "t2", name: "left_click", summary: "Click the Inbox" },
    ...(running ? [] : [{ type: "tool_finished", run_id: 1, tool_id: "t2", ok: true, summary: "OK · screen changed" }, { type: "run_finished", run_id: 1, outcome: "completed" }]),
  ] as Parameters<typeof applyEvent>[1][];
  ev.forEach((e, i) => (s = applyEvent(s, e, Date.now() - 12_000 + i * 2000)));
  return s.items.find((i): i is ToolsItem => i.kind === "tools")!;
}

export default function Gallery() {
  const scheme = useAppearance((s) => s.scheme);
  const set = useAppearance((s) => s.set);
  const [seg, setSeg] = useState<"a" | "b" | "c">("b");
  const [sw, setSw] = useState(true);
  const [showPerm, setShowPerm] = useState(false);
  const fixtures: FixtureName[] = ["running", "awaiting", "verified", "unverified", "failed", "refused", "declined", "stopped", "local", "native"];

  return (
    <div className="h-full overflow-y-auto scroll-thin" data-testid="gallery">
      <div className="mx-auto max-w-[860px] px-6 py-10">
        <div className="mb-10 flex items-center gap-3">
          <RelayMark size={30} />
          <h1 className="flex-1 font-display text-display">Component gallery</h1>
          <Segmented label="Mode" size="sm" value={scheme} onValueChange={(v) => set({ mode: v })} options={[{ value: "light", label: "Light" }, { value: "dark", label: "Dark" }]} />
        </div>

        <Block title="Colour pairs" note="Live values from tokens.css and their WCAG contrast (AA needs 4.5:1 for text).">
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-5">
            {PAIRS.map(([fg, bg]) => (
              <div key={fg + bg} className="overflow-hidden rounded-lg border border-line">
                <div className="flex h-12 items-center justify-center font-serif text-[20px]" style={{ background: `var(${bg})`, color: `var(${fg})` }}>
                  Aa
                </div>
                <div className="px-2 py-1.5 text-micro">
                  <div className="truncate font-mono">{fg.replace("--relay-", "")}</div>
                  <div className="truncate text-fg-muted">
                    on {bg.replace("--relay-", "")} · {contrast(resolveColor(fg), resolveColor(bg)).toFixed(2)}:1
                  </div>
                </div>
              </div>
            ))}
          </div>
        </Block>

        <Block title="Typography">
          <div className="space-y-3">
            <p className="font-display text-display">Display · Good evening</p>
            <p className="font-display text-title">Title · Task history</p>
            <p className="text-heading font-semibold">Heading · Cloud providers</p>
            <p className="text-body">Body · Relay runs every step on this PC and checks the result.</p>
            <p className="text-ui">UI · Menu items, buttons and labels</p>
            <p className="text-caption text-fg-muted">Caption · Secondary details and timestamps</p>
            <p className="font-mono text-code">Code · recycle_files --older-than 30d</p>
          </div>
        </Block>

        <Block title="Buttons">
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="primary">Primary</Button>
            <Button>Secondary</Button>
            <Button variant="outline">Outline</Button>
            <Button variant="ghost">Ghost</Button>
            <Button variant="danger">Danger</Button>
            <Button variant="danger-ghost">
              <Trash2 aria-hidden="true" /> Delete
            </Button>
            <Button variant="link">Link</Button>
            <Button variant="primary" disabled>
              Disabled
            </Button>
          </div>
          <div className="mt-3 flex flex-wrap items-center gap-2">
            <Button size="sm">Small</Button>
            <Button size="md">Medium</Button>
            <Button size="lg">Large</Button>
            <IconButton label="Add">
              <Plus aria-hidden="true" />
            </IconButton>
            <IconButton label="Settings" variant="secondary">
              <Settings2 aria-hidden="true" />
            </IconButton>
            <IconButton label="Notifications" size="icon-sm">
              <Bell aria-hidden="true" />
            </IconButton>
          </div>
        </Block>

        <Block title="Badges and status">
          <div className="flex flex-wrap items-center gap-2">
            <Badge>Neutral</Badge>
            <Badge tone="outline">Outline</Badge>
            <Badge tone="brand">Default</Badge>
            <Badge tone="success">Verified</Badge>
            <Badge tone="warning">Sensitive</Badge>
            <Badge tone="danger">Destructive</Badge>
            <Badge tone="info">Cloud</Badge>
            <span className="ml-3 inline-flex items-center gap-1.5 text-caption">
              <StatusDot tone="ok" /> ok <StatusDot tone="warn" /> warn <StatusDot tone="bad" /> bad <StatusDot tone="idle" /> idle <StatusDot tone="busy" /> busy
            </span>
          </div>
          <div className="mt-4 flex flex-wrap items-center gap-4 text-caption">
            {(["pending", "running", "verified", "done", "failed", "skipped", "stopped"] as const).map((s) => (
              <span key={s} className="inline-flex items-center gap-1.5">
                <StepIcon state={s} /> {s}
              </span>
            ))}
          </div>
        </Block>

        <Block title="Form controls">
          <div className="grid gap-4 sm:grid-cols-2">
            <Input placeholder="Normal input" />
            <Input placeholder="Invalid input" aria-invalid defaultValue="C:Windows" />
            <Input placeholder="Disabled" disabled />
            <Select defaultValue="sonnet">
              <SelectTrigger aria-label="Model">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="default">CLI default</SelectItem>
                <SelectItem value="sonnet">sonnet</SelectItem>
                <SelectItem value="opus">opus</SelectItem>
              </SelectContent>
            </Select>
            <Textarea placeholder="Textarea" rows={3} />
            <div className="flex flex-col gap-3">
              <label className="flex items-center gap-3 text-ui">
                <Switch checked={sw} onCheckedChange={setSw} /> Switch {sw ? "on" : "off"}
              </label>
              <label className="flex items-center gap-3 text-ui">
                <Switch disabled /> Disabled switch
              </label>
              <Segmented label="Segmented" value={seg} onValueChange={setSeg} options={[{ value: "a", label: "Local only" }, { value: "b", label: "Hybrid" }, { value: "c", label: "Cloud preferred" }]} />
            </div>
          </div>
          <RadioGroup defaultValue="x" className="mt-4 grid gap-3 sm:grid-cols-3" aria-label="Radio cards">
            <RadioCard value="x" aria-label="Selected card">
              <span className="text-ui font-medium">Selected card</span>
              <span className="text-caption text-fg-2">Brand border and soft fill.</span>
            </RadioCard>
            <RadioCard value="y" aria-label="Other card">
              <span className="text-ui font-medium">Other card</span>
              <span className="text-caption text-fg-2">Hairline border.</span>
            </RadioCard>
            <div className="flex items-center gap-2 text-ui">
              <RadioGroupItem value="z" aria-label="Plain radio" /> Plain radio
            </div>
          </RadioGroup>
        </Block>

        <Block title="Menus, dialogs, keys">
          <div className="flex flex-wrap items-center gap-3">
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button>Open menu</Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent>
                <DropdownMenuItem>
                  Rename <DropdownMenuShortcut>F2</DropdownMenuShortcut>
                </DropdownMenuItem>
                <DropdownMenuItem>Pin</DropdownMenuItem>
                <DropdownMenuItem disabled>Disabled item</DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem variant="destructive">Delete…</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
            <Dialog>
              <DialogTrigger asChild>
                <Button>Open dialog</Button>
              </DialogTrigger>
              <DialogContent>
                <DialogHeader>
                  <DialogTitle>Dialog title</DialogTitle>
                  <DialogDescription>Serif title, sans body, hairline border and a soft shadow.</DialogDescription>
                </DialogHeader>
                <DialogFooter>
                  <Button variant="ghost">Cancel</Button>
                  <Button variant="primary">Continue</Button>
                </DialogFooter>
              </DialogContent>
            </Dialog>
            <Button onClick={() => setShowPerm(true)}>Permission dialog</Button>
            <Button onClick={() => useApp.getState().toast("Chat archived")}>Info toast</Button>
            <Button onClick={() => useApp.getState().toast("Could not save settings: disk full", "error")}>Error toast</Button>
            <Shortcut keys={["Mod", "K"]} />
            <Kbd>Esc</Kbd>
          </div>
          <PermissionDialogView
            req={showPerm ? { id: 1, run_id: 1, title: "Move 23 files to the Recycle Bin?", detail: "C:\\Users\\you\\Downloads\n14 installers and 9 duplicates (3.6 GB).", risk: "destructive", at: Date.now() } : null}
            more={0}
            onAnswer={() => setShowPerm(false)}
          />
        </Block>

        <Block title="Context controls" note="Privacy mode and provider picker (live data from the backend).">
          <ContextControls />
        </Block>

        <Block title="Task timeline" note="Every verdict and step state, built from contract events by the real reducer.">
          {fixtures.map((f) => (
            <div key={f} className="mb-2">
              <div className="text-micro font-medium tracking-[0.02em] text-fg-muted">{f}</div>
              <TaskTimeline item={taskFixture(f)} live={f === "running" || f === "awaiting"} onStop={() => undefined} onRetry={() => undefined} />
            </div>
          ))}
        </Block>

        <Block title="Agent mode group">
          <ToolGroup item={agentGroup(true)} />
          <ToolGroup item={agentGroup(false)} />
          <Thinking />
        </Block>

        <Block title="Loading, empty and error states">
          <div className="grid gap-4 sm:grid-cols-2">
            <div className="space-y-3 rounded-xl border border-line p-4">
              <Skeleton className="h-6 w-1/2" />
              <SkeletonLines n={3} />
            </div>
            <div className="space-y-4 rounded-xl border border-line p-4">
              <ProgressRow percent={42} label="Downloading model qwen3:14b" />
              <ProgressRow percent={null} label="Starting Relay Local AI" />
            </div>
            <EmptyState title="No tasks yet">Every task Relay runs is listed here.</EmptyState>
            <ErrorNote title="Couldn't load these settings" action={<Button size="sm">Try again</Button>}>
              The backend did not answer.
            </ErrorNote>
          </div>
          <div className="mt-4 flex items-center gap-3">
            <Avatar name="Mohamed Moheyeldin" />
            <FolderIcon color="green" />
            <FolderIcon color="blue" />
            <FolderIcon color="red" />
          </div>
        </Block>
      </div>
    </div>
  );
}
