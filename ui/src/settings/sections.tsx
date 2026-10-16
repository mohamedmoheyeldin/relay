import { useEffect, useState } from "react";
import { DropdownMenu } from "radix-ui";
import { ArchiveRestore, Check, ChevronDown, FolderOpen, MessageCircle, RotateCcw, Trash } from "lucide-react";
import { useApp } from "../store/app";
import { useAppearance } from "../store/appearance";
import { isTauri } from "../lib/backend";
import { api } from "../lib/backend";
import {
  ACCENTS,
  basePalette,
  effectivePalette,
  normalizeHex,
  THEMES,
  type AccentName,
  type Mode,
  type ThemeName,
} from "../lib/theme";
import type { ApprovalMode, LinkKind } from "../lib/types";
import { Button, cn, MOD, RelayMark, Toggle } from "../components/ui";
import { fieldClass } from "../components/Dialogs";
import { Card, PageTitle, Row, Segmented, SubTitle } from "./parts";

function SettingsSkeleton() {
  return (
    <div className="space-y-3" aria-label="Loading">
      <div className="skeleton h-12" />
      <div className="skeleton h-12" />
      <div className="skeleton h-12" />
    </div>
  );
}

function SettingsError() {
  const err = useApp((s) => s.settingsError);
  const load = useApp((s) => s.loadSettings);
  if (!err) return <SettingsSkeleton />;
  return (
    <div className="rounded-2xl bg-danger-soft px-4 py-3 text-[14px]">
      Couldn't load settings: {err}{" "}
      <button type="button" className="ml-2 underline" onClick={() => void load()}>
        Try again
      </button>
    </div>
  );
}

function NameInput() {
  const name = useApp((s) => s.settings?.user_name ?? "");
  const update = useApp((s) => s.updateSettings);
  const [v, setV] = useState(name);
  useEffect(() => setV(name), [name]);
  const commit = () => {
    if (v.trim() !== name) void update({ user_name: v.trim() });
  };
  return (
    <input
      id="user-name"
      value={v}
      onChange={(e) => setV(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
      placeholder="Your name"
      className={cn(fieldClass, "w-[220px] max-w-full")}
      maxLength={60}
    />
  );
}

export function GeneralSection() {
  const settings = useApp((s) => s.settings);
  const status = useApp((s) => s.status);
  const update = useApp((s) => s.updateSettings);
  return (
    <>
      <PageTitle>General</PageTitle>
      {!settings ? (
        <SettingsError />
      ) : (
        <>
          <Card>
            <Row label="Your name" desc="Shown in the sidebar; Relay may use it when it writes for you." htmlFor="user-name">
              <NameInput />
            </Row>
            <Row label="Set up local AI automatically" desc="Relay downloads and starts Relay Local AI by itself when this PC can run it.">
              <Toggle checked={settings.auto_setup} onChange={(v) => void update({ auto_setup: v })} label="Set up local AI automatically" />
            </Row>
          </Card>
          <SubTitle>Safety</SubTitle>
          <Card>
            <Row label="Ask before acting" desc="Choose which actions need your approval first. Unanswered requests are declined after 2 minutes." stack>
              <Segmented<ApprovalMode>
                label="Ask before acting"
                value={settings.approval_mode}
                onChange={(v) => void update({ approval_mode: v })}
                options={[
                  { id: "off", label: "Nothing" },
                  { id: "destructive", label: "Destructive actions" },
                  { id: "always", label: "Every action" },
                ]}
              />
            </Row>
            <Row label="Emergency stop" desc="Press this anywhere in Windows to stop Relay at once. The tray menu has Stop too.">
              <kbd className="rounded-lg border border-line-strong px-2.5 py-1 font-sans text-[13px] text-muted">{status?.kill_hotkey || settings.hotkey}</kbd>
            </Row>
          </Card>
        </>
      )}
    </>
  );
}

function ModeTile({ mode, current, onClick }: { mode: Mode; current: Mode; onClick: () => void }) {
  const light = { bg: "#ffffff", side: "#f3f3f3", line: "#d5d5d5", acc: "var(--accent)" };
  const dark = { bg: "#1f1f1f", side: "#151515", line: "#4a4a4a", acc: "var(--accent)" };
  const pane = (c: typeof light, clip?: string) => (
    <div className="absolute inset-0 flex" style={{ background: c.bg, clipPath: clip }}>
      <div className="w-[26%] space-y-1 p-1.5" style={{ background: c.side }}>
        <div className="h-1 w-3/4 rounded" style={{ background: c.line }} />
        <div className="h-1 w-1/2 rounded" style={{ background: c.line }} />
      </div>
      <div className="flex-1 space-y-1.5 p-2">
        <div className="ml-auto h-1.5 w-1/2 rounded" style={{ background: c.line }} />
        <div className="h-1 w-5/6 rounded" style={{ background: c.acc }} />
        <div className="h-1 w-2/3 rounded" style={{ background: c.line }} />
        <div className="mt-2 h-2.5 rounded-full border" style={{ borderColor: c.line }} />
      </div>
    </div>
  );
  const label = mode === "system" ? "System" : mode === "light" ? "Light" : "Dark";
  return (
    <button type="button" role="radio" aria-checked={current === mode} aria-label={label} onClick={onClick} className="group flex flex-col items-center gap-1.5">
      <span
        className={cn(
          "relative block h-[56px] w-[80px] overflow-hidden rounded-xl border-2 transition-colors duration-150",
          current === mode ? "border-accent" : "border-line group-hover:border-line-strong",
        )}
      >
        {mode === "light" && pane(light)}
        {mode === "dark" && pane(dark)}
        {mode === "system" && (
          <>
            {pane(light)}
            {pane(dark, "polygon(52% 0, 100% 0, 100% 100%, 52% 100%)")}
          </>
        )}
      </span>
      <span className={cn("text-[12.5px]", current === mode ? "text-fg" : "text-subtle")}>{label}</span>
    </button>
  );
}

function ColorField({ which }: { which: "bg" | "fg" }) {
  const appearance = useAppearance((s) => s.appearance);
  const scheme = useAppearance((s) => s.scheme);
  const setCustom = useAppearance((s) => s.setCustom);
  const value = effectivePalette(appearance, scheme)[which];
  const [text, setText] = useState(value.toUpperCase());
  useEffect(() => setText(value.toUpperCase()), [value]);
  const commit = (raw: string) => {
    const hex = normalizeHex(raw);
    if (!hex) {
      setText(value.toUpperCase());
      return;
    }
    const base = basePalette(appearance.theme, scheme)[which];
    setCustom(which, hex === base ? null : hex);
  };
  const label = which === "bg" ? "Background colour" : "Foreground colour";
  return (
    <div className="flex h-9 items-center gap-2 rounded-full border border-line-strong pr-3 pl-1.5">
      <label className="relative inline-flex h-6 w-6 cursor-pointer overflow-hidden rounded-full border border-line-strong" style={{ background: value }}>
        <span className="sr-only">{label} picker</span>
        <input type="color" value={value} onChange={(e) => commit(e.target.value)} className="absolute inset-0 cursor-pointer opacity-0" />
      </label>
      <input
        value={text}
        aria-label={label}
        onChange={(e) => setText(e.target.value)}
        onBlur={(e) => commit(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
        className="w-[78px] bg-transparent font-mono text-[13px] uppercase outline-none"
        maxLength={7}
        spellCheck={false}
      />
    </div>
  );
}

export function AppearanceSection() {
  const a = useAppearance((s) => s.appearance);
  const scheme = useAppearance((s) => s.scheme);
  const set = useAppearance((s) => s.set);
  const reset = useAppearance((s) => s.resetCustom);
  const hasCustom = Boolean(a.custom[scheme]?.bg || a.custom[scheme]?.fg);

  return (
    <>
      <PageTitle>Appearance</PageTitle>
      <SubTitle>Visual style</SubTitle>
      <Card className="mb-4">
        <Row label="Mode" desc="System follows your Windows setting.">
          <div className="flex gap-3" role="radiogroup" aria-label="Mode">
            {(["system", "light", "dark"] as Mode[]).map((m) => (
              <ModeTile key={m} mode={m} current={a.mode} onClick={() => set({ mode: m })} />
            ))}
          </div>
        </Row>
      </Card>
      <Card>
        <Row label="Theme">
          <DropdownMenu.Root>
            <DropdownMenu.Trigger asChild>
              <button type="button" className="flex h-9 min-w-[170px] items-center gap-2 rounded-full border border-line-strong pr-3 pl-3 text-[14px] hover:bg-hover">
                <span className="text-[12px] font-semibold text-accent">Aa</span>
                <span className="flex-1 text-left">{THEMES[a.theme].label}</span>
                <ChevronDown size={15} className="text-subtle" />
              </button>
            </DropdownMenu.Trigger>
            <DropdownMenu.Portal>
              <DropdownMenu.Content className="menu pop-in w-[240px]" align="end" sideOffset={6}>
                <DropdownMenu.RadioGroup value={a.theme} onValueChange={(v) => set({ theme: v as ThemeName })}>
                  {(Object.keys(THEMES) as ThemeName[]).map((t) => (
                    <DropdownMenu.RadioItem key={t} value={t} className="menu-item">
                      <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-md border border-line text-[11px] font-semibold" style={{ background: THEMES[t][scheme].bg, color: THEMES[t][scheme].fg }}>
                        Aa
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="block">{THEMES[t].label}</span>
                        <span className="block text-[12px] text-subtle">{THEMES[t].description}</span>
                      </span>
                      <DropdownMenu.ItemIndicator>
                        <Check size={15} />
                      </DropdownMenu.ItemIndicator>
                    </DropdownMenu.RadioItem>
                  ))}
                </DropdownMenu.RadioGroup>
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
        </Row>
        <Row label="Accent" desc="Used for focus rings, links, toggles and highlights.">
          <DropdownMenu.Root>
            <DropdownMenu.Trigger asChild>
              <button type="button" className="flex h-9 items-center gap-2 rounded-full border border-line-strong px-3 text-[14px] hover:bg-hover">
                <span className="h-3.5 w-3.5 rounded-full" style={{ background: ACCENTS[a.accent][scheme] }} />
                {ACCENTS[a.accent].label}
                <ChevronDown size={15} className="text-subtle" />
              </button>
            </DropdownMenu.Trigger>
            <DropdownMenu.Portal>
              <DropdownMenu.Content className="menu pop-in min-w-[170px]" align="end" sideOffset={6}>
                <DropdownMenu.RadioGroup value={a.accent} onValueChange={(v) => set({ accent: v as AccentName })}>
                  {(Object.keys(ACCENTS) as AccentName[]).map((k) => (
                    <DropdownMenu.RadioItem key={k} value={k} className="menu-item">
                      <span className="h-3.5 w-3.5 rounded-full" style={{ background: ACCENTS[k][scheme] }} />
                      <span className="flex-1">{ACCENTS[k].label}</span>
                      <DropdownMenu.ItemIndicator>
                        <Check size={15} />
                      </DropdownMenu.ItemIndicator>
                    </DropdownMenu.RadioItem>
                  ))}
                </DropdownMenu.RadioGroup>
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
        </Row>
        <Row label="Background">
          <ColorField which="bg" />
        </Row>
        <Row label="Foreground">
          <ColorField which="fg" />
        </Row>
      </Card>
      <div className="mt-3 flex items-center justify-between gap-3 px-1">
        <p className="text-[12.5px] text-subtle">Custom colours apply to {scheme} mode and are saved on this PC.</p>
        <Button size="sm" variant="ghost" onClick={reset} disabled={!hasCustom}>
          <RotateCcw size={14} /> Reset colours
        </Button>
      </div>
    </>
  );
}

export function PersonalizationSection() {
  const settings = useApp((s) => s.settings);
  const update = useApp((s) => s.updateSettings);
  const [text, setText] = useState(settings?.custom_instructions ?? "");
  const [saved, setSaved] = useState(false);
  useEffect(() => setText(settings?.custom_instructions ?? ""), [settings?.custom_instructions]);
  const dirty = settings ? text !== settings.custom_instructions : false;
  return (
    <>
      <PageTitle>Personalization</PageTitle>
      {!settings ? (
        <SettingsError />
      ) : (
        <>
          <Card className="mb-6">
            <Row label="Your name" htmlFor="user-name">
              <NameInput />
            </Row>
          </Card>
          <SubTitle>Custom instructions</SubTitle>
          <p className="-mt-1 mb-3 text-[13px] text-subtle">What should Relay know about you and how you like things done? This applies to every chat.</p>
          <textarea
            value={text}
            onChange={(e) => {
              setText(e.target.value);
              setSaved(false);
            }}
            rows={8}
            maxLength={4000}
            placeholder="e.g. I keep work files in D:\Work. Prefer short answers. Never empty the Recycle Bin."
            className={cn(fieldClass, "scroll-thin resize-y rounded-2xl leading-6")}
            aria-label="Custom instructions"
          />
          <div className="mt-3 flex items-center justify-end gap-3">
            <span className="text-[12.5px] text-subtle">{saved ? "Saved" : `${text.length} / 4000`}</span>
            <Button variant="ghost" size="sm" disabled={!dirty} onClick={() => setText(settings.custom_instructions)}>
              Cancel
            </Button>
            <Button
              variant="primary"
              size="sm"
              disabled={!dirty}
              onClick={async () => {
                await update({ custom_instructions: text });
                setSaved(true);
              }}
            >
              Save
            </Button>
          </div>
        </>
      )}
    </>
  );
}

export function ArchivedSection() {
  const archived = useApp((s) => s.archived);
  const { loadArchived, setArchived, askDeleteConv, openChat } = useApp.getState();
  useEffect(() => {
    void loadArchived();
  }, [loadArchived]);
  return (
    <>
      <PageTitle>Archived chats</PageTitle>
      {archived === null ? (
        <SettingsSkeleton />
      ) : archived.length === 0 ? (
        <p className="rounded-2xl border border-dashed border-line px-4 py-8 text-center text-[14px] text-subtle">No archived chats.</p>
      ) : (
        <Card>
          {archived.map((c) => (
            <div key={c.id} className="flex items-center gap-3 py-2.5">
              <MessageCircle size={16} className="shrink-0 text-subtle" aria-hidden="true" />
              <button type="button" className="min-w-0 flex-1 truncate text-left text-[14px] hover:underline" onClick={() => void openChat(c.id)}>
                {c.title || "New chat"}
              </button>
              <span className="hidden shrink-0 text-[12.5px] text-subtle sm:inline">{new Date(c.updated_at).toLocaleDateString()}</span>
              <Button size="sm" variant="ghost" onClick={() => setArchived(c.id, false)} aria-label={`Unarchive ${c.title}`}>
                <ArchiveRestore size={15} /> Unarchive
              </Button>
              <Button size="sm" variant="ghost" className="text-danger hover:text-danger" onClick={() => askDeleteConv(c.id)} aria-label={`Delete ${c.title}`}>
                <Trash size={15} />
              </Button>
            </div>
          ))}
        </Card>
      )}
    </>
  );
}

export const SHORTCUTS: [string, string][] = [
  ["Search chats", `${MOD}+K`],
  ["New chat", `${MOD}+Shift+O`],
  ["Toggle sidebar", `${MOD}+Shift+S`],
  ["Open settings", `${MOD}+,`],
  ["Send message", "Enter"],
  ["New line", "Shift+Enter"],
  ["Close dialog or settings", "Esc"],
];

export function KeyboardSection() {
  const status = useApp((s) => s.status);
  const hotkey = status?.kill_hotkey || useApp.getState().settings?.hotkey || "Ctrl+Alt+Shift+Esc";
  return (
    <>
      <PageTitle>Keyboard</PageTitle>
      <Card>
        {[...SHORTCUTS, ["Emergency stop (anywhere in Windows)", hotkey] as [string, string]].map(([label, keys]) => (
          <Row key={label} label={<span className="font-normal">{label}</span>}>
            <span className="flex gap-1">
              {keys.split("+").map((k) => (
                <kbd key={k} className="min-w-[26px] rounded-md border border-line-strong px-1.5 py-0.5 text-center font-sans text-[12.5px] text-muted">
                  {k}
                </kbd>
              ))}
            </span>
          </Row>
        ))}
      </Card>
    </>
  );
}

export function DataSection() {
  const settings = useApp((s) => s.settings);
  const toast = useApp((s) => s.toast);
  const open = (k: LinkKind) => api.openLink(k).catch((e) => toast(`Could not open the folder: ${e instanceof Error ? e.message : e}`, "error"));
  const rows: { k: LinkKind; label: string; desc: string; path?: string }[] = [
    { k: "data", label: "Relay data", desc: "Chats, projects and settings", path: settings?.paths.data },
    { k: "audit", label: "Activity log", desc: "Every action Relay took on this PC", path: settings?.paths.audit },
    { k: "logs", label: "Diagnostic logs", desc: "Useful when something goes wrong", path: settings?.paths.logs },
  ];
  return (
    <>
      <PageTitle>Data and storage</PageTitle>
      {!settings ? (
        <SettingsError />
      ) : (
        <>
          <Card>
            {rows.map((r) => (
              <Row key={r.k} label={r.label} desc={<span className="break-all font-mono text-[12px]">{r.path}</span>}>
                <Button size="sm" variant="secondary" onClick={() => void open(r.k)}>
                  <FolderOpen size={15} /> Open
                </Button>
              </Row>
            ))}
          </Card>
          <p className="mt-3 px-1 text-[12.5px] text-subtle">Everything stays on this PC. Relay never uploads your chats.</p>
        </>
      )}
    </>
  );
}

export function AboutSection() {
  const settings = useApp((s) => s.settings);
  const status = useApp((s) => s.status);
  return (
    <>
      <PageTitle>About</PageTitle>
      <div className="mb-6 flex items-center gap-4">
        <span className="inline-flex h-14 w-14 items-center justify-center rounded-2xl border border-line bg-surface">
          <RelayMark size={30} />
        </span>
        <div>
          <div className="text-[18px] font-semibold">Relay</div>
          <div className="text-[13.5px] text-subtle">Your hands on this PC. Version {settings?.version ?? "…"}</div>
        </div>
      </div>
      <Card>
        <Row label="Version">
          <span className="text-[14px] text-muted">{settings?.version ?? "…"}</span>
        </Row>
        <Row label="Emergency stop" desc="Stops any running task immediately, from anywhere in Windows.">
          <kbd className="rounded-lg border border-line-strong px-2.5 py-1 font-sans text-[13px] text-muted">{status?.kill_hotkey || settings?.hotkey || "…"}</kbd>
        </Row>
        <Row label="Status">
          <span className="text-[14px] text-muted">{status ? status.detail || (status.ready ? "Ready" : "Not ready") : "Checking…"}</span>
        </Row>
        <Row label="Backend">
          <span className="text-[14px] text-muted">{isTauri() ? "Relay app" : "Preview (simulated)"}</span>
        </Row>
      </Card>
      <SubTitle>Help</SubTitle>
      <Card>
        <Row label="Getting started" desc="Type what you want done, like “Clean up my Downloads folder”. Relay works on your PC and shows each step it takes." />
        <Row label="Stopping a task" desc="Use the Stop button, the tray menu, or the emergency stop keys." />
      </Card>
    </>
  );
}
