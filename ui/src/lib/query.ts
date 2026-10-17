// TanStack Query client and keys. All async backend data goes through these keys; backend events
// invalidate or patch them (see store/app.ts, `flush`).
import { QueryClient } from "@tanstack/react-query";

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // Relay is the only writer of most data, so cached data stays fresh until an event or a
      // mutation says otherwise.
      staleTime: 30_000,
      gcTime: 10 * 60_000,
      retry: 1,
      refetchOnWindowFocus: false,
    },
    mutations: { retry: 0 },
  },
});

export const qk = {
  convs: (archived = false) => ["convs", { archived }] as const,
  convsAll: ["convs"] as const,
  conv: (id: string) => ["conv", id] as const,
  projects: ["projects"] as const,
  search: (q: string) => ["search", q] as const,
  hybrid: ["hybrid"] as const,
  workflows: (convId?: string) => ["workflows", convId ?? "all"] as const,
  workflowsAll: ["workflows"] as const,
  workflow: (id: string) => ["workflow", id] as const,
  engines: ["engines"] as const,
  settings: ["settings"] as const,
  status: ["status"] as const,
  platform: ["platform"] as const,
  mcp: ["mcp"] as const,
};
