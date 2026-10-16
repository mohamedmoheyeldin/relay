import { lazy, Suspense, useEffect } from "react";
import { Tooltip } from "radix-ui";
import { useApp } from "./store/app";
import { useAppearance } from "./store/appearance";
import { Sidebar } from "./components/Sidebar";
import { Header } from "./components/Header";
import { Home } from "./components/Home";
import { ChatView } from "./components/ChatView";
import { ProjectPage } from "./components/ProjectPage";
import { SearchPalette } from "./components/SearchPalette";
import { ApprovalDialog, ConfirmDialog, ProjectDialog, RenameDialog } from "./components/Dialogs";
import { Toasts } from "./components/Toasts";

const Settings = lazy(() => import("./settings/Settings"));

// Dev only: lets browser automation drive the store while checking the UI.
if (import.meta.env.DEV) Object.assign(window, { __relay: useApp, __appearance: useAppearance });

function useShortcuts() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      const s = useApp.getState();
      const k = e.key.toLowerCase();
      if (mod && !e.shiftKey && k === "k") {
        e.preventDefault();
        s.setSearchOpen(!s.searchOpen);
      } else if (mod && e.shiftKey && k === "o") {
        e.preventDefault();
        s.newChat();
      } else if (mod && e.shiftKey && k === "s") {
        e.preventDefault();
        s.toggleSidebar();
      } else if (mod && (e.key === "," || e.code === "Comma")) {
        e.preventDefault();
        if (s.view.kind === "settings") s.closeSettings();
        else s.openSettings();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}

function MainView() {
  const kind = useApp((s) => s.view.kind);
  const id = useApp((s) => (s.view.kind === "chat" || s.view.kind === "project" ? s.view.id : null));
  if (kind === "chat" && id) return <ChatView key={id} convId={id} />;
  if (kind === "project" && id) return <ProjectPage key={id} projectId={id} />;
  return <Home />;
}

function LiveRegion() {
  const live = useApp((s) => s.live);
  return (
    <div className="sr-only" aria-live="polite" role="status">
      {live}
    </div>
  );
}

export function App() {
  const init = useApp((s) => s.init);
  const inSettings = useApp((s) => s.view.kind === "settings");
  useEffect(() => init(), [init]);
  useShortcuts();

  return (
    <Tooltip.Provider delayDuration={500} skipDelayDuration={200}>
      {inSettings ? (
        <Suspense fallback={<div className="h-full bg-bg" />}>
          <Settings />
        </Suspense>
      ) : (
        <div className="flex h-full overflow-hidden bg-bg">
          <Sidebar />
          <main className="relative flex min-w-0 flex-1 flex-col" aria-label="Chat">
            <Header />
            <MainView />
          </main>
        </div>
      )}
      <SearchPalette />
      <ProjectDialog />
      <ConfirmDialog />
      <RenameDialog />
      <ApprovalDialog />
      <Toasts />
      <LiveRegion />
    </Tooltip.Provider>
  );
}
