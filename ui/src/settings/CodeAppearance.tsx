// Settings > Appearance > Code: a searchable code-theme picker per colour scheme with a live preview
// (a short TypeScript snippet with a two-line diff), and the code font field.
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { Check, ChevronsUpDown, Search } from "lucide-react";
import { codeTheme, codeThemesFor, type CodeTheme, type CodeThemeScheme } from "@/lib/codeThemes";
import { mix, sanitizeFontName, THEMES, CODE_FONT_MAX } from "@/lib/theme";
import { useAppearance } from "@/store/appearance";
import { cn } from "@/lib/utils";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";

/** Background / text colour a theme shows in its preview (Relay's own follows the app theme). */
function previewColours(t: CodeTheme, appTheme: keyof typeof THEMES): { bg: string; fg: string } {
  if (t.bg && t.fg) return { bg: t.bg, fg: t.fg };
  const sw = THEMES[appTheme][t.scheme];
  return { bg: t.scheme === "light" ? sw.surface : mix(sw.bg, "#000000", 0.3), fg: sw.fg };
}

export function CodeThemePicker({ scheme }: { scheme: CodeThemeScheme }) {
  const key = scheme === "light" ? "codeThemeLight" : "codeThemeDark";
  const value = useAppearance((s) => s.appearance[key]);
  const appTheme = useAppearance((s) => s.appearance.theme);
  const set = useAppearance((s) => s.set);
  const current = codeTheme(value, scheme);
  const label = scheme === "light" ? "Light theme" : "Dark theme";
  return (
    <div className="min-w-0" data-testid={`code-theme-${scheme}`}>
      <div className="mb-1.5 text-ui font-medium text-fg" aria-hidden="true">
        {label}
      </div>
      <ThemeCombobox scheme={scheme} label={label} current={current} appTheme={appTheme} onSelect={(id) => set({ [key]: id })} />
      <CodePreview theme={current} appTheme={appTheme} />
    </div>
  );
}

function ThemeCombobox({
  scheme,
  label,
  current,
  appTheme,
  onSelect,
}: {
  scheme: CodeThemeScheme;
  label: string;
  current: CodeTheme;
  appTheme: keyof typeof THEMES;
  onSelect: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const all = useMemo(() => codeThemesFor(scheme), [scheme]);
  const list = useMemo(() => {
    const t = q.trim().toLowerCase();
    return t ? all.filter((x) => x.label.toLowerCase().includes(t)) : all;
  }, [all, q]);
  const [active, setActive] = useState(0);
  const listId = useId();
  const listRef = useRef<HTMLUListElement>(null);

  useEffect(() => {
    if (!open) return;
    setQ("");
    setActive(Math.max(0, all.findIndex((x) => x.id === current.id)));
  }, [open, all, current.id]);
  useEffect(() => setActive((a) => Math.min(a, Math.max(0, list.length - 1))), [list.length]);
  useEffect(() => {
    if (open) listRef.current?.querySelector(`[data-index="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [open, active]);

  const choose = (t: CodeTheme | undefined) => {
    if (!t) return;
    onSelect(t.id);
    setOpen(false);
  };
  const onKeyDown = (e: React.KeyboardEvent) => {
    const last = list.length - 1;
    if (e.key === "ArrowDown") setActive((a) => Math.min(last, a + 1));
    else if (e.key === "ArrowUp") setActive((a) => Math.max(0, a - 1));
    else if (e.key === "Home") setActive(0);
    else if (e.key === "End") setActive(last);
    else if (e.key === "Enter") choose(list[active]);
    else return;
    e.preventDefault();
  };
  const optionId = (i: number) => `${listId}-o${i}`;

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          role="combobox"
          aria-haspopup="listbox"
          aria-expanded={open}
          aria-label={`${label}: ${current.label}`}
          className="flex h-9 w-full items-center gap-2 rounded-md border border-line-strong bg-surface px-2.5 text-left text-ui text-fg outline-none transition-[border-color] duration-(--relay-dur-fast) hover:border-fg-muted focus-visible:border-brand"
          data-testid={`code-theme-${scheme}-trigger`}
        >
          <Swatch theme={current} appTheme={appTheme} />
          <span className="min-w-0 flex-1 truncate">{current.label}</span>
          <ChevronsUpDown className="size-4 shrink-0 text-fg-muted" aria-hidden="true" />
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-(--radix-popover-trigger-width) min-w-[240px] p-1.5">
        <div className="relative mb-1">
          <Search className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-fg-muted" aria-hidden="true" />
          <input
            autoFocus
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={onKeyDown}
            role="combobox"
            aria-expanded="true"
            aria-controls={listId}
            aria-autocomplete="list"
            aria-activedescendant={list.length ? optionId(active) : undefined}
            aria-label={`Search ${label.toLowerCase()}s`}
            placeholder="Search themes"
            className="h-8 w-full rounded-md border border-line-strong bg-surface pr-2 pl-8 text-ui outline-none placeholder:text-fg-muted focus-visible:border-brand"
          />
        </div>
        <ul ref={listRef} id={listId} role="listbox" aria-label={label} className="max-h-64 overflow-y-auto scroll-thin">
          {list.map((t, i) => {
            const selected = t.id === current.id;
            return (
              <li
                key={t.id}
                id={optionId(i)}
                data-index={i}
                role="option"
                aria-selected={selected}
                onMouseMove={() => setActive(i)}
                onClick={() => choose(t)}
                className={cn("flex h-8 cursor-default items-center gap-2 rounded-md px-2 text-ui", i === active && "bg-hover")}
              >
                <Swatch theme={t} appTheme={appTheme} />
                <span className="min-w-0 flex-1 truncate">{t.label}</span>
                {selected && <Check className="size-4 shrink-0 text-brand-text" aria-hidden="true" />}
              </li>
            );
          })}
          {list.length === 0 && <li className="px-2 py-2 text-caption text-fg-muted">No themes match “{q}”.</li>}
        </ul>
      </PopoverContent>
    </Popover>
  );
}

function Swatch({ theme, appTheme }: { theme: CodeTheme; appTheme: keyof typeof THEMES }) {
  const c = previewColours(theme, appTheme);
  return (
    <span className="inline-flex size-5 shrink-0 items-center justify-center rounded-sm border border-line" style={{ background: c.bg }} aria-hidden="true">
      <span className="h-0.5 w-2.5 rounded-full" style={{ background: c.fg }} />
    </span>
  );
}

const SNIPPET = `type User = { id: string };

async function load(id: string) {
  const res = await fetch(\`/u/\${id}\`);
  return res.json();
  return (await res.json()) as User;
}`;
/** 0-based line indexes of the diff in SNIPPET: one line removed, one added. */
const REMOVED = 4;
const ADDED = 5;

type Line = { content: string; color?: string }[];

function CodePreview({ theme, appTheme }: { theme: CodeTheme; appTheme: keyof typeof THEMES }) {
  const [tokens, setTokens] = useState<{ theme: string; lines: Line[] } | null>(null);
  useEffect(() => {
    let live = true;
    import("@/lib/highlight")
      .then((m) => m.tokenize(SNIPPET, "typescript", theme.shiki))
      .then((lines) => {
        if (live && lines) setTokens({ theme: theme.shiki, lines });
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [theme.shiki]);
  const c = previewColours(theme, appTheme);
  const lines: Line[] = tokens?.theme === theme.shiki ? tokens.lines : SNIPPET.split("\n").map((l) => [{ content: l }]);
  return (
    <figure
      className="mt-2 overflow-x-auto rounded-lg border border-line/70 py-2 text-[12px] leading-5 scroll-thin"
      style={{ background: c.bg, color: c.fg, fontFamily: "var(--relay-font-code, var(--relay-font-mono))", fontVariantLigatures: "none" }}
      aria-label={`${theme.label} preview`}
      data-testid="code-theme-preview"
      data-theme-id={theme.id}
      data-highlighted={tokens?.theme === theme.shiki ? "true" : "false"}
    >
      <pre className="m-0 min-w-max">
        {lines.map((line, i) => {
          const removed = i === REMOVED;
          const added = i === ADDED;
          return (
            <div
              key={i}
              className="flex pr-4"
              style={removed ? { background: "rgba(229, 83, 75, 0.18)" } : added ? { background: "rgba(70, 170, 90, 0.18)" } : undefined}
            >
              <span className="w-8 shrink-0 pr-2 text-right select-none" style={{ opacity: 0.5 }} aria-hidden="true">
                {i < ADDED ? i + 1 : i}
              </span>
              <span className="w-4 shrink-0 select-none" aria-hidden="true" style={{ opacity: removed || added ? 0.9 : 0 }}>
                {removed ? "-" : added ? "+" : " "}
              </span>
              <code className="whitespace-pre">
                {removed && <span className="sr-only">Removed: </span>}
                {added && <span className="sr-only">Added: </span>}
                {line.length === 0 || (line.length === 1 && line[0]!.content === "") ? " " : line.map((t, j) => <span key={j} style={t.color ? { color: t.color } : undefined}>{t.content}</span>)}
              </code>
            </div>
          );
        })}
      </pre>
    </figure>
  );
}

export function CodeFontField({ id }: { id: string }) {
  const saved = useAppearance((s) => s.appearance.codeFont);
  const set = useAppearance((s) => s.set);
  const [v, setV] = useState(saved);
  useEffect(() => setV(saved), [saved]);
  const commit = () => {
    const next = sanitizeFontName(v);
    setV(next);
    if (next !== saved) set({ codeFont: next });
  };
  return (
    <Input
      id={id}
      value={v}
      onChange={(e) => setV(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
      placeholder="Geist Mono (default)"
      className="w-[220px] max-w-full font-mono text-[13px]"
      maxLength={CODE_FONT_MAX}
      spellCheck={false}
      autoComplete="off"
    />
  );
}
