// App root: providers, routes, the shell (sidebar + main column) and global overlays.
import { lazy, Suspense, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { QueryClientProvider } from "@tanstack/react-query";
import { HashRouter, Navigate, Outlet, Route, Routes, useLocation, useNavigate, useParams } from "react-router";
import { queryClient } from "@/lib/query";
import { registerNavigate, setCurrentPath } from "@/lib/nav";
import { useApp } from "@/store/app";
import { usePlatform, useHybrid, useStatus, useSettings, useConversations, useProjects } from "@/hooks/queries";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Sidebar } from "@/components/shell/Sidebar";
import { WindowControls } from "@/components/shell/WindowControls";
import { Header, useChatTitle } from "@/components/shell/Header";
import { ConfirmDialog } from "@/components/shell/Dialogs";
import { LiveRegion, Toasts } from "@/components/shell/Toasts";
import { PermissionDialog } from "@/components/task/PermissionDialog";
import { Home } from "@/components/chat/Home";
import { ChatView } from "@/components/chat/ChatView";
import { Skeleton } from "@/components/ui/skeleton";
import { useGlobalShortcuts } from "./shortcuts";

const SettingsPage = lazy(() => import("@/settings/SettingsPage"));
const TasksPage = lazy(() => import("@/pages/TasksPage"));
const Gallery = import.meta.env.DEV ? lazy(() => import("@/pages/Gallery")) : null;
const InstallerRoute = lazy(() => import("./InstallerRoute"));
const ProjectPage = lazy(() => import("@/components/projects/ProjectPage").then((m) => ({ default: m.ProjectPage })));
// Overlays that open on demand load on first use (they are tiny to fetch from disk in the app).
const SearchPalette = lazy(() => import("@/components/shell/SearchPalette").then((m) => ({ default: m.SearchPalette })));
const ProjectDialog = lazy(() => import("@/components/shell/Dialogs").then((m) => ({ default: m.ProjectDialog })));

/** Renders `children` once `when` has been true at least once (keeps closing animations working). */
function OnceOpened({ when, children }: { when: boolean; children: ReactNode }) {
  const [ever, setEver] = useState(when);
  useEffect(() => {
    if (when) setEver(true);
  }, [when]);
  return ever ? <Suspense fallback={null}>{children}</Suspense> : null;
}

/** Keeps lib/nav in sync with the router (store actions navigate without hooks). */
function RouterBridge() {
  const navigate = useNavigate();
  const location = useLocation();
  const setLast = useApp((s) => s.setLastAppPath);
  useEffect(() => {
    registerNavigate((to, opts) => navigate(to, opts));
    return () => registerNavigate(null);
  }, [navigate]);
  useLayoutEffect(() => {
    setCurrentPath(location.pathname);
    if (!location.pathname.startsWith("/settings") && !location.pathname.startsWith("/installer")) setLast(location.pathname);
  }, [location.pathname, setLast]);
  return null;
}

/** Starts background data loads in parallel; nothing here blocks the first paint. */
function Bootstrap() {
  const init = useApp((s) => s.init);
  useEffect(() => init(), [init]);
  usePlatform();
  useStatus();
  useSettings();
  useConversations();
  useProjects();
  useHybrid();
  return null;
}

function Shell() {
  const collapsed = useApp((s) => s.sidebarCollapsed);
  const main = useRef<HTMLElement>(null);
  const prev = useRef(collapsed);
  useGlobalShortcuts();

  // FLIP: the sidebar's layout width changes at once; the main column then slides into place with a
  // transform (no per-frame layout), matching the sidebar's own transform animation.
  useLayoutEffect(() => {
    const el = main.current;
    if (!el || prev.current === collapsed) return;
    prev.current = collapsed;
    if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) return;
    const delta = collapsed ? 224 : -224;
    el.style.transition = "none";
    el.style.transform = `translateX(${delta}px)`;
    void el.offsetWidth;
    el.style.transition = "transform var(--relay-dur-panel) var(--relay-ease)";
    el.style.transform = "";
    const done = () => {
      el.style.transition = "";
    };
    el.addEventListener("transitionend", done, { once: true });
  }, [collapsed]);

  return (
    <div className="flex h-full overflow-hidden">
      <Sidebar />
      <main ref={main} className="relative flex min-w-0 flex-1 flex-col bg-canvas" id="main" aria-label="Relay">
        <Outlet />
      </main>
    </div>
  );
}

function HomeRoute() {
  return (
    <>
      <Header />
      <Home />
    </>
  );
}

function ChatRoute() {
  const { id = "" } = useParams();
  const title = useChatTitle(id);
  return (
    <>
      <Header title={title} chatId={id} />
      <ChatView key={id} convId={id} />
    </>
  );
}

function ProjectRoute() {
  const { id = "" } = useParams();
  return (
    <>
      <Header />
      <Suspense fallback={<PageFallback />}>
        <ProjectPage key={id} projectId={id} />
      </Suspense>
    </>
  );
}

function PageFallback() {
  return (
    <div className="mx-auto w-full max-w-(--relay-content-max) space-y-4 px-6 pt-16" role="status" aria-label="Loading">
      <Skeleton className="h-8 w-56" />
      <Skeleton className="h-24 w-full rounded-xl" />
      <Skeleton className="h-24 w-full rounded-xl" />
    </div>
  );
}

function Lazy({ children }: { children: ReactNode }) {
  return <Suspense fallback={<PageFallback />}>{children}</Suspense>;
}

function Overlays() {
  return (
    <>
      <OnceOpened when={useApp((s) => s.searchOpen)}>
        <SearchPalette />
      </OnceOpened>
      <OnceOpened when={useApp((s) => s.projectDialog !== null)}>
        <ProjectDialog />
      </OnceOpened>
      <ConfirmDialog />
      <PermissionDialog />
      <Toasts />
      <LiveRegion />
    </>
  );
}

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <HashRouter>
          <RouterBridge />
          <Bootstrap />
          <WindowControls />
          <Routes>
            <Route element={<Shell />}>
              <Route index element={<HomeRoute />} />
              <Route path="chat/:id" element={<ChatRoute />} />
              <Route path="project/:id" element={<ProjectRoute />} />
              <Route
                path="tasks/:id?"
                element={
                  <Lazy>
                    <TasksPage />
                  </Lazy>
                }
              />
            </Route>
            <Route
              path="settings/:section?"
              element={
                <Lazy>
                  <SettingsShortcuts />
                  <SettingsPage />
                </Lazy>
              }
            />
            <Route
              path="installer"
              element={
                <Lazy>
                  <InstallerRoute />
                </Lazy>
              }
            />
            {Gallery && (
              <Route
                path="gallery"
                element={
                  <Lazy>
                    <Gallery />
                  </Lazy>
                }
              />
            )}
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
          <Overlays />
        </HashRouter>
      </TooltipProvider>
    </QueryClientProvider>
  );
}

function SettingsShortcuts() {
  useGlobalShortcuts();
  return null;
}
