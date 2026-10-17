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
  root.style.colorScheme = mode;
})();
