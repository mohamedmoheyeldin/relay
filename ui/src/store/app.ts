// Global app state (zustand). Components subscribe with narrow selectors so a streaming
// delta only re-renders the message being streamed.
import { create } from "zustand";
import { api, listenEvents } from "../lib/backend";
import {
  addUser,
  applyEvent,
  dropLastTurn,
  emptyChat,
  lastUser,
  normalizeLoaded,
  searchTextOf,
  titleFrom,
  type ChatEvent,
  type ChatState,
} from "../lib/chat";
import type {
  ApprovalMode,
  ConvSummary,
  Engines,
  Progress,
  Project,
  Risk,
  RouteMode,
  Settings,
  StatusPayload,
  UiEvent,
} from "../lib/types";

export type View =
  | { kind: "home"; projectId?: null }
  | { kind: "chat"; id: string }
  | { kind: "project"; id: string }
  | { kind: "settings"; section: string };

export type Chat = ChatState & { loaded: boolean; error: string | null };
export type ConfirmReq = { id: number; run_id: number; title: string; detail: string; risk: Risk; at: number };
export type Toast = { id: number; text: string; tone: "info" | "error" };
export type ProjectDialogState = { mode: "create" } | { mode: "edit"; project: Project } | null;
export type ConfirmDialogState = {
  title: string;
  body: string;
  action: string;
  destructive?: boolean;
  onConfirm: () => void;
} | null;

const LS_SIDEBAR = "relay.sidebarOpen";
const LS_PINNED_PROJECTS = "relay.pinnedProjects";

function lsGet(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
function lsSet(key: string, v: string) {
  try {
    localStorage.setItem(key, v);
  } catch {
    /* storage unavailable */
  }
}

const narrow = () => typeof window !== "undefined" && window.innerWidth < 768;

type State = {
  view: View;
  lastAppView: View;
  convs: ConvSummary[];
  convsLoaded: boolean;
  archived: ConvSummary[] | null;
  projects: Project[];
  projectsLoaded: boolean;
  pinnedProjects: string[];
  chats: Record<string, Chat>;
  runConv: Record<number, string>;
  activeRun: { run_id: number; conv_id: string } | null;
  sending: string | null; // conv id while send_message is in flight
  status: StatusPayload | null;
  engines: Engines | null;
  enginesError: string | null;
  settings: Settings | null;
  settingsError: string | null;
  progress: Record<string, Progress>;
  confirms: ConfirmReq[];
  sidebarOpen: boolean;
  searchOpen: boolean;
  projectDialog: ProjectDialogState;
  confirmDialog: ConfirmDialogState;
  renameDialog: { id: string; title: string } | null;
  toasts: Toast[];
  composerFill: { text: string; nonce: number } | null;
  live: string; // screen-reader announcement
};

type Actions = {
  init: () => void;
  go: (v: View) => void;
  newChat: (projectId?: string | null) => void;
  openChat: (id: string) => Promise<void>;
  openSettings: (section?: string) => void;
  closeSettings: () => void;
  send: (text: string, attachments: string[], projectId?: string | null) => Promise<void>;
  retry: (convId: string) => Promise<void>;
  stop: () => void;
  answerConfirm: (id: number, approved: boolean) => void;
  renameConv: (id: string, title: string) => void;
  setPinned: (id: string, pinned: boolean) => void;
  setArchived: (id: string, archived: boolean) => void;
  moveToProject: (id: string, projectId: string | null) => void;
  deleteConv: (id: string) => void;
  askDeleteConv: (id: string) => void;
  loadArchived: () => Promise<void>;
  createProject: (name: string, color: string, instructions: string) => Promise<Project | null>;
  updateProject: (id: string, patch: { name?: string; color?: string; instructions?: string }) => Promise<void>;
  deleteProject: (id: string) => void;
  togglePinProject: (id: string) => void;
  loadEngines: () => Promise<void>;
  setRouteMode: (m: RouteMode) => void;
  setLocalModel: (m: string) => void;
  loadSettings: () => Promise<void>;
  updateSettings: (p: Partial<{ approval_mode: ApprovalMode; user_name: string; custom_instructions: string; auto_setup: boolean }>) => Promise<void>;
  setSidebar: (open: boolean) => void;
  toggleSidebar: () => void;
  setSearchOpen: (o: boolean) => void;
  setProjectDialog: (d: ProjectDialogState) => void;
  setConfirmDialog: (d: ConfirmDialogState) => void;
  setRenameDialog: (d: { id: string; title: string } | null) => void;
  toast: (text: string, tone?: Toast["tone"]) => void;
  dismissToast: (id: number) => void;
  fillComposer: (text: string) => void;
};

export type AppState = State & Actions;

let toastSeq = 0;
let tempSeq = 0;
let initialized = false;
const saveTimers = new Map<string, ReturnType<typeof setTimeout>>();

const msg = (e: unknown) => (e instanceof Error ? e.message : String(e));

export const useApp = create<AppState>()((set, get) => {
  // ---- persistence of chats ------------------------------------------------------------
  const saveNow = (id: string) => {
    const t = saveTimers.get(id);
    if (t) clearTimeout(t);
    saveTimers.delete(id);
    if (id.startsWith("tmp-")) return;
    const chat = get().chats[id];
    if (!chat || !chat.loaded) return;
    const title = get().convs.find((c) => c.id === id)?.title;
    api.convSave(id, chat.items, searchTextOf(chat.items), title || undefined).catch((e) => {
      console.warn("conv_save failed", e);
    });
  };
  const saveSoon = (id: string, ms = 1200) => {
    if (saveTimers.has(id)) return;
    saveTimers.set(id, setTimeout(() => saveNow(id), ms));
  };

  const patchChat = (id: string, f: (c: Chat) => Chat) =>
    set((s) => {
      const cur = s.chats[id] ?? { ...emptyChat(), loaded: true, error: null };
      return { chats: { ...s.chats, [id]: f(cur) } };
    });

  const patchConv = (id: string, p: Partial<ConvSummary>) =>
    set((s) => ({
      convs: s.convs.map((c) => (c.id === id ? { ...c, ...p } : c)),
      archived: s.archived?.map((c) => (c.id === id ? { ...c, ...p } : c)) ?? null,
    }));

  const touchConv = (id: string) =>
    set((s) => {
      const c = s.convs.find((x) => x.id === id);
      if (!c) return {};
      const updated = { ...c, updated_at: Date.now() };
      return { convs: [updated, ...s.convs.filter((x) => x.id !== id)] };
    });

  // ---- event pipeline: queue, then apply once per animation frame ----------------------------
  let queue: UiEvent[] = [];
  let scheduled = false;
  const schedule = () => {
    if (scheduled) return;
    scheduled = true;
    let done = false;
    const run = () => {
      if (done) return;
      done = true;
      scheduled = false;
      flush();
    };
    requestAnimationFrame(run);
    setTimeout(run, 120); // rAF is paused while the window is hidden in the tray
  };

  const routeConv = (s: State, runId: number | null): string | null => {
    if (runId != null && s.runConv[runId]) return s.runConv[runId]!;
    if (s.activeRun) return s.activeRun.conv_id;
    if (s.sending) return s.sending;
    return null;
  };

  const flush = () => {
    const events = queue;
    queue = [];
    if (!events.length) return;
    const now = Date.now();
    const touched = new Set<string>();
    const finished = new Set<string>();
    let refreshEngines = false;
    let announce: string | null = null;
    const errorsToToast: string[] = [];

    set((s0) => {
      let s: State = s0;
      const chats = { ...s.chats };
      const apply = (convId: string, ev: ChatEvent) => {
        const cur = chats[convId] ?? { ...emptyChat(), loaded: true, error: null };
        chats[convId] = { ...cur, ...applyEvent(cur, ev, now) };
        touched.add(convId);
      };
      for (const ev of events) {
        switch (ev.type) {
          case "status": {
            const { type: _t, ...st } = ev;
            s = { ...s, status: st };
            break;
          }
          case "progress": {
            const { type: _t, ...p } = ev;
            s = { ...s, progress: { ...s.progress, [p.id]: p } };
            if (p.done) refreshEngines = true;
            break;
          }
          case "run_started": {
            s = {
              ...s,
              runConv: { ...s.runConv, [ev.run_id]: ev.conv_id },
              activeRun: { run_id: ev.run_id, conv_id: ev.conv_id },
              sending: null,
            };
            apply(ev.conv_id, ev);
            announce = "Relay is working";
            break;
          }
          case "confirm": {
            s = {
              ...s,
              confirms: [
                ...s.confirms.filter((c) => c.id !== ev.id),
                { id: ev.id, run_id: ev.run_id, title: ev.title, detail: ev.detail, risk: ev.risk, at: now },
              ],
            };
            const conv = routeConv(s, ev.run_id);
            if (conv) apply(conv, ev);
            break;
          }
          case "confirm_resolved": {
            s = { ...s, confirms: s.confirms.filter((c) => c.id !== ev.id) };
            for (const [id, c] of Object.entries(chats)) {
              if (c.items.some((it) => it.kind === "tools" && it.steps.some((x) => x.kind === "confirm" && x.confirm_id === ev.id)))
                apply(id, ev);
            }
            break;
          }
          case "notice": {
            const conv = routeConv(s, null) ?? (s.view.kind === "chat" ? s.view.id : null);
            if (conv) apply(conv, ev);
            else errorsToToast.push(ev.text);
            break;
          }
          case "error": {
            const conv = routeConv(s, ev.run_id) ?? (ev.run_id == null && s.view.kind === "chat" ? s.view.id : null);
            if (conv) apply(conv, ev);
            else errorsToToast.push(ev.message);
            if (s.sending && !s.activeRun) s = { ...s, sending: null };
            break;
          }
          case "run_finished": {
            const conv = routeConv(s, ev.run_id);
            if (conv) {
              apply(conv, ev);
              finished.add(conv);
              const last = chats[conv]!.items.at(-1);
              announce = last && last.kind === "assistant" ? `Relay: ${last.text.slice(0, 400)}` : "Relay finished";
            }
            s = { ...s, activeRun: s.activeRun?.run_id === ev.run_id ? null : s.activeRun, sending: null, confirms: s.confirms.filter((c) => c.run_id !== ev.run_id) };
            break;
          }
          default: {
            const conv = routeConv(s, ev.run_id);
            if (conv) apply(conv, ev);
          }
        }
      }
      return { ...s, chats, ...(announce ? { live: announce } : {}) };
    });

    for (const id of touched) if (!finished.has(id)) saveSoon(id);
    for (const id of finished) {
      saveNow(id);
      touchConv(id);
    }
    for (const t of errorsToToast) get().toast(t, "error");
    if (refreshEngines) void get().loadEngines();
  };

  const startListening = () => {
    listenEvents((ev) => {
      queue.push(ev);
      schedule();
    }).catch((e) => console.error("listen failed", e));
  };

  // ---- initial state ---------------------------------------------------------------------
  const storedSidebar = lsGet(LS_SIDEBAR);
  let pinnedProjects: string[] = [];
  try {
    pinnedProjects = JSON.parse(lsGet(LS_PINNED_PROJECTS) ?? "[]") as string[];
  } catch {
    pinnedProjects = [];
  }

  return {
    view: { kind: "home" },
    lastAppView: { kind: "home" },
    convs: [],
    convsLoaded: false,
    archived: null,
    projects: [],
    projectsLoaded: false,
    pinnedProjects,
    chats: {},
    runConv: {},
    activeRun: null,
    sending: null,
    status: null,
    engines: null,
    enginesError: null,
    settings: null,
    settingsError: null,
    progress: {},
    confirms: [],
    sidebarOpen: narrow() ? false : storedSidebar !== "false",
    searchOpen: false,
    projectDialog: null,
    confirmDialog: null,
    renameDialog: null,
    toasts: [],
    composerFill: null,
    live: "",

    init: () => {
      if (initialized) return;
      initialized = true;
      startListening();
      // Fire everything in parallel; nothing blocks the first paint.
      api.convList().then((convs) => set({ convs, convsLoaded: true })).catch((e) => {
        set({ convsLoaded: true });
        get().toast(`Could not load chats: ${msg(e)}`, "error");
      });
      api.projectList().then((projects) => set({ projects, projectsLoaded: true })).catch(() => set({ projectsLoaded: true }));
      api.getStatus().then((status) => set({ status })).catch(() => undefined);
      void get().loadSettings();
      void get().loadEngines();
    },

    go: (v) => {
      set({ view: v, ...(v.kind !== "settings" ? { lastAppView: v } : {}) });
      if (narrow()) set({ sidebarOpen: false });
    },

    newChat: (projectId) => {
      get().go(projectId ? { kind: "project", id: projectId } : { kind: "home" });
    },

    openChat: async (id) => {
      get().go({ kind: "chat", id });
      const existing = get().chats[id];
      if (existing && (existing.loaded || existing.items.length)) return;
      set((s) => ({ chats: { ...s.chats, [id]: { ...emptyChat(), loaded: false, error: null } } }));
      try {
        const conv = await api.convGet(id);
        const items = normalizeLoaded(Array.isArray(conv.items) ? conv.items : []);
        set((s) => {
          const cur = s.chats[id];
          // A run may have started meanwhile; keep live items if so.
          if (cur && cur.items.length) return {};
          return { chats: { ...s.chats, [id]: { items, run: null, seq: items.length, loaded: true, error: null } } };
        });
      } catch (e) {
        set((s) => ({ chats: { ...s.chats, [id]: { ...emptyChat(), loaded: true, error: msg(e) } } }));
      }
    },

    openSettings: (section = "general") => {
      const s = get();
      set({ view: { kind: "settings", section }, lastAppView: s.view.kind === "settings" ? s.lastAppView : s.view });
    },
    closeSettings: () => set((s) => ({ view: s.lastAppView })),

    send: async (text, attachments, projectId) => {
      const s = get();
      const body = text.trim();
      if (!body) return;
      if (s.activeRun || s.sending || s.status?.busy) {
        get().toast("Relay is still working on the last task. Stop it first or wait for it to finish.");
        return;
      }
      let id = s.view.kind === "chat" ? s.view.id : null;
      const now = Date.now();
      if (!id) {
        // Show the message at once; create the conversation in the background.
        const tmp = `tmp-${++tempSeq}`;
        set((st) => ({
          view: { kind: "chat", id: tmp },
          lastAppView: { kind: "chat", id: tmp },
          sending: tmp,
          chats: { ...st.chats, [tmp]: { ...addUser(emptyChat(), body, attachments, now), loaded: true, error: null } },
        }));
        try {
          const conv = await api.convCreate(projectId ?? null);
          const title = titleFrom(body);
          set((st) => {
            const chats = { ...st.chats };
            chats[conv.id] = chats[tmp]!;
            delete chats[tmp];
            const view: View = st.view.kind === "chat" && st.view.id === tmp ? { kind: "chat", id: conv.id } : st.view;
            return {
              chats,
              view,
              lastAppView: st.lastAppView.kind === "chat" && st.lastAppView.id === tmp ? { kind: "chat", id: conv.id } : st.lastAppView,
              sending: conv.id,
              convs: [{ ...conv, title }, ...st.convs.filter((c) => c.id !== conv.id)],
            };
          });
          id = conv.id;
        } catch (e) {
          set((st) => ({ sending: null, chats: { ...st.chats, [tmp]: { ...st.chats[tmp]!, error: `Could not create the chat: ${msg(e)}` } } }));
          return;
        }
      } else {
        const convId = id;
        set({ sending: convId });
        patchChat(convId, (c) => ({ ...c, ...addUser(c, body, attachments, now) }));
        const conv = get().convs.find((c) => c.id === convId);
        if (conv && !conv.title) patchConv(convId, { title: titleFrom(body) });
        touchConv(convId);
      }
      const convId = id;
      saveNow(convId);
      try {
        const { run_id } = await api.sendMessage(convId, body, attachments);
        set((st) => ({ runConv: { ...st.runConv, [run_id]: convId } }));
      } catch (e) {
        set({ sending: null });
        patchChat(convId, (c) => ({
          ...c,
          ...applyEvent(c, { type: "error", run_id: null, kind: "internal", message: msg(e) }, Date.now()),
        }));
        saveSoon(convId);
      }
    },

    retry: async (convId) => {
      const chat = get().chats[convId];
      if (!chat) return;
      const u = lastUser(chat.items);
      if (!u) return;
      if (get().activeRun || get().sending) return;
      patchChat(convId, (c) => ({ ...c, items: dropLastTurn(c.items) }));
      if (get().view.kind !== "chat" || (get().view as { id: string }).id !== convId) get().go({ kind: "chat", id: convId });
      await get().send(u.text, u.attachments);
    },

    stop: () => {
      api.stopTask().catch((e) => get().toast(`Could not stop: ${msg(e)}`, "error"));
    },

    answerConfirm: (id, approved) => {
      set((s) => ({ confirms: s.confirms.filter((c) => c.id !== id) }));
      api.answerConfirm(id, approved).catch((e) => get().toast(`Could not send your answer: ${msg(e)}`, "error"));
    },

    renameConv: (id, title) => {
      const t = title.trim();
      if (!t) return;
      const prev = get().convs.find((c) => c.id === id)?.title;
      patchConv(id, { title: t });
      api.convUpdate(id, { title: t }).catch((e) => {
        if (prev !== undefined) patchConv(id, { title: prev });
        get().toast(`Rename failed: ${msg(e)}`, "error");
      });
    },

    setPinned: (id, pinned) => {
      patchConv(id, { pinned });
      api.convUpdate(id, { pinned }).catch((e) => {
        patchConv(id, { pinned: !pinned });
        get().toast(`Could not ${pinned ? "pin" : "unpin"}: ${msg(e)}`, "error");
      });
    },

    setArchived: (id, archived) => {
      const conv = get().convs.find((c) => c.id === id) ?? get().archived?.find((c) => c.id === id);
      if (!conv) return;
      set((s) => ({
        convs: archived ? s.convs.filter((c) => c.id !== id) : [{ ...conv, archived }, ...s.convs.filter((c) => c.id !== id)],
        archived: s.archived
          ? archived
            ? [{ ...conv, archived }, ...s.archived]
            : s.archived.filter((c) => c.id !== id)
          : null,
      }));
      const v = get().view;
      if (archived && v.kind === "chat" && v.id === id) get().go({ kind: "home" });
      api.convUpdate(id, { archived }).then(
        () => archived && get().toast("Chat archived"),
        (e) => get().toast(`Archive failed: ${msg(e)}`, "error"),
      );
    },

    moveToProject: (id, projectId) => {
      const prev = get().convs.find((c) => c.id === id)?.project_id ?? null;
      patchConv(id, { project_id: projectId });
      api.convMove(id, projectId).then(
        () => {
          const p = get().projects.find((x) => x.id === projectId);
          get().toast(p ? `Moved to ${p.name}` : "Removed from project");
        },
        (e) => {
          patchConv(id, { project_id: prev });
          get().toast(`Move failed: ${msg(e)}`, "error");
        },
      );
    },

    deleteConv: (id) => {
      set((s) => {
        const chats = { ...s.chats };
        delete chats[id];
        return {
          convs: s.convs.filter((c) => c.id !== id),
          archived: s.archived?.filter((c) => c.id !== id) ?? null,
          chats,
        };
      });
      const v = get().view;
      if (v.kind === "chat" && v.id === id) get().go({ kind: "home" });
      api.convDelete(id).catch((e) => get().toast(`Delete failed: ${msg(e)}`, "error"));
    },

    askDeleteConv: (id) => {
      const c = get().convs.find((x) => x.id === id) ?? get().archived?.find((x) => x.id === id);
      set({
        confirmDialog: {
          title: "Delete chat?",
          body: `This will delete "${c?.title || "New chat"}". This can't be undone.`,
          action: "Delete",
          destructive: true,
          onConfirm: () => get().deleteConv(id),
        },
      });
    },

    loadArchived: async () => {
      try {
        set({ archived: await api.convList(true) });
      } catch (e) {
        get().toast(`Could not load archived chats: ${msg(e)}`, "error");
        set({ archived: [] });
      }
    },

    createProject: async (name, color, instructions) => {
      try {
        const p = await api.projectCreate(name, color, instructions);
        set((s) => ({ projects: [p, ...s.projects] }));
        return p;
      } catch (e) {
        get().toast(`Could not create the project: ${msg(e)}`, "error");
        return null;
      }
    },

    updateProject: async (id, patch) => {
      try {
        const p = await api.projectUpdate(id, patch);
        set((s) => ({ projects: s.projects.map((x) => (x.id === id ? p : x)) }));
      } catch (e) {
        get().toast(`Could not save the project: ${msg(e)}`, "error");
      }
    },

    deleteProject: (id) => {
      set((s) => ({
        projects: s.projects.filter((p) => p.id !== id),
        convs: s.convs.map((c) => (c.project_id === id ? { ...c, project_id: null } : c)),
        pinnedProjects: s.pinnedProjects.filter((x) => x !== id),
      }));
      const v = get().view;
      if (v.kind === "project" && v.id === id) get().go({ kind: "home" });
      api.projectDelete(id).catch((e) => get().toast(`Delete failed: ${msg(e)}`, "error"));
    },

    togglePinProject: (id) => {
      set((s) => {
        const pinnedProjects = s.pinnedProjects.includes(id) ? s.pinnedProjects.filter((x) => x !== id) : [...s.pinnedProjects, id];
        lsSet(LS_PINNED_PROJECTS, JSON.stringify(pinnedProjects));
        return { pinnedProjects };
      });
    },

    loadEngines: async () => {
      try {
        const engines = await api.getEngines();
        set({ engines, enginesError: null });
      } catch (e) {
        set({ enginesError: msg(e) });
      }
    },

    setRouteMode: (m) => {
      const prev = get().engines?.route_mode ?? get().status?.route_mode;
      set((s) => ({
        engines: s.engines ? { ...s.engines, route_mode: m } : s.engines,
        status: s.status ? { ...s.status, route_mode: m } : s.status,
      }));
      api.setRouteMode(m).catch((e) => {
        if (prev)
          set((s) => ({
            engines: s.engines ? { ...s.engines, route_mode: prev } : s.engines,
            status: s.status ? { ...s.status, route_mode: prev } : s.status,
          }));
        get().toast(`Could not change the engine: ${msg(e)}`, "error");
      });
    },

    setLocalModel: (m) => {
      set((s) => ({ engines: s.engines ? { ...s.engines, local: { ...s.engines.local, selected: m || null } } : s.engines }));
      api.setLocalModel(m).then(
        () => void get().loadEngines(),
        (e) => get().toast(`Could not select the model: ${msg(e)}`, "error"),
      );
    },

    loadSettings: async () => {
      try {
        set({ settings: await api.getSettings(), settingsError: null });
      } catch (e) {
        set({ settingsError: msg(e) });
      }
    },

    updateSettings: async (p) => {
      const prev = get().settings;
      if (prev) set({ settings: { ...prev, ...p } });
      try {
        set({ settings: await api.setSettings(p) });
      } catch (e) {
        if (prev) set({ settings: prev });
        get().toast(`Could not save settings: ${msg(e)}`, "error");
      }
    },

    setSidebar: (open) => {
      set({ sidebarOpen: open });
      if (!narrow()) lsSet(LS_SIDEBAR, String(open));
    },
    toggleSidebar: () => get().setSidebar(!get().sidebarOpen),
    setSearchOpen: (o) => set({ searchOpen: o }),
    setProjectDialog: (d) => set({ projectDialog: d }),
    setConfirmDialog: (d) => set({ confirmDialog: d }),
    setRenameDialog: (d) => set({ renameDialog: d }),

    toast: (text, tone = "info") => {
      const id = ++toastSeq;
      set((s) => ({ toasts: [...s.toasts.slice(-2), { id, text, tone }] }));
      setTimeout(() => get().dismissToast(id), tone === "error" ? 6000 : 3500);
    },
    dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
    fillComposer: (text) => set({ composerFill: { text, nonce: Date.now() } }),
  };
});

/** True while any task is running or being started. */
export const selectBusy = (s: AppState) => Boolean(s.activeRun || s.sending || s.status?.busy);

export const selectActiveChatId = (s: AppState) => (s.view.kind === "chat" ? s.view.id : null);
