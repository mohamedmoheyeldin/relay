// In-app approvals: relay-screen asks here instead of a separate popup while Relay runs.
// Software-injected clicks and keys aimed at Relay's window are dropped by the app,
// so automation can't press these buttons; only the owner can.
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ShieldTick } from "@untitledui/icons";
import { Button } from "@/components/base/buttons/button";
import { type Approval, api } from "@/lib/api";

const Countdown = ({ seconds }: { seconds: number }) => {
    const [left, setLeft] = useState(Math.round(seconds));
    useEffect(() => {
        const t = setInterval(() => setLeft((s) => Math.max(0, s - 1)), 1000);
        return () => clearInterval(t);
    }, []);
    return <span className="tabular-nums">{left} s</span>;
};

const Card = ({ a }: { a: Approval }) => {
    const [busy, setBusy] = useState(false);
    const answer = async (decision: "approve" | "approve_batch" | "deny") => {
        setBusy(true);
        await api.approvalAnswer(a.id, decision).catch(() => {});
    };
    return (
        <div className="pointer-events-auto w-full max-w-xl rounded-2xl bg-primary p-4 shadow-xl ring-1 ring-secondary animate-in fade-in slide-in-from-bottom-3 duration-200">
            <div className="flex items-start gap-3">
                <ShieldTick className="mt-0.5 size-5 shrink-0 text-fg-brand-primary" />
                <div className="min-w-0 flex-1">
                    <div className="flex items-baseline justify-between gap-3">
                        <span className="text-md font-semibold text-primary">{a.title}</span>
                        <span className="shrink-0 text-xs text-quaternary">
                            {a.step && a.total ? `Step ${a.step} of ${a.total} · ` : ""}
                            <Countdown seconds={a.timeout} />
                        </span>
                    </div>
                    <p className="mt-1 text-sm break-words text-secondary">{a.detail}</p>
                    {a.image && (
                        <img src={`data:image/png;base64,${a.image}`} alt="Where Relay will act" className="mt-3 max-h-48 rounded-lg ring-1 ring-secondary" />
                    )}
                </div>
            </div>
            <div className="mt-4 flex flex-wrap justify-end gap-2">
                <Button size="sm" color="secondary" onClick={() => answer("deny")} isDisabled={busy}>
                    Deny
                </Button>
                {a.allow_batch && (
                    <Button size="sm" color="secondary" onClick={() => answer("approve_batch")} isDisabled={busy}>
                        {a.batch_label || "Approve rest of batch"}
                    </Button>
                )}
                <Button size="sm" onClick={() => answer("approve")} isDisabled={busy}>
                    {a.approve_label || "Approve"}
                </Button>
            </div>
        </div>
    );
};

/** Pending approvals, floating over whatever screen is open. */
export const ApprovalTray = () => {
    const [pending, setPending] = useState<Approval[]>([]);
    useEffect(() => {
        const subs = [
            listen<Approval>("approval", (e) => setPending((p) => [...p.filter((x) => x.id !== e.payload.id), e.payload])),
            listen<number>("approval-done", (e) => setPending((p) => p.filter((x) => x.id !== e.payload))),
        ];
        return () => {
            subs.forEach((s) => s.then((f) => f()));
        };
    }, []);
    if (pending.length === 0) return null;
    return (
        <div className="pointer-events-none fixed inset-x-0 bottom-28 z-50 flex flex-col items-center gap-3 px-4">
            {pending.map((a) => (
                <Card key={a.id} a={a} />
            ))}
        </div>
    );
};
