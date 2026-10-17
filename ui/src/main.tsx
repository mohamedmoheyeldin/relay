import { lazy, StrictMode, Suspense } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/inter/wght.css";
import "@fontsource-variable/newsreader/opsz.css";
import "@fontsource-variable/geist-mono/wght.css";
// Only downloaded when "Interface font: Atkinson Hyperlegible" is chosen (font files load on use).
import "@fontsource-variable/atkinson-hyperlegible-next/wght.css";
import "./styles/index.css";
import "./store/appearance"; // applies the theme before the first render
import { App } from "./app/App";
import { api } from "./lib/backend";
import { applyPlatform, FALLBACK_PLATFORM } from "./lib/platform";
import type { InstallerInfo } from "./lib/types";

const Installer = lazy(() => import("./pages/Installer"));
const root = createRoot(document.getElementById("root")!);
applyPlatform(FALLBACK_PLATFORM);

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

// The setup/uninstall screens must never flash the app shell, so ask installer_info first. It is a
// cheap local call; a backend without it, or a slow answer, falls back to the app.
const fallback = setTimeout(() => render(null), 600);
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
