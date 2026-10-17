// Runs before the stylesheet paints anything: applies the saved theme so the first frame already
// has the right colours (no light flash in dark mode). External file because the CSP forbids
// inline scripts. Mirrors parseAppearance/applyAppearance in src/store/appearance.ts.
(function () {
  var a = {};
  try {
    a = JSON.parse(localStorage.getItem("relay.appearance") || "{}") || {};
  } catch (e) {
    a = {};
  }
  var mode = a.mode === "light" || a.mode === "dark" ? a.mode : null;
  if (!mode) mode = window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  var root = document.documentElement;
  root.setAttribute("data-mode", mode);
  root.setAttribute("data-theme", a.theme === "graphite" || a.theme === "midnight" ? a.theme : "relay");
  root.setAttribute("data-accent", typeof a.accent === "string" ? a.accent : "terracotta");
  root.setAttribute("data-ui-font", a.uiFont === "system" || a.uiFont === "atkinson" ? a.uiFont : "inter");
  root.setAttribute("data-text-size", a.textSize === "small" || a.textSize === "large" ? a.textSize : "medium");
  root.setAttribute("data-density", a.density === "compact" ? "compact" : "comfortable");
  if (typeof a.sidebarWidth === "number" && isFinite(a.sidebarWidth)) {
    root.style.setProperty("--relay-sidebar-w", Math.round(Math.min(420, Math.max(220, a.sidebarWidth))) + "px");
  }
  root.style.colorScheme = mode;
})();
