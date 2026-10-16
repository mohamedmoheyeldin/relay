import { lazy, StrictMode, Suspense } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/inter";
import "./index.css";
import "./store/appearance"; // applies the theme before the first paint
import { App } from "./App";
import { api } from "./lib/backend";
import type { InstallerInfo } from "./lib/types";

const Installer = lazy(() => import("./installer/Installer"));
const root = createRoot(document.getElementById("root")!);

function render(info: InstallerInfo | null) {
  root.render(
    <StrictMode>
      {info && info.mode !== "app" ? (
        <Suspense fallback={null}>
          <Installer info={info} />
        </Suspense>
      ) : (
        <App />
      )}
    </StrictMode>,
  );
}

// Decide between the app and the setup/uninstall screens before rendering anything, so the
// app shell never flashes in installer mode. A backend without `installer_info` means "app".
const fallback = setTimeout(() => render(null), 1500);
api
  .installerInfo()
  .then(
    (info) => info,
    () => null,
  )
  .then((info) => {
    clearTimeout(fallback);
    render(info);
  });
