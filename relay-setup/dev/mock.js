// Dev-only stand-in for Relay Setup's Tauri calls, to preview the page in a plain
// browser. Not part of the app: tauri.conf.json serves only relay-setup/ui.
//
// Load it before ui/app.js (for example with Playwright's addInitScript) and pick a
// scene with the URL hash: #not_installed, #older, #older_user, #same, #newer,
// #no_payload, #running, #progress, #done, #done_uninstall, #failed, #declined.
(() => {
  const scene = (location.hash || "#not_installed").slice(1);
  const machine = (version) => ({
    version,
    location: "C:\\Program Files\\Relay",
    uninstall_string: '"C:\\Program Files\\Relay\\uninstall.exe"',
    scope: "machine",
  });
  const base = {
    payload_version: "0.1.5",
    has_payload: true,
    default_location: "C:\\Program Files\\Relay",
    running: false,
  };
  const scenes = {
    not_installed: { status: "not_installed", installed: null, actions: ["install"] },
    older: { status: "older", installed: machine("0.1.4"), actions: ["update", "uninstall"] },
    older_user: {
      status: "older",
      installed: { ...machine("0.1.0"), location: "C:\\relay", scope: "user" },
      actions: ["update", "uninstall"],
    },
    same: { status: "same", installed: machine("0.1.5"), actions: ["repair", "uninstall"] },
    newer: { status: "newer", installed: machine("0.2.0"), actions: ["uninstall"] },
    no_payload: { status: "not_installed", installed: null, actions: ["install"], has_payload: false },
    running: { status: "older", installed: machine("0.1.4"), actions: ["update", "uninstall"], running: true },
  };
  const detection = { ...base, ...(scenes[scene] || scenes.older) };

  const listeners = {};
  const emit = (event, payload) => (listeners[event] || []).forEach((cb) => cb({ payload }));
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const outcomes = {
    done: { ok: true, code: 0, message: "Relay 0.1.5 is installed.", version: "0.1.5" },
    done_uninstall: { ok: true, code: 0, message: "Relay was removed.", version: null },
    failed: {
      ok: false,
      code: 2,
      message: "Setup stopped: Relay may still be running, or a newer Relay is installed.",
      version: null,
    },
    declined: {
      ok: false,
      code: 1223,
      message: "Windows permission was declined, so nothing was changed.",
      version: null,
    },
  };

  async function run(action) {
    const steps = ["close", "prepare", "permission", "work", "finish"];
    for (const id of steps) {
      emit("setup-step", { id, state: "active" });
      if (scene === "progress" && id === "work") return new Promise(() => {});
      await wait(150);
      emit("setup-step", { id, state: "done" });
    }
    if (action === "uninstall") return outcomes.done_uninstall;
    return outcomes[scene] || outcomes.done;
  }

  window.__RELAY_SETUP_MOCK__ = {
    invoke: async (cmd, args) => {
      if (cmd === "detect") return detection;
      if (cmd === "apply") return run(args.action);
      return null;
    },
    listen: async (event, cb) => {
      (listeners[event] = listeners[event] || []).push(cb);
      return () => {};
    },
    minimize: () => {},
    show: () => {},
  };
})();
