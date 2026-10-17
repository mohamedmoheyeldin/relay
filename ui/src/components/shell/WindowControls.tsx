import { useEffect, useState } from "react";
import { Copy, Minus, Square, X } from "lucide-react";
import { cn } from "@/lib/utils";
import { usePlatform } from "@/hooks/queries";

type Win = {
  minimize(): Promise<void>;
  toggleMaximize(): Promise<void>;
  close(): Promise<void>;
  isMaximized(): Promise<boolean>;
  isResizable(): Promise<boolean>;
  onResized(cb: () => void): Promise<() => void>;
};

async function currentWindow(): Promise<Win | null> {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return null;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow() as unknown as Win;
}

function Button({ label, onClick, danger, children }: { label: string; onClick: () => void; danger?: boolean; children: React.ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      tabIndex={-1}
      onClick={onClick}
      className={cn(
        "no-drag flex h-8 w-[46px] items-center justify-center text-fg-2 outline-none transition-colors duration-(--relay-dur-fast)",
        danger ? "hover:bg-[#c42b1c] hover:text-white" : "hover:bg-hover hover:text-fg",
      )}
    >
      {children}
    </button>
  );
}

/**
 * Minimise / maximise / close for the frameless Windows window, drawn in the app's own colours so the
 * title bar and the content read as one surface. Rendered only when the backend reports a custom title bar.
 */
export function WindowControls() {
  const custom = usePlatform().title_bar === "custom";
  const [win, setWin] = useState<Win | null>(null);
  const [maximized, setMaximized] = useState(false);
  const [resizable, setResizable] = useState(true);

  useEffect(() => {
    if (!custom) return;
    let off: (() => void) | undefined;
    let dead = false;
    void currentWindow().then(async (w) => {
      if (!w || dead) return;
      setWin(w);
      const refresh = () => {
        void w.isMaximized().then(setMaximized);
        void w.isResizable().then(setResizable);
      };
      refresh();
      off = await w.onResized(refresh);
    });
    return () => {
      dead = true;
      off?.();
    };
  }, [custom]);

  if (!custom || !win) return null;
  return (
    <>
      {/* A thin strip so the window can be dragged by its top edge on screens without a header (Setup). */}
      <div data-tauri-drag-region className="fixed inset-x-0 top-0 z-(--relay-z-sticky) h-2" />
      <div className="fixed top-0 right-0 z-(--relay-z-overlay) flex" role="group" aria-label="Window controls">
        <Button label="Minimize" onClick={() => void win.minimize()}>
          <Minus className="size-4" strokeWidth={1.5} aria-hidden="true" />
        </Button>
        {resizable && (
          <Button label={maximized ? "Restore" : "Maximize"} onClick={() => void win.toggleMaximize()}>
            {maximized ? <Copy className="size-[13px]" strokeWidth={1.5} aria-hidden="true" /> : <Square className="size-[13px]" strokeWidth={1.5} aria-hidden="true" />}
          </Button>
        )}
        <Button label="Close" danger onClick={() => void win.close()}>
          <X className="size-4" strokeWidth={1.5} aria-hidden="true" />
        </Button>
      </div>
    </>
  );
}
