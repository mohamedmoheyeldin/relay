// Relay draws its own title row, like the reference chat app: the window has no
// system frame, so the top strip drags the window and these buttons sit at its right.
import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy01, Minus, Square, XClose } from "@untitledui/icons";
import { cx } from "@/utils/cx";

const win = () => getCurrentWindow();

/** The empty strip across the top of the window that moves it; double-click maximizes. */
export const DragStrip = () => <div data-tauri-drag-region className="fixed inset-x-0 top-0 z-30 h-9" />;

export const WindowControls = () => {
    const [maximized, setMaximized] = useState(false);
    useEffect(() => {
        let off: (() => void) | undefined;
        try {
            const w = win();
            w.isMaximized().then(setMaximized, () => {});
            w.onResized(() => w.isMaximized().then(setMaximized, () => {})).then((f) => (off = f), () => {});
        } catch {
            /* outside the desktop app (previews) there is no window to follow */
        }
        return () => off?.();
    }, []);
    const button = "flex h-9 w-[46px] cursor-default items-center justify-center text-fg-tertiary transition duration-100 outline-hidden";
    return (
        <div className="fixed top-0 right-0 z-50 flex">
            <button aria-label="Minimize" className={cx(button, "hover:bg-primary_hover hover:text-fg-secondary")} onClick={() => win().minimize()}>
                <Minus className="size-4" />
            </button>
            <button
                aria-label={maximized ? "Restore" : "Maximize"}
                className={cx(button, "hover:bg-primary_hover hover:text-fg-secondary")}
                onClick={() => win().toggleMaximize()}
            >
                {maximized ? <Copy01 className="size-3.5" /> : <Square className="size-3.5" />}
            </button>
            <button aria-label="Close" className={cx(button, "hover:bg-[#c42b1c] hover:text-white")} onClick={() => win().close()}>
                <XClose className="size-4" />
            </button>
        </div>
    );
};
