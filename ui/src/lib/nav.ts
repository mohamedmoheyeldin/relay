// Navigation outside React components (store actions, keyboard shortcuts). The router registers
// its navigate function on mount; before that, the hash is set directly (HashRouter listens to it).
type NavigateFn = (to: string, opts?: { replace?: boolean }) => void;

let navigateFn: NavigateFn | null = null;
let currentPath = "/";
let requestedPath = "/";

export function registerNavigate(fn: NavigateFn | null) {
  navigateFn = fn;
}

export function setCurrentPath(p: string) {
  currentPath = p;
  requestedPath = p;
}

export function getCurrentPath(): string {
  return currentPath;
}

/** The path most recently requested with go() (navigation may not have rendered yet). */
export function getRequestedPath(): string {
  return requestedPath;
}

export function go(to: string, opts?: { replace?: boolean }) {
  requestedPath = to;
  if (navigateFn) navigateFn(to, opts);
  else if (typeof window !== "undefined") {
    if (opts?.replace) window.location.replace(`#${to}`);
    else window.location.hash = to;
  }
}

export const paths = {
  home: () => "/",
  chat: (id: string) => `/chat/${encodeURIComponent(id)}`,
  project: (id: string) => `/project/${encodeURIComponent(id)}`,
  tasks: (id?: string) => (id ? `/tasks/${encodeURIComponent(id)}` : "/tasks"),
  settings: (section = "general") => `/settings/${section}`,
  gallery: () => "/gallery",
};

/** The conversation id in a path like /chat/:id, if any. */
export function chatIdOf(path: string): string | null {
  const m = /^\/chat\/([^/?#]+)/.exec(path);
  return m ? decodeURIComponent(m[1]!) : null;
}
