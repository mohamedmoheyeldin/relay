// The in-app terminal pane (xterm.js over a ConPTY) used to sign in to Claude Code.
import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal as XTerm } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { api, errorText } from "@/lib/api";

export const Terminal = ({ kind, onExit, onError }: { kind: "login"; onExit: () => void; onError: (message: string) => void }) => {
    const host = useRef<HTMLDivElement>(null);
    const exitRef = useRef(onExit);
    exitRef.current = onExit;

    useEffect(() => {
        if (!host.current) return;
        const term = new XTerm({
            fontFamily: "Cascadia Mono, Consolas, monospace",
            fontSize: 13,
            cursorBlink: true,
            theme: { background: "#0c111d", foreground: "#f5f5f6" },
        });
        const fit = new FitAddon();
        term.loadAddon(fit);
        term.open(host.current);
        fit.fit();
        const unlisten: Promise<() => void>[] = [
            listen<number[]>("pty-data", (e) => term.write(new Uint8Array(e.payload))),
            listen("pty-exit", () => exitRef.current()),
        ];
        const input = term.onData((d) => api.ptyWrite(d).catch(() => {}));
        api.ptyStart(kind, term.cols, term.rows).catch((e) => onError(errorText(e)));
        const ro = new ResizeObserver(() => {
            fit.fit();
            api.ptyResize(term.cols, term.rows).catch(() => {});
        });
        ro.observe(host.current);
        term.focus();
        return () => {
            ro.disconnect();
            input.dispose();
            unlisten.forEach((p) => p.then((f) => f()));
            api.ptyKill().catch(() => {});
            term.dispose();
        };
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [kind]);

    return <div ref={host} className="h-72 w-full overflow-hidden rounded-lg bg-[#0c111d] p-2" />;
};
