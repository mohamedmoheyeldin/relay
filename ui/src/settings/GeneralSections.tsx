import { useEffect, useId, useState } from "react";
import { ArchiveRestore, FolderOpen, MessageSquare, RotateCcw, Trash2 } from "lucide-react";
import { useSettings, useSetSettings, useStatus, useConversations, useUpdateConversation, useDeleteConversation } from "@/hooks/queries";
import { useAppearance } from "@/store/appearance";
import { useApp } from "@/store/app";
import { api, errorText, isTauri } from "@/lib/backend";
import { ACCENTS, effectiveSwatch, normalizeHex, THEMES, type AccentName, type Density, type Mode, type TaskView, type TextSize, type ThemeName, type UiFont } from "@/lib/theme";
import type { ApprovalMode, LinkKind } from "@/lib/types";
import { OTHER_KEYS, SHORTCUTS } from "@/lib/platform";
import { go, paths } from "@/lib/nav";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Switch } from "@/components/ui/switch";
import { Segmented } from "@/components/ui/toggle-group";
import { Kbd, Shortcut } from "@/components/ui/kbd";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { EmptyState, RelayMark } from "@/components/common";
import { Card, LoadState, PageTitle, Row, SectionTitle } from "./parts";
import { CodeFontField, CodeThemePicker } from "./CodeAppearance";

const TASK_VIEW_DESC: Record<TaskView, string> = {
  simple: "Simple: only the result and Relay's final message.",
  detailed: "Detailed: the result and the list of steps.",
  verbose: "Verbose: every step opened with its details, plus the raw log.",
};

function NameInput({ id }: { id: string }) {
  const settings = useSettings();
  const set = useSetSettings();
  const name = settings.data?.user_name ?? "";
  const [v, setV] = useState(name);
  useEffect(() => setV(name), [name]);
  const commit = () => {
    if (v.trim() !== name) set.mutate({ user_name: v.trim() });
  };
  return (
    <Input id={id} value={v} onChange={(e) => setV(e.target.value)} onBlur={commit} onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()} placeholder="Your name" className="w-[220px] max-w-full" maxLength={60} />
  );
}

export function GeneralSection() {
  const settings = useSettings();
  const status = useStatus();
  const set = useSetSettings();
  const nameId = useId();
  const autoId = useId();
  if (!settings.data)
    return (
      <>
        <PageTitle>General</PageTitle>
        <LoadState error={settings.error} retry={() => void settings.refetch()} />
      </>
    );
  const s = settings.data;
  return (
    <>
      <PageTitle>General</PageTitle>
      <Card>
        <Row label="Your name" desc="Shown in the sidebar and the greeting." htmlFor={nameId}>
          <NameInput id={nameId} />
        </Row>
        <Row label="Set up Relay Local AI automatically" desc="Relay downloads and starts its local AI by itself when this PC can run it." htmlFor={autoId}>
          <Switch id={autoId} checked={s.auto_setup} onCheckedChange={(v) => set.mutate({ auto_setup: v })} />
        </Row>
      </Card>
      <CustomInstructions saved={s.custom_instructions} />
      <SectionTitle description="Plan steps follow the confirmation level in AI & Privacy. This setting covers agent mode, where a cloud provider drives the desktop directly.">
        Agent mode approvals
      </SectionTitle>
      <Card>
        <Row label="Ask before acting in agent mode" stack>
          <Segmented<ApprovalMode>
            label="Ask before acting in agent mode"
            value={s.approval_mode}
            onValueChange={(v) => set.mutate({ approval_mode: v })}
            options={[
              { value: "off", label: "Never ask" },
              { value: "destructive", label: "Destructive actions" },
              { value: "always", label: "Every action" },
            ]}
          />
        </Row>
        <Row label="Emergency stop" desc="Press this anywhere to stop Relay at once. The tray menu has Stop too.">
          <span className="flex gap-1">
            {(status.data?.kill_hotkey || s.hotkey).split("+").map((k) => (
              <Kbd key={k}>{k}</Kbd>
            ))}
          </span>
        </Row>
      </Card>
    </>
  );
}

function ModeTile({ mode, current }: { mode: Mode; current: Mode }) {
  const light = THEMES.relay.light;
  const dark = THEMES.relay.dark;
  const pane = (c: typeof light, clip?: string) => (
    <span className="absolute inset-0 flex" style={{ background: c.bg, clipPath: clip }}>
      <span className="w-[28%] space-y-1 p-1.5" style={{ background: c.sidebar }}>
        <span className="block h-1 w-3/4 rounded-full" style={{ background: c.muted, opacity: 0.5 }} />
        <span className="block h-1 w-1/2 rounded-full" style={{ background: c.muted, opacity: 0.5 }} />
      </span>
      <span className="flex flex-1 flex-col items-center justify-center gap-1.5 p-2">
        <span className="block h-1.5 w-1/2 rounded-full" style={{ background: c.fg, opacity: 0.7 }} />
        <span className="block h-3 w-4/5 rounded-[4px] border" style={{ borderColor: c.muted, background: c.surface }} />
      </span>
    </span>
  );
  const label = mode === "system" ? "Match system" : mode === "light" ? "Light" : "Dark";
  return (
    <label className="group flex cursor-default flex-col items-center gap-2">
      <span
        className={cn(
          "relative block h-[60px] w-[92px] overflow-hidden rounded-lg border transition-[border-color,box-shadow] duration-(--relay-dur-fast)",
          current === mode ? "border-brand shadow-[0_0_0_1px_var(--relay-accent)]" : "border-line group-hover:border-line-strong/70",
        )}
      >
        {mode === "light" && pane(light)}
        {mode === "dark" && pane(dark)}
        {mode === "system" && (
          <>
            {pane(light)}
            {pane(dark, "polygon(55% 0, 100% 0, 100% 100%, 45% 100%)")}
          </>
        )}
      </span>
      <span className="flex items-center gap-1.5 text-caption text-fg-2">
        <RadioGroupItem value={mode} aria-label={label} />
        {label}
      </span>
    </label>
  );
}

function ColorField({ which }: { which: "bg" | "fg" }) {
  const appearance = useAppearance((s) => s.appearance);
  const scheme = useAppearance((s) => s.scheme);
  const setCustom = useAppearance((s) => s.setCustom);
  const value = effectiveSwatch(appearance, scheme)[which];
  const [text, setText] = useState(value.toUpperCase());
  useEffect(() => setText(value.toUpperCase()), [value]);
  const commit = (raw: string) => {
    const hex = normalizeHex(raw);
    if (!hex) {
      setText(value.toUpperCase());
      return;
    }
    const base = THEMES[appearance.theme][scheme][which];
    setCustom(which, hex === base ? null : hex);
  };
  const label = which === "bg" ? "Background colour" : "Text colour";
  return (
    <div className="flex h-9 items-center gap-2 rounded-md border border-line-strong/70 bg-surface pr-2 pl-1.5">
      <label className="relative inline-flex size-6 cursor-pointer overflow-hidden rounded-sm border border-line-strong" style={{ background: value }}>
        <span className="sr-only">{label}: pick a colour</span>
        <input type="color" value={value} onChange={(e) => commit(e.target.value)} className="absolute inset-0 cursor-pointer opacity-0" />
      </label>
      <input
        value={text}
        aria-label={`${label} (hex)`}
        onChange={(e) => setText(e.target.value)}
        onBlur={(e) => commit(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
        className="w-[76px] bg-transparent font-mono text-[12.5px] uppercase outline-none"
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
  const codeFontId = useId();
  return (
    <>
      <PageTitle>Appearance</PageTitle>
      <SectionTitle id="mode-label">Mode</SectionTitle>
      <RadioGroup aria-labelledby="mode-label" value={a.mode} onValueChange={(v) => set({ mode: v as Mode })} className="flex flex-wrap gap-5">
        {(["system", "light", "dark"] as Mode[]).map((m) => (
          <ModeTile key={m} mode={m} current={a.mode} />
        ))}
      </RadioGroup>

      <SectionTitle id="theme-label">Theme</SectionTitle>
      <RadioGroup aria-labelledby="theme-label" value={a.theme} onValueChange={(v) => set({ theme: v as ThemeName })} className="grid gap-3 sm:grid-cols-3">
        {(Object.keys(THEMES) as ThemeName[]).map((t) => {
          const sw = THEMES[t][scheme];
          return (
            <label key={t} className={cn("flex cursor-default items-center gap-3 rounded-lg border bg-surface p-3", a.theme === t ? "border-brand" : "border-line hover:border-line-strong/60")}>
              <RadioGroupItem value={t} aria-label={THEMES[t].label} />
              <span className="inline-flex size-8 shrink-0 items-center justify-center rounded-md border border-line font-serif text-[15px]" style={{ background: sw.bg, color: sw.fg }} aria-hidden="true">
                Aa
              </span>
              <span className="min-w-0">
                <span className="block text-ui font-medium">{THEMES[t].label}</span>
                <span className="block text-caption text-fg-muted">{THEMES[t].description}</span>
              </span>
            </label>
          );
        })}
      </RadioGroup>

      <SectionTitle id="accent-label" description="Used for the send button, focus rings, links and highlights.">
        Accent
      </SectionTitle>
      <RadioGroup aria-labelledby="accent-label" value={a.accent} onValueChange={(v) => set({ accent: v as AccentName })} className="flex flex-wrap gap-4">
        {(Object.keys(ACCENTS) as AccentName[]).map((k) => (
          <label key={k} className="flex cursor-default items-center gap-2 text-ui">
            <RadioGroupItem value={k} aria-label={ACCENTS[k].label} />
            <span className="size-4 rounded-full border border-line" style={{ background: ACCENTS[k][scheme] }} aria-hidden="true" />
            {ACCENTS[k].label}
          </label>
        ))}
      </RadioGroup>

      <SectionTitle>Interface</SectionTitle>
      <Card>
        <Row label="Interface font" desc="Used for menus, buttons, settings and answers in sans.">
          <Segmented<UiFont>
            label="Interface font"
            size="sm"
            value={a.uiFont}
            onValueChange={(v) => set({ uiFont: v })}
            options={[
              { value: "inter", label: "Inter" },
              { value: "system", label: "System" },
              { value: "atkinson", label: "Atkinson Hyperlegible" },
            ]}
          />
        </Row>
        <Row label="Text size" desc="Size of the conversation: answers, your messages and code.">
          <Segmented<TextSize>
            label="Text size"
            size="sm"
            value={a.textSize}
            onValueChange={(v) => set({ textSize: v })}
            options={[
              { value: "small", label: "Small" },
              { value: "medium", label: "Medium" },
              { value: "large", label: "Large" },
            ]}
          />
        </Row>
        <Row label="Density" desc="Compact fits more rows in the sidebar, task steps and settings.">
          <Segmented<Density>
            label="Density"
            size="sm"
            value={a.density}
            onValueChange={(v) => set({ density: v })}
            options={[
              { value: "comfortable", label: "Comfortable" },
              { value: "compact", label: "Compact" },
            ]}
          />
        </Row>
        <Row label="Default task view" desc={TASK_VIEW_DESC[a.taskView]}>
          <Segmented<TaskView>
            label="Default task view"
            size="sm"
            value={a.taskView}
            onValueChange={(v) => set({ taskView: v })}
            options={[
              { value: "simple", label: "Simple" },
              { value: "detailed", label: "Detailed" },
              { value: "verbose", label: "Verbose" },
            ]}
          />
        </Row>
      </Card>

      <SectionTitle description="Colours for code in answers. Relay uses the light theme in light mode and the dark theme in dark mode.">Code</SectionTitle>
      <div className="grid gap-5 sm:grid-cols-2">
        <CodeThemePicker scheme="light" />
        <CodeThemePicker scheme="dark" />
      </div>
      <Card className="mt-4">
        <Row label="Code font" desc="Any monospace font installed on this PC. Leave empty for Geist Mono." htmlFor={codeFontId}>
          <CodeFontField id={codeFontId} />
        </Row>
      </Card>

      <SectionTitle>Text and colours</SectionTitle>
      <Card>
        <Row label="Answer font" desc="Serif reads like a letter; sans is denser for logs and lists. Menus and tool output always use sans.">
          <Segmented label="Answer font" size="sm" value={a.prose} onValueChange={(v) => set({ prose: v })} options={[{ value: "sans", label: "Sans" }, { value: "serif", label: "Serif" }]} />
        </Row>
        <Row label="Background" desc={`Your own background for ${scheme} mode.`}>
          <ColorField which="bg" />
        </Row>
        <Row label="Text" desc="Secondary and muted text are derived from it and kept readable.">
          <ColorField which="fg" />
        </Row>
      </Card>
      <div className="mt-3 flex items-center justify-between gap-3 px-1">
        <p className="text-caption text-fg-muted">Appearance is saved on this PC.</p>
        <Button size="sm" variant="ghost" onClick={reset} disabled={!hasCustom}>
          <RotateCcw aria-hidden="true" /> Reset colours
        </Button>
      </div>
    </>
  );
}

/** Custom instructions (part of General; it was a separate "Personalization" page before). */
function CustomInstructions({ saved: savedValue }: { saved: string }) {
  const set = useSetSettings();
  const [text, setText] = useState(savedValue);
  const [saved, setSaved] = useState(false);
  useEffect(() => setText(savedValue), [savedValue]);
  const dirty = text !== savedValue;
  return (
    <>
      <SectionTitle id="custom-instructions" description="What should Relay know about you and how you like things done? Applies to every chat.">
        Custom instructions
      </SectionTitle>
      <Textarea
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          setSaved(false);
        }}
        rows={5}
        maxLength={4000}
        placeholder="e.g. I keep work files in D:\Work. Prefer short answers. Never empty the Recycle Bin."
        aria-label="Custom instructions"
        className="rounded-lg"
      />
      <div className="mt-3 flex items-center justify-end gap-3">
        <span className="text-caption text-fg-muted" aria-live="polite">
          {saved ? "Saved" : `${text.length} / 4000`}
        </span>
        <Button variant="ghost" size="sm" disabled={!dirty} onClick={() => setText(savedValue)}>
          Cancel
        </Button>
        <Button variant="primary" size="sm" disabled={!dirty} onClick={() => set.mutate({ custom_instructions: text }, { onSuccess: () => setSaved(true) })}>
          Save
        </Button>
      </div>
    </>
  );
}

export function ArchivedSection() {
  const archived = useConversations(true);
  const update = useUpdateConversation();
  const del = useDeleteConversation();
  return (
    <>
      <PageTitle>Archived chats</PageTitle>
      {!archived.data ? (
        <LoadState error={archived.error} retry={() => void archived.refetch()} />
      ) : archived.data.length === 0 ? (
        <EmptyState title="No archived chats">Archive a chat from its menu to tidy the sidebar without deleting it.</EmptyState>
      ) : (
        <Card>
          {archived.data.map((c) => (
            <div key={c.id} className="flex items-center gap-3 py-2.5">
              <MessageSquare className="size-4 shrink-0 text-fg-muted" aria-hidden="true" />
              <button type="button" className="min-w-0 flex-1 truncate text-left text-ui hover:underline" onClick={() => go(paths.chat(c.id))}>
                {c.title || "New chat"}
              </button>
              <span className="hidden shrink-0 text-caption text-fg-muted sm:inline">{new Date(c.updated_at).toLocaleDateString()}</span>
              <Button size="sm" variant="ghost" onClick={() => update.mutate({ id: c.id, patch: { archived: false } })} aria-label={`Restore ${c.title || "chat"}`}>
                <ArchiveRestore aria-hidden="true" /> Restore
              </Button>
              <Button size="sm" variant="danger-ghost" onClick={() => useApp.getState().askDeleteConv(c, () => del.mutate(c.id))} aria-label={`Delete ${c.title || "chat"}`}>
                <Trash2 aria-hidden="true" />
              </Button>
            </div>
          ))}
        </Card>
      )}
    </>
  );
}

export function KeyboardSection() {
  const status = useStatus();
  const settings = useSettings();
  const hotkey = status.data?.kill_hotkey || settings.data?.hotkey || "Ctrl+Alt+Shift+Esc";
  return (
    <>
      <PageTitle>Keyboard</PageTitle>
      <Card>
        {SHORTCUTS.map((s) => (
          <Row key={s.id} label={<span className="font-normal">{s.label}</span>}>
            <Shortcut keys={s.keys} />
          </Row>
        ))}
        {OTHER_KEYS.map((s) => (
          <Row key={s.label} label={<span className="font-normal">{s.label}</span>}>
            <Shortcut keys={s.keys} />
          </Row>
        ))}
        <Row label={<span className="font-normal">Emergency stop (anywhere)</span>}>
          <span className="flex gap-0.5">
            {hotkey.split("+").map((k) => (
              <Kbd key={k}>{k}</Kbd>
            ))}
          </span>
        </Row>
      </Card>
      <p className="mt-3 px-1 text-caption text-fg-muted">In lists, menus and the task history use the arrow keys; F2 renames the focused chat and Delete asks to delete it.</p>
    </>
  );
}

export function DataSection() {
  const settings = useSettings();
  const toast = useApp((s) => s.toast);
  const open = (k: LinkKind) => api.openLink(k).catch((e) => toast(`Could not open the folder: ${errorText(e)}`, "error"));
  if (!settings.data)
    return (
      <>
        <PageTitle>Data and storage</PageTitle>
        <LoadState error={settings.error} retry={() => void settings.refetch()} />
      </>
    );
  const rows: { k: LinkKind; label: string; desc: string; path: string }[] = [
    { k: "data", label: "Relay data", desc: "Chats, projects, task history and settings", path: settings.data.paths.data },
    { k: "audit", label: "Activity log", desc: "Every action Relay took on this PC", path: settings.data.paths.audit },
    { k: "logs", label: "Diagnostic logs", desc: "Useful when something goes wrong", path: settings.data.paths.logs },
  ];
  return (
    <>
      <PageTitle description="Everything is stored on this PC. Relay never uploads your chats; cloud requests contain only what AI & Privacy allows.">Data and storage</PageTitle>
      <Card>
        {rows.map((r) => (
          <Row
            key={r.k}
            label={r.label}
            desc={
              <>
                {r.desc}
                <span className="mt-0.5 block font-mono text-[12px] break-all">{r.path}</span>
              </>
            }
          >
            <Button size="sm" variant="secondary" onClick={() => void open(r.k)} aria-label={`Open ${r.label} folder`}>
              <FolderOpen aria-hidden="true" /> Open
            </Button>
          </Row>
        ))}
      </Card>
    </>
  );
}

export function AboutSection() {
  const settings = useSettings();
  const status = useStatus();
  return (
    <>
      <PageTitle>About</PageTitle>
      <div className="mb-8 flex items-center gap-4">
        <span className="inline-flex size-14 items-center justify-center rounded-xl border border-line bg-surface shadow-card">
          <RelayMark size={30} />
        </span>
        <div>
          <div className="font-display text-[22px] leading-7">Relay</div>
          <div className="text-caption text-fg-muted">Your hands on this PC. Version {settings.data?.version ?? "…"}</div>
        </div>
      </div>
      <Card>
        <Row label="Version">
          <span className="text-ui text-fg-2">{settings.data?.version ?? "…"}</span>
        </Row>
        <Row label="Status">
          <span className="text-ui text-fg-2">{status.data ? status.data.detail || (status.data.ready ? "Ready" : "Not ready") : "Checking…"}</span>
        </Row>
        <Row label="Backend">
          <span className="text-ui text-fg-2">{isTauri() ? "Relay app" : "Preview (simulated backend)"}</span>
        </Row>
      </Card>
      <SectionTitle>How Relay works</SectionTitle>
      <Card>
        <Row label="Plans are proposals" desc="An AI proposes a plan. Relay runs each step on this PC only if it passes your permission settings." />
        <Row label="Verified means checked" desc="A step is verified only when Relay inspected its effect afterwards. Steps with nothing to check show as done." />
        <Row label="Stopping a task" desc="Use Stop, the tray menu, or the emergency stop keys." />
      </Card>
    </>
  );
}
