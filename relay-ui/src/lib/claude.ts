// Claude Code's state and the "does Claude answer" check, shared by every screen
// so the check runs once per sign-in instead of once per screen.
import { useEffect, useState } from "react";
import { type ClaudeState, api, errorText } from "./api";

export type CheckState = { status: "idle" } | { status: "running" } | { status: "ok"; text: string } | { status: "error"; text: string };

interface Store {
    state: ClaudeState | null;
    loading: boolean;
    check: CheckState;
}

let store: Store = { state: null, loading: false, check: { status: "idle" } };
const listeners = new Set<(s: Store) => void>();
let inflight: Promise<ClaudeState> | null = null;

function set(next: Partial<Store>) {
    store = { ...store, ...next };
    for (const l of listeners) l(store);
}

/** Ask Claude Code for its install and sign-in state again. */
export function refreshClaude(): Promise<ClaudeState> {
    if (inflight) return inflight;
    set({ loading: true });
    inflight = api
        .claudeState()
        .then((state) => {
            const wasIn = !!store.state?.auth.loggedIn;
            set({ state, loading: false });
            // A new sign-in (or sign-out) makes the last check stale.
            if (wasIn !== !!state.auth.loggedIn) set({ check: { status: "idle" } });
            return state;
        })
        .finally(() => {
            inflight = null;
            if (store.loading) set({ loading: false });
        });
    return inflight;
}

/** Send Claude a one-word test message. Only one runs at a time. */
export async function runClaudeCheck(): Promise<void> {
    if (store.check.status === "running") return;
    set({ check: { status: "running" } });
    try {
        const text = await api.claudeVerify();
        set({ check: { status: "ok", text: text.trim() || "OK" } });
    } catch (e) {
        set({ check: { status: "error", text: errorText(e) } });
    }
}

export function useClaude(options: { autoCheck?: boolean } = {}) {
    const [s, setS] = useState(store);
    useEffect(() => {
        listeners.add(setS);
        if (!store.state) refreshClaude().catch(() => {});
        return () => {
            listeners.delete(setS);
        };
    }, []);
    const signedIn = !!s.state?.installed && !!s.state.auth.loggedIn;
    useEffect(() => {
        if (options.autoCheck && signedIn && store.check.status === "idle") runClaudeCheck();
    }, [options.autoCheck, signedIn]);
    return { ...s, signedIn, refresh: refreshClaude, runCheck: runClaudeCheck };
}
