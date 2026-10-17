// TanStack Query hooks for every piece of backend data. Mutations update the cache
// optimistically and roll back on error.
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, errorText } from "@/lib/backend";
import { qk } from "@/lib/query";
import { applyPlatform, FALLBACK_PLATFORM } from "@/lib/platform";
import { applyHybridPatch, type ConvSummary, type Hybrid, type HybridPatch, type Project, type Settings, type SettingsPatch } from "@/lib/schemas";
import { patchConvCache, removeConvCache, selectBusy, upsertConvCache, useApp } from "@/store/app";
import { chatIdOf, getCurrentPath, go, paths } from "@/lib/nav";

const toast = (t: string, tone: "info" | "error" = "info") => useApp.getState().toast(t, tone);

// ---- reads -------------------------------------------------------------------------------------

export const useConversations = (archived = false) =>
  useQuery({ queryKey: qk.convs(archived), queryFn: () => api.convList(archived), staleTime: Infinity });

export const useConversation = (id: string | null) =>
  useQuery({
    queryKey: qk.conv(id ?? ""),
    queryFn: () => api.convGet(id!),
    enabled: Boolean(id) && !id!.startsWith("tmp-"),
    staleTime: Infinity,
    gcTime: 0,
  });

export const useProjects = () => useQuery({ queryKey: qk.projects, queryFn: () => api.projectList(), staleTime: Infinity });

export const useSearch = (q: string) =>
  useQuery({
    queryKey: qk.search(q),
    queryFn: () => api.search(q),
    enabled: q.trim().length > 0,
    staleTime: 5_000,
    placeholderData: (prev) => prev,
  });

export const useHybrid = () => useQuery({ queryKey: qk.hybrid, queryFn: () => api.getHybrid() });

export const useWorkflows = (convId?: string) =>
  useQuery({ queryKey: qk.workflows(convId), queryFn: () => api.workflowList({ convId, limit: 500 }) });

export const useWorkflow = (id: string | null) =>
  useQuery({ queryKey: qk.workflow(id ?? ""), queryFn: () => api.workflowGet(id!), enabled: Boolean(id) });

export const useEngines = () => useQuery({ queryKey: qk.engines, queryFn: () => api.getEngines() });

export const useSettings = () => useQuery({ queryKey: qk.settings, queryFn: () => api.getSettings(), staleTime: Infinity });

export const useStatus = () => useQuery({ queryKey: qk.status, queryFn: () => api.getStatus(), staleTime: Infinity });

export const useMcp = () => useQuery({ queryKey: qk.mcp, queryFn: () => api.mcpGet() });

export function usePlatform() {
  const q = useQuery({
    queryKey: qk.platform,
    queryFn: async () => {
      try {
        const p = await api.getPlatform();
        applyPlatform(p);
        return p;
      } catch {
        // An older backend without get_platform: keep solid colours and native chrome.
        applyPlatform(FALLBACK_PLATFORM);
        return FALLBACK_PLATFORM;
      }
    },
    staleTime: Infinity,
    retry: false,
  });
  return q.data ?? FALLBACK_PLATFORM;
}

/** True while a task runs (store state, or the backend says it is busy). */
export function useBusy(): boolean {
  const local = useApp(selectBusy);
  const status = useStatus();
  return local || Boolean(status.data?.busy);
}

// ---- conversation mutations (optimistic) -----------------------------------------------------

type ConvPatch = { title?: string; pinned?: boolean; archived?: boolean };

export function useUpdateConversation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, patch }: { id: string; patch: ConvPatch }) => api.convUpdate(id, patch),
    onMutate: async ({ id, patch }) => {
      await qc.cancelQueries({ queryKey: qk.convsAll });
      const prev = { active: qc.getQueryData<ConvSummary[]>(qk.convs(false)), archived: qc.getQueryData<ConvSummary[]>(qk.convs(true)) };
      const conv = prev.active?.find((c) => c.id === id) ?? prev.archived?.find((c) => c.id === id);
      if (conv && patch.archived !== undefined && patch.archived !== conv.archived) upsertConvCache({ ...conv, ...patch });
      else patchConvCache(id, patch);
      return prev;
    },
    onError: (e, { patch }, prev) => {
      if (prev) {
        qc.setQueryData(qk.convs(false), prev.active);
        qc.setQueryData(qk.convs(true), prev.archived);
      }
      const what = patch.title !== undefined ? "rename" : patch.pinned !== undefined ? (patch.pinned ? "pin" : "unpin") : patch.archived ? "archive" : "restore";
      toast(`Could not ${what} the chat: ${errorText(e)}`, "error");
    },
    onSuccess: (conv, { patch }) => {
      if (patch.archived) {
        toast("Chat archived");
        if (chatIdOf(getCurrentPath()) === conv.id) go(paths.home());
      }
    },
  });
}

export function useMoveConversation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, projectId }: { id: string; projectId: string | null }) => api.convMove(id, projectId),
    onMutate: async ({ id, projectId }) => {
      await qc.cancelQueries({ queryKey: qk.convsAll });
      const prev = qc.getQueryData<ConvSummary[]>(qk.convs(false));
      patchConvCache(id, { project_id: projectId });
      return prev;
    },
    onError: (e, _v, prev) => {
      qc.setQueryData(qk.convs(false), prev);
      toast(`Could not move the chat: ${errorText(e)}`, "error");
    },
    onSuccess: (_c, { projectId }) => {
      const p = qc.getQueryData<Project[]>(qk.projects)?.find((x) => x.id === projectId);
      toast(p ? `Moved to ${p.name}` : "Removed from the project");
    },
  });
}

export function useDeleteConversation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.convDelete(id),
    onMutate: async (id) => {
      await qc.cancelQueries({ queryKey: qk.convsAll });
      const prev = { active: qc.getQueryData<ConvSummary[]>(qk.convs(false)), archived: qc.getQueryData<ConvSummary[]>(qk.convs(true)) };
      removeConvCache(id);
      useApp.getState().forgetChat(id);
      if (chatIdOf(getCurrentPath()) === id) go(paths.home());
      return prev;
    },
    onError: (e, _id, prev) => {
      if (prev) {
        qc.setQueryData(qk.convs(false), prev.active);
        qc.setQueryData(qk.convs(true), prev.archived);
      }
      toast(`Could not delete the chat: ${errorText(e)}`, "error");
    },
    onSettled: () => void qc.invalidateQueries({ queryKey: qk.workflowsAll }),
  });
}

// ---- projects ----------------------------------------------------------------------------------

export function useCreateProject() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (p: { name: string; color: string; instructions: string }) => api.projectCreate(p.name, p.color, p.instructions),
    onSuccess: (p) => qc.setQueryData<Project[]>(qk.projects, (list) => [p, ...(list ?? [])]),
    onError: (e) => toast(`Could not create the project: ${errorText(e)}`, "error"),
  });
}

export function useUpdateProject() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, patch }: { id: string; patch: { name?: string; color?: string; instructions?: string } }) => api.projectUpdate(id, patch),
    onMutate: async ({ id, patch }) => {
      await qc.cancelQueries({ queryKey: qk.projects });
      const prev = qc.getQueryData<Project[]>(qk.projects);
      qc.setQueryData<Project[]>(qk.projects, (list) => list?.map((p) => (p.id === id ? { ...p, ...patch } : p)));
      return prev;
    },
    onError: (e, _v, prev) => {
      qc.setQueryData(qk.projects, prev);
      toast(`Could not save the project: ${errorText(e)}`, "error");
    },
    onSuccess: (p) => qc.setQueryData<Project[]>(qk.projects, (list) => list?.map((x) => (x.id === p.id ? p : x))),
  });
}

export function useDeleteProject() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.projectDelete(id),
    onMutate: async (id) => {
      await qc.cancelQueries({ queryKey: qk.projects });
      const prev = { projects: qc.getQueryData<Project[]>(qk.projects), convs: qc.getQueryData<ConvSummary[]>(qk.convs(false)) };
      qc.setQueryData<Project[]>(qk.projects, (list) => list?.filter((p) => p.id !== id));
      qc.setQueryData<ConvSummary[]>(qk.convs(false), (list) => list?.map((c) => (c.project_id === id ? { ...c, project_id: null } : c)));
      if (useApp.getState().pinnedProjects.includes(id)) useApp.getState().togglePinProject(id);
      if (getCurrentPath() === paths.project(id)) go(paths.home());
      return prev;
    },
    onError: (e, _id, prev) => {
      if (prev) {
        qc.setQueryData(qk.projects, prev.projects);
        qc.setQueryData(qk.convs(false), prev.convs);
      }
      toast(`Could not delete the project: ${errorText(e)}`, "error");
    },
  });
}

// ---- hybrid, providers, settings -------------------------------------------------------------

/** set_hybrid: applies at once (no restart); optimistic, rolled back on error. */
export function useSetHybrid() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (patch: HybridPatch) => api.setHybrid(patch),
    onMutate: async (patch) => {
      await qc.cancelQueries({ queryKey: qk.hybrid });
      const prev = qc.getQueryData<Hybrid>(qk.hybrid);
      if (prev) qc.setQueryData<Hybrid>(qk.hybrid, applyHybridPatch(prev, patch));
      return prev;
    },
    onError: (e, _p, prev) => {
      if (prev) qc.setQueryData(qk.hybrid, prev);
      toast(`Could not change the setting: ${errorText(e)}`, "error");
    },
    onSuccess: (h) => qc.setQueryData(qk.hybrid, h),
  });
}

/** refresh_providers: re-probes install and sign-in state (slow). */
export function useRefreshProviders() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.refreshProviders(),
    onSuccess: (providers) => {
      qc.setQueryData<Hybrid>(qk.hybrid, (h) => (h ? { ...h, providers } : h));
      useApp.getState().announce("Provider status refreshed");
    },
    onError: (e) => toast(`Could not check the providers: ${errorText(e)}`, "error"),
  });
}

export function useSetSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (patch: SettingsPatch) => api.setSettings(patch),
    onMutate: async (patch) => {
      await qc.cancelQueries({ queryKey: qk.settings });
      const prev = qc.getQueryData<Settings>(qk.settings);
      if (prev) qc.setQueryData<Settings>(qk.settings, { ...prev, ...patch });
      return prev;
    },
    onError: (e, _p, prev) => {
      if (prev) qc.setQueryData(qk.settings, prev);
      toast(`Could not save settings: ${errorText(e)}`, "error");
    },
    onSuccess: (s) => qc.setQueryData(qk.settings, s),
  });
}

export function useSetLocalModel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (model: string) => api.setLocalModel(model),
    onSettled: () => void qc.invalidateQueries({ queryKey: qk.engines }),
    onError: (e) => toast(`Could not select the model: ${errorText(e)}`, "error"),
  });
}
