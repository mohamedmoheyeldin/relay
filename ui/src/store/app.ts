// Client state (zustand): live conversations driven by backend events, the approval queue,
// progress, toasts and UI chrome (sidebar, palette, dialogs). Server data (lists, settings, hybrid,
// workflows, engines) lives in TanStack Query; events patch or invalidate it from `flush`.
// Components subscribe with narrow selectors so a streaming delta only re-renders the message
// being streamed.
import { create } from "zustand";
import { api, errorText, listenEvents, onContractViolation } from "../lib/backend";
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
import { queryClient, qk } from "../lib/query";
import { chatIdOf, getCurrentPath, getRequestedPath, go, paths } from "../lib/nav";
import type { ConvSummary, Progress, Project, Risk, StatusPayload, UiEvent } from "../lib/types";

export type Chat = ChatState & { loaded: boolean; error: string | null };
export type ConfirmReq = { id: number; run_id: number; title: string; detail: string; risk: Risk; at: number };
export type Toast = { id: number; text: string; tone: "info" | "error"; action?: { label: string; run: () => void } };
export type ProjectDialogState = { mode: "create" } | { mode: "edit"; project: Project } | null;
export type ConfirmDialogState = {
  title: string;
  body: string;
  action: string;
  destructive?: boolean;
  onConfirm: () => void;
} | null;

const LS_SIDEBAR = "relay.sidebarCollapsed";
const LS_PINNED_PROJECTS = "relay.pinnedProjects";
export const CONFIRM_TIMEOUT_S = 120;

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

type State = {
  chats: Record<string, Chat>;
  runConv: Record<number, string>;
  activeRun: { run_id: number; conv_id: string; started_at: number } | null;
  sending: string | null; // conv id while send_message is in flight
  progress: Record<string, Progress>;
  confirms: ConfirmReq[];
  /** Desktop: sidebar collapsed to the icon rail. */
  sidebarCollapsed: boolean;
  /** Narrow windows: sidebar shown as an overlay drawer. */
  drawerOpen: boolean;
  searchOpen: boolean;
  pinnedProjects: string[];
  projectDialog: ProjectDialogState;
  confirmDialog: ConfirmDialogState;
  toasts: Toast[];
  composerFill: { text: string; nonce: number } | null;
  composerFocus: number;
  /** Screen-reader announcement (polite live region). */
  live: string;
  lastAppPath: string;
};

type Actions = {
  init: () => void;
  ensureChat: (id: string, rawItems: unknown[]) => void;
  send: (text: string, attachments: string[], opts?: { projectId?: string | null; convId?: string | null }) => Promise<void>;
  retry: (convId: string) => Promise<void>;
  stop: () => void;
  answerConfirm: (id: number, approved: boolean) => void;
  forgetChat: (id: string) => void;
  togglePinProject: (id: string) => void;
  setSidebarCollapsed: (v: boolean) => void;
  toggleSidebar: (narrow: boolean) => void;
  setDrawerOpen: (v: boolean) => void;
  setSearchOpen: (o: boolean) => void;
  setProjectDialog: (d: ProjectDialogState) => void;
  setConfirmDialog: (d: ConfirmDialogState) => void;
  askDeleteConv: (conv: Pick<ConvSummary, "id" | "title">, onConfirm: () => void) => void;
  toast: (text: string, tone?: Toast["tone"], action?: Toast["action"]) => void;
  dismissToast: (id: number) => void;
  fillComposer: (text: string) => void;
  focusComposer: () => void;
  announce: (text: string) => void;
  setLastAppPath: (p: string) => void;
};

export type AppState = State & Actions;

let toastSeq = 0;
let tempSeq = 0;
let initialized = false;
const saveTimers = new Map<string, ReturnType<typeof setTimeout>>();

// ---- query cache helpers (conversation lists) ------------------------------------------------

export function convsCache(archived = false): ConvSummary[] | undefined {
  return queryClient.getQueryData<ConvSummary[]>(qk.convs(archived));
}
export function findConv(id: string): ConvSummary | undefined {
  return convsCache(false)?.find((c) => c.id === id) ?? convsCache(true)?.find((c) => c.id === id);
}
export function patchConvCache(id: string, patch: Partial<ConvSummary>) {
  for (const archived of [false, true]) {
    queryClient.setQueryData<ConvSummary[]>(qk.convs(archived), (list) => list?.map((c) => (c.id === id ? { ...c, ...patch } : c)));
  }
}
export function upsertConvCache(conv: ConvSummary) {
  queryClient.setQueryData<ConvSummary[]>(qk.convs(conv.archived), (list) => [conv, ...(list ?? []).filter((c) => c.id !== conv.id)]);
  queryClient.setQueryData<ConvSummary[]>(qk.convs(!conv.archived), (list) => list?.filter((c) => c.id !== conv.id));
}
export function removeConvCache(id: string) {
  for (const archived of [false, true]) {
    queryClient.setQueryData<ConvSummary[]>(qk.convs(archived), (list) => list?.filter((c) => c.id !== id));
  }
}
function touchConv(id: string) {
  const c = convsCache(false)?.find((x) => x.id === id);
  if (c) upsertConvCache({ ...c, updated_at: Date.now() });
}

export const useApp = create<AppState>()((set, get) => {
  // ---- persistence of chats ------------------------------------------------------------
  const saveNow = (id: string) => {
    const t = saveTimers.get(id);
    if (t) clearTimeout(t);
    saveTimers.delete(id);
    if (id.startsWith("tmp-")) return;
    const chat = get().chats[id];
    if (!chat || !chat.loaded) return;
    const title = findConv(id)?.title;
    api.convSave(id, chat.items, searchTextOf(chat.items), title || undefined).catch((e) => {
      console.warn("conv_save failed", e);
    });
  };
  const saveSoon = (id: string, ms = 1200) => {
    if (saveTimers.has(id)) return;
    saveTimers.set(
      id,
      setTimeout(() => saveNow(id), ms),
    );
  };

  const patchChat = (id: string, f: (c: Chat) => Chat) =>
    set((s) => {
      const cur = s.chats[id] ?? { ...emptyChat(), loaded: true, error: null };
      return { chats: { ...s.chats, [id]: f(cur) } };
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
    if (typeof requestAnimationFrame === "function") requestAnimationFrame(run);
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
    let status: StatusPayload | null = null;
    let announce: string | null = null;
    const toasts: string[] = [];

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
            status = st;
            break;
          }
          case "progress": {
            const { type: _t, ...p } = ev;
            s = { ...s, progress: { ...s.progress, [p.id]: p } };
            if (p.done && p.id === "local-ai") refreshEngines = true;
            if (p.done && p.id === "local-ai") announce = p.error ? `Relay Local AI setup failed: ${p.error}` : p.label;
            break;
          }
          case "run_started": {
            s = {
              ...s,
              runConv: { ...s.runConv, [ev.run_id]: ev.conv_id },
              activeRun: { run_id: ev.run_id, conv_id: ev.conv_id, started_at: now },
              sending: null,
            };
            apply(ev.conv_id, ev);
            announce = "Relay started the task";
            break;
          }
          case "plan_proposed": {
            const conv = routeConv(s, ev.run_id);
            if (conv) apply(conv, ev);
            announce = `Plan proposed with ${ev.steps.length} ${ev.steps.length === 1 ? "step" : "steps"}`;
            break;
          }
          case "step_finished": {
            const conv = routeConv(s, ev.run_id);
            if (conv) apply(conv, ev);
            if (ev.status === "failed") announce = `A step failed: ${ev.summary}`;
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
            announce = `Relay needs your approval: ${ev.title}`;
            break;
          }
          case "confirm_resolved": {
            s = { ...s, confirms: s.confirms.filter((c) => c.id !== ev.id) };
            for (const [id, c] of Object.entries(chats)) {
              if (
                c.items.some(
                  (it) =>
                    (it.kind === "tools" && it.steps.some((x) => x.kind === "confirm" && x.confirm_id === ev.id)) ||
                    (it.kind === "task" && it.confirms.some((x) => x.confirm_id === ev.id)),
                )
              )
                apply(id, ev);
            }
            break;
          }
          case "notice": {
            const conv = routeConv(s, null) ?? chatIdOf(getCurrentPath());
            if (conv) apply(conv, ev);
            else toasts.push(ev.text);
            break;
          }
          case "error": {
            const conv = routeConv(s, ev.run_id) ?? (ev.run_id == null ? chatIdOf(getCurrentPath()) : null);
            if (conv) apply(conv, ev);
            else toasts.push(ev.message);
            if (s.sending && !s.activeRun) s = { ...s, sending: null };
            break;
          }
          case "run_finished": {
            const conv = routeConv(s, ev.run_id);
            if (conv) {
              apply(conv, ev);
              finished.add(conv);
              const last = chats[conv]!.items.at(-1);
              announce =
                ev.outcome === "cancelled"
                  ? "Task stopped"
                  : last && last.kind === "assistant"
                    ? `Relay: ${last.text.slice(0, 300)}`
                    : "Relay finished the task";
            }
            s = {
              ...s,
              activeRun: s.activeRun?.run_id === ev.run_id ? null : s.activeRun,
              sending: null,
              confirms: s.confirms.filter((c) => c.run_id !== ev.run_id),
            };
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

    if (status) queryClient.setQueryData(qk.status, status);
    for (const id of touched) if (!finished.has(id)) saveSoon(id);
    for (const id of finished) {
      saveNow(id);
      touchConv(id);
    }
    if (finished.size) void queryClient.invalidateQueries({ queryKey: qk.workflowsAll });
    for (const t of toasts) get().toast(t, "error");
    if (refreshEngines) void queryClient.invalidateQueries({ queryKey: qk.engines });
  };

  const startListening = () => {
    listenEvents((ev) => {
      queue.push(ev);
      schedule();
    }).catch((e) => console.error("listen failed", e));
  };

  // ---- initial state ---------------------------------------------------------------------
  let pinnedProjects: string[] = [];
  try {
    const v = JSON.parse(lsGet(LS_PINNED_PROJECTS) ?? "[]") as unknown;
    pinnedProjects = Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
  } catch {
    pinnedProjects = [];
  }

  return {
    chats: {},
    runConv: {},
    activeRun: null,
    sending: null,
    progress: {},
    confirms: [],
    sidebarCollapsed: lsGet(LS_SIDEBAR) === "true",
    drawerOpen: false,
    searchOpen: false,
    pinnedProjects,
    projectDialog: null,
    confirmDialog: null,
    toasts: [],
    composerFill: null,
    composerFocus: 0,
    live: "",
    lastAppPath: "/",

    init: () => {
      if (initialized) return;
      initialized = true;
      startListening();
      if (import.meta.env.DEV) {
        onContractViolation((v) => get().toast(`Contract violation in ${v.where} "${v.name}": ${v.message}`, "error"));
      }
    },

    ensureChat: (id, rawItems) => {
      const existing = get().chats[id];
      if (existing && (existing.loaded || existing.items.length)) return;
      const items = normalizeLoaded(rawItems);
      set((s) => ({ chats: { ...s.chats, [id]: { items, run: null, seq: items.length, loaded: true, error: null } } }));
    },

    send: async (text, attachments, opts = {}) => {
      const s = get();
      const body = text.trim();
      if (!body) return;
      const status = queryClient.getQueryData<StatusPayload>(qk.status);
      if (s.activeRun || s.sending || status?.busy) {
        get().toast("Relay is still working on the last task. Stop it first or wait for it to finish.");
        return;
      }
      let id = opts.convId && !opts.convId.startsWith("tmp-") ? opts.convId : null;
      const now = Date.now();
      if (!id) {
        // Show the message at once; create the conversation in the background.
        const tmp = `tmp-${++tempSeq}`;
        set((st) => ({
          sending: tmp,
          chats: { ...st.chats, [tmp]: { ...addUser(emptyChat(), body, attachments, now), loaded: true, error: null } },
        }));
        go(paths.chat(tmp));
        try {
          const conv = await api.convCreate(opts.projectId ?? null);
          const title = titleFrom(body);
          set((st) => {
            const chats = { ...st.chats };
            chats[conv.id] = chats[tmp]!;
            delete chats[tmp];
            return { chats, sending: conv.id };
          });
          upsertConvCache({ ...conv, title });
          if (chatIdOf(getRequestedPath()) === tmp || chatIdOf(getCurrentPath()) === tmp) go(paths.chat(conv.id), { replace: true });
          id = conv.id;
        } catch (e) {
          set((st) => ({ sending: null, chats: { ...st.chats, [tmp]: { ...st.chats[tmp]!, error: `Could not create the chat: ${errorText(e)}` } } }));
          return;
        }
      } else {
        const convId = id;
        set({ sending: convId });
        patchChat(convId, (c) => ({ ...c, ...addUser(c, body, attachments, now) }));
        const conv = findConv(convId);
        if (conv && !conv.title) patchConvCache(convId, { title: titleFrom(body) });
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
          ...applyEvent(c, { type: "error", run_id: null, kind: "internal", message: errorText(e) }, Date.now()),
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
      if (chatIdOf(getCurrentPath()) !== convId) go(paths.chat(convId));
      await get().send(u.text, u.attachments, { convId });
    },

    stop: () => {
      api.stopTask().catch((e) => get().toast(`Could not stop: ${errorText(e)}`, "error"));
    },

    answerConfirm: (id, approved) => {
      set((s) => ({ confirms: s.confirms.filter((c) => c.id !== id) }));
      api.answerConfirm(id, approved).catch((e) => get().toast(`Could not send your answer: ${errorText(e)}`, "error"));
    },

    forgetChat: (id) =>
      set((s) => {
        const chats = { ...s.chats };
        delete chats[id];
        return { chats };
      }),

    togglePinProject: (id) => {
      set((s) => {
        const pinnedProjects = s.pinnedProjects.includes(id) ? s.pinnedProjects.filter((x) => x !== id) : [...s.pinnedProjects, id];
        lsSet(LS_PINNED_PROJECTS, JSON.stringify(pinnedProjects));
        return { pinnedProjects };
      });
    },

    setSidebarCollapsed: (v) => {
      set({ sidebarCollapsed: v });
      lsSet(LS_SIDEBAR, String(v));
    },
    toggleSidebar: (narrow) => {
      if (narrow) set((s) => ({ drawerOpen: !s.drawerOpen }));
      else get().setSidebarCollapsed(!get().sidebarCollapsed);
    },
    setDrawerOpen: (v) => set({ drawerOpen: v }),
    setSearchOpen: (o) => set({ searchOpen: o }),
    setProjectDialog: (d) => set({ projectDialog: d }),
    setConfirmDialog: (d) => set({ confirmDialog: d }),
    askDeleteConv: (conv, onConfirm) =>
      set({
        confirmDialog: {
          title: "Delete chat?",
          body: `"${conv.title || "New chat"}" and its task history in this chat will be deleted. This can't be undone.`,
          action: "Delete",
          destructive: true,
          onConfirm,
        },
      }),

    toast: (text, tone = "info", action) => {
      const id = ++toastSeq;
      set((s) => ({ toasts: [...s.toasts.slice(-2), { id, text, tone, action }] }));
      setTimeout(() => get().dismissToast(id), tone === "error" ? 6500 : 4000);
    },
    dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
    fillComposer: (text) => set({ composerFill: { text, nonce: Date.now() } }),
    focusComposer: () => set((s) => ({ composerFocus: s.composerFocus + 1 })),
    announce: (text) => set({ live: text }),
    setLastAppPath: (p) => set({ lastAppPath: p }),
  };
});

/** True while any task is running or being started. */
export const selectBusy = (s: AppState) => Boolean(s.activeRun || s.sending);
