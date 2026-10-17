// Settings: a full page like a native preferences window. Left navigation with search, sections
// on the right; Esc returns to where you were. Code-split from the main bundle.
import { useEffect, useMemo, useRef, useState, type ComponentType } from "react";
import { useParams } from "react-router";
import { Archive, ArrowLeft, ChevronLeft, CircleHelp, Cpu, HardDrive, Keyboard, Palette, Plug, Search, Settings as Gear, ShieldCheck } from "lucide-react";
import { useApp } from "@/store/app";
import { useNarrow } from "@/hooks/dom";
import { useRovingList } from "@/hooks/roving";
import { go, paths } from "@/lib/nav";
import { cn } from "@/lib/utils";
import { AboutSection, AppearanceSection, ArchivedSection, DataSection, GeneralSection, KeyboardSection } from "./GeneralSections";
import { PrivacySection } from "./PrivacySection";
import { LocalAiSection } from "./LocalAiSection";
import { ConnectionsSection } from "./ConnectionsSection";

type Section = { id: string; label: string; icon: ComponentType<{ className?: string }>; keywords: string; group: string; render: ComponentType };

export const SECTIONS: Section[] = [
  { id: "general", label: "General", icon: Gear, group: "App", keywords: "name custom instructions about you emergency stop hotkey automatic setup agent approvals", render: GeneralSection },
  { id: "appearance", label: "Appearance", icon: Palette, group: "App", keywords: "theme dark light mode accent colour color background text font serif", render: AppearanceSection },
  { id: "privacy", label: "AI & Privacy", icon: ShieldCheck, group: "AI", keywords: "privacy local hybrid cloud provider claude code codex model reasoning share context screenshots files folders tools confirm approval sign in login install", render: PrivacySection },
  { id: "local-ai", label: "Relay Local AI", icon: Cpu, group: "AI", keywords: "local model gpu download setup offline", render: LocalAiSection },
  { id: "connections", label: "Connections", icon: Plug, group: "AI", keywords: "mcp servers tools plugins json", render: ConnectionsSection },
  { id: "archived", label: "Archived chats", icon: Archive, group: "You", keywords: "archive restore unarchive", render: ArchivedSection },
  { id: "data", label: "Data & storage", icon: HardDrive, group: "System", keywords: "folder paths logs audit data", render: DataSection },
  { id: "keyboard", label: "Keyboard", icon: Keyboard, group: "System", keywords: "shortcuts keys hotkey", render: KeyboardSection },
  { id: "about", label: "About", icon: CircleHelp, group: "System", keywords: "version help", render: AboutSection },
];

const GROUPS = ["App", "AI", "You", "System"];

export default function SettingsPage() {
  const { section: sectionId } = useParams();
  const narrow = useNarrow();
  const [q, setQ] = useState("");
  const section = SECTIONS.find((s) => s.id === sectionId) ?? SECTIONS[0]!;
  const [navOpen, setNavOpen] = useState(false);
  const navRef = useRef<HTMLDivElement>(null);
  const onNavKeyDown = useRovingList(navRef);
  const mainRef = useRef<HTMLElement>(null);

  const close = () => go(useApp.getState().lastAppPath || "/");

  const filtered = useMemo(() => {
    const t = q.trim().toLowerCase();
    if (!t) return SECTIONS;
    return SECTIONS.filter((s) => s.label.toLowerCase().includes(t) || s.keywords.includes(t));
  }, [q]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented) return;
      if (document.querySelector("[role=dialog],[role=alertdialog],[role=menu],[role=listbox][data-state=open]")) return;
      if ((e.target as HTMLElement | null)?.closest("[data-radix-popper-content-wrapper]")) return;
      close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Moving to a section puts the page at the top and announces it via the heading.
  useEffect(() => {
    mainRef.current?.scrollTo({ top: 0 });
  }, [section.id]);

  const Body = section.render;
  const showNav = !narrow || navOpen;

  return (
    <div className="flex h-full" data-testid="settings-page">
      {showNav && (
        <nav aria-label="Settings sections" className={cn("flex h-full shrink-0 flex-col bg-sidebar", narrow ? "w-full" : "w-[260px]")}>
          <div data-tauri-drag-region className="flex h-12 shrink-0 items-center pr-2 pl-[calc(var(--relay-titlebar-inset)+8px)]">
            <button type="button" onClick={close} className="no-drag flex h-8 items-center gap-2 rounded-md px-2 text-ui text-fg-2 hover:bg-hover hover:text-fg">
              <ArrowLeft className="size-4" aria-hidden="true" /> Back to Relay
            </button>
          </div>
          <div className="px-3 pb-2">
            <div className="relative">
              <Search className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-fg-muted" aria-hidden="true" />
              <input
                value={q}
                onChange={(e) => setQ(e.target.value)}
                placeholder="Search settings"
                aria-label="Search settings"
                className="h-8 w-full rounded-md border border-line-strong bg-surface pr-2 pl-8 text-ui outline-none placeholder:text-fg-muted focus-visible:border-brand"
                onKeyDown={(e) => {
                  if (e.key === "Enter" && filtered[0]) {
                    go(paths.settings(filtered[0].id));
                    setNavOpen(false);
                  }
                }}
              />
            </div>
          </div>
          <div ref={navRef} onKeyDown={onNavKeyDown} className="min-h-0 flex-1 overflow-y-auto px-2 pb-4 scroll-thin">
            {GROUPS.map((g) => {
              const items = filtered.filter((s) => s.group === g);
              if (!items.length) return null;
              return (
                <div key={g} className="mb-3">
                  <h2 className="px-2 pt-2 pb-1 text-micro font-medium tracking-[0.02em] text-fg-muted">{g}</h2>
                  {items.map((s) => {
                    const active = s.id === section.id && !narrow;
                    return (
                      <button
                        key={s.id}
                        type="button"
                        data-roving
                        data-roving-active={active ? "true" : undefined}
                        aria-current={active ? "page" : undefined}
                        onClick={() => {
                          go(paths.settings(s.id), { replace: true });
                          setNavOpen(false);
                        }}
                        className={cn(
                          "flex h-8 w-full items-center gap-2.5 rounded-md px-2 text-left text-ui text-fg transition-colors duration-(--relay-dur-fast) hover:bg-hover",
                          active && "bg-active hover:bg-active",
                        )}
                      >
                        <s.icon className="size-4 text-fg-2" />
                        {s.label}
                      </button>
                    );
                  })}
                </div>
              );
            })}
            {filtered.length === 0 && <p className="px-2 text-caption text-fg-muted">No settings match “{q}”.</p>}
          </div>
        </nav>
      )}
      {(!narrow || !navOpen) && (
        <main ref={mainRef} className="min-w-0 flex-1 overflow-y-auto scroll-thin" aria-label={section.label}>
          <div data-tauri-drag-region className={cn("sticky top-0 z-(--relay-z-sticky) flex h-12 items-center gap-1 pr-[max(8px,var(--relay-controls-w))] pl-2", narrow ? "bg-canvas" : "")}>
            {narrow && (
              <>
                <button type="button" onClick={() => setNavOpen(true)} className="no-drag flex h-8 items-center gap-1 rounded-md px-2 text-ui text-fg-2 hover:bg-hover hover:text-fg">
                  <ChevronLeft className="size-4" aria-hidden="true" /> Settings
                </button>
                <button type="button" onClick={close} className="no-drag ml-auto flex h-8 items-center rounded-md px-3 text-ui text-fg-2 hover:bg-hover hover:text-fg">
                  Done
                </button>
              </>
            )}
          </div>
          <div className="mx-auto w-full max-w-[760px] px-5 pt-2 pb-20 sm:px-10">
            <Body />
          </div>
        </main>
      )}
    </div>
  );
}
