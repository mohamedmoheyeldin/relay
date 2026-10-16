import { useEffect, useMemo, useState, type ComponentType } from "react";
import {
  Archive,
  ArrowLeft,
  Bot,
  ChevronLeft,
  CircleQuestionMark,
  HardDrive,
  Keyboard,
  Palette,
  Plug,
  Search,
  Settings as Gear,
  SlidersHorizontal,
} from "lucide-react";
import { useApp } from "../store/app";
import { cn } from "../components/ui";
import { useNarrow } from "../components/hooks";
import { AboutSection, AppearanceSection, ArchivedSection, DataSection, GeneralSection, KeyboardSection, PersonalizationSection } from "./sections";
import { EnginesSection } from "./EnginesSection";
import { ConnectionsSection } from "./ConnectionsSection";

type Section = { id: string; label: string; icon: ComponentType<{ size?: number }>; keywords: string; group: string; render: ComponentType };

const SECTIONS: Section[] = [
  { id: "general", label: "General", icon: Gear, group: "Personal", keywords: "name approvals approval confirm stop hotkey emergency automatic setup", render: GeneralSection },
  { id: "appearance", label: "Appearance", icon: Palette, group: "Personal", keywords: "theme dark light mode accent colour color background foreground", render: AppearanceSection },
  { id: "personalization", label: "Personalization", icon: SlidersHorizontal, group: "Personal", keywords: "custom instructions name about you", render: PersonalizationSection },
  { id: "archived", label: "Archived chats", icon: Archive, group: "Personal", keywords: "archive restore unarchive", render: ArchivedSection },
  { id: "keyboard", label: "Keyboard", icon: Keyboard, group: "Personal", keywords: "shortcuts keys hotkey", render: KeyboardSection },
  { id: "connections", label: "Connections", icon: Plug, group: "Integrations", keywords: "mcp servers tools plugins json", render: ConnectionsSection },
  { id: "engines", label: "AI engines", icon: Bot, group: "Integrations", keywords: "local ai claude model routing gpu download sign in login", render: EnginesSection },
  { id: "data", label: "Data and storage", icon: HardDrive, group: "System", keywords: "folder paths logs audit data", render: DataSection },
  { id: "about", label: "About", icon: CircleQuestionMark, group: "System", keywords: "version help", render: AboutSection },
];

export default function Settings() {
  const sectionId = useApp((s) => (s.view.kind === "settings" ? s.view.section : "general"));
  const { closeSettings, openSettings } = useApp.getState();
  const narrow = useNarrow();
  const [q, setQ] = useState("");
  const [navOpen, setNavOpen] = useState(false);
  const section = SECTIONS.find((s) => s.id === sectionId) ?? SECTIONS[0]!;

  const filtered = useMemo(() => {
    const t = q.trim().toLowerCase();
    if (!t) return SECTIONS;
    return SECTIONS.filter((s) => s.label.toLowerCase().includes(t) || s.keywords.includes(t));
  }, [q]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented) return;
      if (document.querySelector("[role=dialog],[role=alertdialog],[role=menu]")) return;
      closeSettings();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [closeSettings]);

  const groups = ["Personal", "Integrations", "System"];
  const Body = section.render;
  const showNav = !narrow || navOpen;

  return (
    <div className="flex h-full bg-bg">
      {showNav && (
        <nav
          aria-label="Settings"
          className={cn("scroll-thin flex h-full shrink-0 flex-col overflow-y-auto bg-sidebar px-2 pb-4", narrow ? "w-full" : "w-[var(--sidebar-w)] border-r border-line")}
        >
          <div className="h-3 shrink-0" />
          <button type="button" onClick={closeSettings} className="flex h-9 items-center gap-2.5 rounded-lg px-2.5 text-[14px] hover:bg-hover">
            <ArrowLeft size={17} /> Back to app
          </button>
          <div className="relative my-2.5">
            <Search size={15} className="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-subtle" aria-hidden="true" />
            <input
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder="Search"
              aria-label="Search settings"
              className="h-9 w-full rounded-full bg-hover pr-3 pl-9 text-[14px] outline-none focus-visible:ring-2 focus-visible:ring-accent"
            />
          </div>
          {groups.map((g) => {
            const items = filtered.filter((s) => s.group === g);
            if (!items.length) return null;
            return (
              <div key={g} className="mb-3">
                <h2 className="px-2.5 pt-2 pb-1.5 text-[13px] font-medium text-subtle">{g}</h2>
                {items.map((s) => (
                  <button
                    key={s.id}
                    type="button"
                    aria-current={s.id === section.id ? "page" : undefined}
                    onClick={() => {
                      openSettings(s.id);
                      setNavOpen(false);
                    }}
                    className={cn(
                      "flex h-9 w-full items-center gap-2.5 rounded-lg px-2.5 text-left text-[14px] transition-colors duration-100 hover:bg-hover",
                      s.id === section.id && !narrow && "bg-active hover:bg-active",
                    )}
                  >
                    <s.icon size={17} />
                    {s.label}
                  </button>
                ))}
              </div>
            );
          })}
          {filtered.length === 0 && <p className="px-2.5 text-[13px] text-subtle">No settings match "{q}".</p>}
        </nav>
      )}
      {(!narrow || !navOpen) && (
        <main className="scroll-thin min-w-0 flex-1 overflow-y-auto" aria-label={section.label}>
          {narrow && (
            <div className="sticky top-0 z-10 flex h-12 items-center gap-1 bg-bg px-2">
              <button type="button" onClick={() => setNavOpen(true)} className="flex h-9 items-center gap-1 rounded-lg px-2 text-[14px] text-muted hover:bg-hover hover:text-fg">
                <ChevronLeft size={18} /> Settings
              </button>
              <button type="button" onClick={closeSettings} className="ml-auto flex h-9 items-center rounded-lg px-3 text-[14px] text-muted hover:bg-hover hover:text-fg">
                Done
              </button>
            </div>
          )}
          <div className="mx-auto w-full max-w-[768px] px-5 pt-8 pb-16 sm:px-8 sm:pt-10">
            <Body />
          </div>
        </main>
      )}
    </div>
  );
}
