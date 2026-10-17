// Setup and uninstall screens: the downloaded Setup.exe is the app itself (docs/ui-contract.md).
// Fixed 560x620 window with the native title bar, same design system as the app.
import { useEffect, useId, useState, type ReactNode } from "react";
import { CircleCheck, FolderOpen, ShieldCheck, TriangleAlert } from "lucide-react";
import { api, errorText, listenEvents } from "@/lib/backend";
import { formatBytes } from "@/lib/grouping";
import type { InstallerInfo, Progress } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { ProgressRow } from "@/components/ui/progress";
import { RelayMark, ErrorNote } from "@/components/common";

type Phase = "start" | "working" | "done" | "error";

function useInstallerProgress(): Progress | null {
  const [p, setP] = useState<Progress | null>(null);
  useEffect(() => {
    let off: (() => void) | undefined;
    let live = true;
    void listenEvents((ev) => {
      if (ev.type === "progress" && ev.id === "installer") setP(ev);
    }).then((u) => {
      if (live) off = u;
      else u();
    });
    return () => {
      live = false;
      off?.();
    };
  }, []);
  return p;
}

function Shell({ children, footer, label }: { children: ReactNode; footer: ReactNode; label: string }) {
  return (
    <main className="flex h-full flex-col bg-canvas text-fg" aria-label={label} data-testid="installer">
      <div className="flex min-h-0 flex-1 flex-col items-center overflow-y-auto px-10 pt-14 pb-6 scroll-thin">{children}</div>
      <div className="flex shrink-0 items-center justify-end gap-2 border-t border-line px-8 py-4">{footer}</div>
    </main>
  );
}

function Logo({ state }: { state?: "ok" | "bad" }) {
  return (
    <span className="relative mb-7 inline-flex size-[72px] items-center justify-center rounded-[20px] border border-line bg-surface shadow-card">
      <RelayMark size={38} />
      {state === "ok" && <CircleCheck className="absolute -right-2 -bottom-2 size-7 rounded-full bg-canvas fill-success text-surface" aria-hidden="true" />}
      {state === "bad" && <TriangleAlert className="absolute -right-2 -bottom-2 size-7 rounded-full bg-canvas p-0.5 text-danger" aria-hidden="true" />}
    </span>
  );
}

function Title({ children }: { children: ReactNode }) {
  return <h1 className="text-center font-display text-title">{children}</h1>;
}

function ToggleRow({ label, desc, checked, onChange }: { label: string; desc?: string; checked: boolean; onChange: (v: boolean) => void }) {
  const id = useId();
  return (
    <div className="flex items-center gap-3 py-3">
      <label htmlFor={id} className="min-w-0 flex-1">
        <span className="block text-ui text-fg">{label}</span>
        {desc && <span className="mt-0.5 block text-caption break-all text-fg-muted">{desc}</span>}
      </label>
      <Switch id={id} checked={checked} onCheckedChange={onChange} />
    </div>
  );
}

function InstallScreen({ info }: { info: InstallerInfo }) {
  const [phase, setPhase] = useState<Phase>("start");
  const [desktop, setDesktop] = useState(true);
  const [startup, setStartup] = useState(true);
  const [error, setError] = useState("");
  const progress = useInstallerProgress();
  const update = Boolean(info.installed_version);

  useEffect(() => {
    if (phase !== "working" || !progress?.done) return;
    if (progress.error) {
      setError(progress.error);
      setPhase("error");
    } else setPhase("done");
  }, [progress, phase]);

  const install = async () => {
    setError("");
    setPhase("working");
    try {
      await api.installerInstall(update ? false : desktop, startup); // an update keeps the existing shortcuts
      setPhase((p) => (p === "working" ? "done" : p));
    } catch (e) {
      setError(errorText(e));
      setPhase("error");
    }
  };

  if (phase === "done") {
    return (
      <Shell
        label="Setup finished"
        footer={
          <Button variant="primary" size="lg" className="px-6" autoFocus onClick={() => api.installerLaunch().catch((e) => setError(errorText(e)))}>
            Open Relay
          </Button>
        }
      >
        <Logo state="ok" />
        <Title>{update ? "Relay is up to date" : "Relay is installed"}</Title>
        <p className="mt-3 max-w-[400px] text-center text-ui leading-6 text-fg-2">
          You'll find Relay in the Start menu{desktop && !update ? " and on your desktop" : ""}. Relay Local AI sets itself up the first time you open it, so give it a few minutes.
        </p>
        {error && <p className="mt-4 text-caption text-danger">{error}</p>}
      </Shell>
    );
  }

  if (phase === "working") {
    return (
      <Shell label="Installing" footer={<span className="text-caption text-fg-muted">Please keep this window open.</span>}>
        <Logo />
        <Title>{update ? "Updating Relay…" : "Installing Relay…"}</Title>
        <div className="mt-10 w-full max-w-[400px]" aria-live="polite">
          <ProgressRow percent={progress?.percent ?? null} label={progress?.label || "Preparing"} />
        </div>
      </Shell>
    );
  }

  return (
    <Shell
      label={update ? "Update Relay" : "Install Relay"}
      footer={
        <>
          <Button variant="ghost" size="lg" onClick={() => api.appQuit().catch(() => window.close())}>
            Cancel
          </Button>
          <Button variant="primary" size="lg" className="px-6" onClick={() => void install()}>
            {phase === "error" ? "Try again" : update ? "Update" : "Install"}
          </Button>
        </>
      }
    >
      <Logo state={phase === "error" ? "bad" : undefined} />
      <Title>{update ? "Update Relay" : "Install Relay"}</Title>
      <p className="mt-1.5 text-caption text-fg-muted">{update ? `Version ${info.installed_version} → ${info.version}` : `Version ${info.version}`}</p>
      <p className="mt-4 max-w-[400px] text-center text-ui leading-6 text-fg-2">
        Relay works on your PC for you and checks every step it takes. {update ? "Your chats and settings are kept." : "It only takes a moment."}
      </p>
      {phase === "error" && (
        <div className="mt-5 w-full max-w-[420px]">
          <ErrorNote title="Setup didn't finish">{error}</ErrorNote>
        </div>
      )}
      <div className="mt-7 w-full max-w-[420px] divide-y divide-line rounded-xl border border-line bg-surface px-4">
        <div className="flex items-start gap-3 py-3">
          <FolderOpen className="mt-0.5 size-4 shrink-0 text-fg-muted" aria-hidden="true" />
          <div className="min-w-0">
            <div className="text-caption text-fg-muted">Install location</div>
            <div className="font-mono text-[12.5px] break-all text-fg">{info.install_dir}</div>
            <div className="mt-1 flex items-center gap-1 text-caption text-fg-muted">
              <ShieldCheck className="size-3.5" aria-hidden="true" /> Windows will ask for permission.
            </div>
          </div>
        </div>
        {!update && <ToggleRow label="Add a desktop shortcut" checked={desktop} onChange={setDesktop} />}
        <ToggleRow label="Start Relay with Windows" checked={startup} onChange={setStartup} />
      </div>
    </Shell>
  );
}

function UninstallScreen({ info }: { info: InstallerInfo }) {
  const [phase, setPhase] = useState<Phase>("start");
  const [deleteData, setDeleteData] = useState(false);
  const [error, setError] = useState("");
  const progress = useInstallerProgress();

  useEffect(() => {
    if (phase !== "working" || !progress?.done) return;
    if (progress.error) {
      setError(progress.error);
      setPhase("error");
    } else setPhase("done");
  }, [progress, phase]);

  const run = async () => {
    setError("");
    setPhase("working");
    try {
      await api.uninstallRun(deleteData);
      setPhase((p) => (p === "working" ? "done" : p));
    } catch (e) {
      setError(errorText(e));
      setPhase("error");
    }
  };
  const quit = () => api.appQuit().catch(() => window.close());

  if (phase === "done") {
    return (
      <Shell
        label="Removal finished"
        footer={
          <Button variant="primary" size="lg" className="px-6" autoFocus onClick={quit}>
            Close
          </Button>
        }
      >
        <Logo state="ok" />
        <Title>Relay was removed</Title>
        <p className="mt-3 max-w-[400px] text-center text-ui leading-6 text-fg-2">
          {deleteData ? "Your chats, settings and the local AI were deleted too." : "Your chats and settings were kept, in case you install Relay again."}
        </p>
      </Shell>
    );
  }

  if (phase === "working") {
    return (
      <Shell label="Removing" footer={<span className="text-caption text-fg-muted">Please keep this window open.</span>}>
        <Logo />
        <Title>Removing Relay…</Title>
        <div className="mt-10 w-full max-w-[400px]" aria-live="polite">
          <ProgressRow percent={progress?.percent ?? null} label={progress?.label || "Preparing"} />
        </div>
      </Shell>
    );
  }

  return (
    <Shell
      label="Remove Relay"
      footer={
        <>
          <Button variant="ghost" size="lg" onClick={quit}>
            Cancel
          </Button>
          <Button variant="danger" size="lg" className="px-6" onClick={() => void run()}>
            {phase === "error" ? "Try again" : "Remove Relay"}
          </Button>
        </>
      }
    >
      <Logo state={phase === "error" ? "bad" : undefined} />
      <Title>Remove Relay?</Title>
      <p className="mt-1.5 text-caption text-fg-muted">Version {info.installed_version ?? info.version}</p>
      <p className="mt-4 max-w-[400px] text-center text-ui leading-6 text-fg-2">Relay will be closed and removed from this PC.</p>
      {phase === "error" && (
        <div className="mt-5 w-full max-w-[420px]">
          <ErrorNote title="Removal didn't finish">{error}</ErrorNote>
        </div>
      )}
      <div className="mt-7 w-full max-w-[420px] rounded-xl border border-line bg-surface px-4">
        <ToggleRow
          label={`Also delete my chats, settings and the downloaded local AI (${formatBytes(info.data_bytes)})`}
          desc={info.data_dir}
          checked={deleteData}
          onChange={setDeleteData}
        />
      </div>
    </Shell>
  );
}

export default function Installer({ info }: { info: InstallerInfo }) {
  useEffect(() => {
    document.title = info.mode === "uninstall" ? "Remove Relay" : "Relay Setup";
  }, [info.mode]);
  return info.mode === "uninstall" ? <UninstallScreen info={info} /> : <InstallScreen info={info} />;
}
