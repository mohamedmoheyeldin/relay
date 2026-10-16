import React from "react";
import ReactDOM from "react-dom/client";
// Fonts are bundled with the app; nothing loads from the network at runtime.
import "@fontsource-variable/inter";
import "@fontsource-variable/source-serif-4";
import { ThemeProvider } from "@/providers/theme-provider";
import App from "./App";
import { Popup } from "./screens/popup";
import "@/styles/globals.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider defaultTheme="system" storageKey="relay-theme">
      {window.location.hash === "#popup" ? <Popup /> : <App />}
    </ThemeProvider>
  </React.StrictMode>,
);
