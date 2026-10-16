// Setup and uninstall screens: the release exe is its own installer (see docs/ui-contract.md).
import { useEffect, useState, type ReactNode } from "react";
import { CircleCheck, FolderOpen, TriangleAlert } from "lucide-react";
import { api, listenEvents } from "../lib/backend";
import { formatBytes } from "../lib/grouping";
import type { InstallerInfo, Progress } from "../lib/types";
import { Button, RelayMark, Toggle } from "../components/ui";
import { ProgressBar } from "../settings/parts";

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

function Shell({ children, footer }: { children: ReactNode; footer: ReactNode }) {
  return (
    <div className="flex h-full flex-col bg-bg text-fg">
      <div className="scroll-thin flex min-h-0 flex-1 flex-col items-center overflow-y-auto px-10 pt-14 pb-6">{children}</div>
      <div className="flex shrink-0 items-center justify-end gap-2 px-8 pt-3 pb-7">{footer}</div>
    </div>
  );
}

function Logo({ state }: { state?: "ok" | "bad" }) {
  return (
    <span className="relative mb-6 inline-flex h-[72px] w-[72px] items-center justify-center rounded-[22px] border border-line bg-surface shadow-[0_8px_30px_rgba(0,0,0,0.25)]">
      <RelayMark size={38} />
      {state === "ok" && <CircleCheck size={26} className="absolute -right-2 -bottom-2 rounded-full bg-bg text-success" />}
      {state === "bad" && <TriangleAlert size={24} className="absolute -right-2 -bottom-2 rounded-full bg-bg p-0.5 text-danger" />}
    </span>
  );
}

const errText = (e: unknown) => (e instanceof Error ? e.message : String(e));

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
      setError(errText(e));
      setPhase("error");
    }
  };

  if (phase === "done") {
    return (
      <Shell
        footer={
          <Button variant="primary" className="h-10 px-6" onClick={() => api.installerLaunch().catch((e) => setError(errText(e)))}>
            Open Relay
          </Button>
        }
      >
        <Logo state="ok" />
        <h1 className="text-[26px] leading-9 font-normal">{update ? "Relay is up to date" : "Relay is installed"}</h1>
        <p className="mt-2 max-w-[400px] text-center text-[14.5px] leading-6 text-muted">
          You'll find Relay in the Start menu{desktop && !update ? " and on your desktop" : ""}. Relay Local AI sets itself up the first time you open it, so give it a few minutes.
        </p>
        {error && <p className="mt-4 text-[13px] text-danger">{error}</p>}
      </Shell>
    );
  }

  if (phase === "working") {
    return (
      <Shell footer={<span className="text-[12.5px] text-subtle">Please keep this window open.</span>}>
        <Logo />
        <h1 className="text-[26px] leading-9 font-normal">{update ? "Updating Relay…" : "Installing Relay…"}</h1>
        <div className="mt-10 w-full max-w-[400px]" aria-live="polite">
          <ProgressBar percent={progress?.percent ?? null} label={progress?.label || "Preparing"} />
        </div>
      </Shell>
    );
  }

  return (
    <Shell
      footer={
        <>
          <Button variant="ghost" onClick={() => api.appQuit().catch(() => window.close())}>
            Cancel
          </Button>
          <Button variant="primary" className="h-10 px-6" onClick={() => void install()}>
            {phase === "error" ? "Try again" : update ? "Update" : "Install"}
          </Button>
        </>
      }
    >
      <Logo state={phase === "error" ? "bad" : undefined} />
      <h1 className="text-[26px] leading-9 font-normal">{update ? "Update Relay" : "Install Relay"}</h1>
      <p className="mt-1.5 text-[14px] text-subtle">
        {update ? `Version ${info.installed_version} → ${info.version}` : `Version ${info.version}`}
      </p>
      <p className="mt-4 max-w-[400px] text-center text-[14.5px] leading-6 text-muted">
        Relay works on your PC for you: tidying files, opening apps, filling in forms. {update ? "Your chats and settings are kept." : "It only takes a moment."}
      </p>
      {phase === "error" && (
        <div role="alert" className="mt-5 w-full max-w-[420px] rounded-2xl bg-danger-soft px-4 py-3 text-[13.5px] leading-5">
          <div className="font-medium">Setup didn't finish</div>
          <div className="text-muted">{error}</div>
        </div>
      )}
      <div className="mt-7 w-full max-w-[420px] divide-y divide-[var(--border)] rounded-2xl border border-line px-4">
        <div className="flex items-start gap-3 py-3">
          <FolderOpen size={17} className="mt-0.5 shrink-0 text-subtle" aria-hidden="true" />
          <div className="min-w-0">
            <div className="text-[13px] text-subtle">Install location</div>
            <div className="break-all font-mono text-[12.5px]">{info.install_dir}</div>
          </div>
        </div>
        {!update && (
          <label className="flex items-center gap-3 py-3">
            <span className="flex-1 text-[14px]">Add a desktop shortcut</span>
            <Toggle checked={desktop} onChange={setDesktop} label="Add a desktop shortcut" />
          </label>
        )}
        <label className="flex items-center gap-3 py-3">
          <span className="flex-1 text-[14px]">Start Relay when I sign in</span>
          <Toggle checked={startup} onChange={setStartup} label="Start Relay when I sign in" />
        </label>
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
      setError(errText(e));
      setPhase("error");
    }
  };
  const quit = () => api.appQuit().catch(() => window.close());

  if (phase === "done") {
    return (
      <Shell
        footer={
          <Button variant="primary" className="h-10 px-6" onClick={quit}>
            Close
          </Button>
        }
      >
        <Logo state="ok" />
        <h1 className="text-[26px] leading-9 font-normal">Relay was removed</h1>
        <p className="mt-2 max-w-[400px] text-center text-[14.5px] leading-6 text-muted">
          {deleteData ? "Your chats, settings and the local AI were deleted too." : "Your chats and settings were kept, in case you install Relay again."}
        </p>
      </Shell>
    );
  }

  if (phase === "working") {
    return (
      <Shell footer={<span className="text-[12.5px] text-subtle">Please keep this window open.</span>}>
        <Logo />
        <h1 className="text-[26px] leading-9 font-normal">Removing Relay…</h1>
        <div className="mt-10 w-full max-w-[400px]" aria-live="polite">
          <ProgressBar percent={progress?.percent ?? null} label={progress?.label || "Preparing"} />
        </div>
      </Shell>
    );
  }

  return (
    <Shell
      footer={
        <>
          <Button variant="ghost" onClick={quit}>
            Cancel
          </Button>
          <Button variant="danger" className="h-10 px-6" onClick={() => void run()}>
            {phase === "error" ? "Try again" : "Remove Relay"}
          </Button>
        </>
      }
    >
      <Logo state={phase === "error" ? "bad" : undefined} />
      <h1 className="text-[26px] leading-9 font-normal">Remove Relay?</h1>
      <p className="mt-1.5 text-[14px] text-subtle">Version {info.installed_version ?? info.version}</p>
      <p className="mt-4 max-w-[400px] text-center text-[14.5px] leading-6 text-muted">Relay will be closed and removed from this PC.</p>
      {phase === "error" && (
        <div role="alert" className="mt-5 w-full max-w-[420px] rounded-2xl bg-danger-soft px-4 py-3 text-[13.5px] leading-5">
          <div className="font-medium">Removal didn't finish</div>
          <div className="text-muted">{error}</div>
        </div>
      )}
      <label className="mt-7 flex w-full max-w-[420px] items-center gap-3 rounded-2xl border border-line px-4 py-3.5">
        <span className="min-w-0 flex-1">
          <span className="block text-[14px]">
            Also delete my chats, settings and the downloaded local AI ({formatBytes(info.data_bytes)})
          </span>
          <span className="mt-0.5 block break-all font-mono text-[12px] text-subtle">{info.data_dir}</span>
        </span>
        <Toggle checked={deleteData} onChange={setDeleteData} label="Also delete my data" />
      </label>
    </Shell>
  );
}

export default function Installer({ info }: { info: InstallerInfo }) {
  useEffect(() => {
    document.title = info.mode === "uninstall" ? "Remove Relay" : "Relay Setup";
  }, [info.mode]);
  return info.mode === "uninstall" ? <UninstallScreen info={info} /> : <InstallScreen info={info} />;
}
