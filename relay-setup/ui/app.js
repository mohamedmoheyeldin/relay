// Relay Setup's page. Plain JS, no bundler: Tauri's global API (withGlobalTauri)
// in the app; a dev-only mock (relay-setup/dev/mock.js) when previewed in a browser.
"use strict";

const bridge = (() => {
  const T = window.__TAURI__;
  if (T) {
    const win = T.window.getCurrentWindow();
    return {
      invoke: T.core.invoke,
      listen: T.event.listen,
      minimize: () => win.minimize(),
      show: () => win.show(),
    };
  }
  return window.__RELAY_SETUP_MOCK__ || null;
})();

const view = document.getElementById("view");
let detection = null;
let busy = false;

const ICONS = {
  check: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>',
  cross: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 7l10 10M17 7L7 17"/></svg>',
  stepDone:
    '<svg viewBox="0 0 18 18" aria-hidden="true"><circle class="check-circle" cx="9" cy="9" r="9"/><path class="check" d="M5.5 9.2l2.3 2.3 4.7-4.9"/></svg>',
};

function el(tag, attrs = {}, children = []) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") node.className = v;
    else if (k === "text") node.textContent = v;
    else node.setAttribute(k, v);
  }
  for (const child of [].concat(children)) {
    node.append(child instanceof Node ? child : document.createTextNode(String(child)));
  }
  return node;
}

function mount(templateId) {
  const node = document.getElementById(templateId).content.firstElementChild.cloneNode(true);
  view.replaceChildren(node);
  const slots = {};
  node.querySelectorAll("[data-slot]").forEach((s) => (slots[s.dataset.slot] = s));
  return slots;
}

function row(label, value) {
  return el("div", { class: "row" }, [el("span", { class: "label", text: label }), value]);
}

function pill(text, accent = false) {
  return el("span", { class: accent ? "pill accent" : "pill", text });
}

function pathValue(text) {
  return el("span", { class: "value path", title: text, text });
}

function setButton(button, label, onClick, extraClass) {
  button.hidden = false;
  button.textContent = label;
  if (extraClass) button.classList.add(extraClass);
  button.onclick = onClick;
}

const installedVersion = () => detection.installed?.version || "unknown";
const installedLocation = () => detection.installed?.location || detection.default_location;

// --- First screen: what's on this PC and what can be done ---------------------

function renderChoose() {
  const d = detection;
  const s = mount("t-choose");
  const ship = d.payload_version;
  const card = s.card;

  switch (d.status) {
    case "not_installed":
      s.title.textContent = "Install Relay";
      s.subtitle.textContent = "Local hands, cloud brain: a desktop operator for Windows.";
      card.append(
        row("Version", pill(ship, true)),
        row("Location", pathValue(d.default_location)),
        row("For", el("span", { class: "value", text: "Everyone on this PC" })),
      );
      setButton(s.primary, `Install Relay ${ship}`, () => start("install"));
      break;

    case "older": {
      s.title.textContent = "Update Relay";
      s.subtitle.textContent =
        "A newer Relay is ready. Your chats, settings and models stay as they are.";
      const versions = el("span", { class: "value versions" }, [
        pill(installedVersion()),
        el("span", { class: "arrow", text: "→" }),
        pill(ship, true),
      ]);
      card.append(row("Version", versions));
      if (d.installed?.scope === "user") {
        card.append(
          row("Now in", pathValue(installedLocation())),
          row("Moves to", pathValue(d.default_location)),
        );
      } else {
        card.append(row("Location", pathValue(installedLocation())));
      }
      setButton(s.primary, `Update Relay ${installedVersion()} → ${ship}`, () => start("update"));
      setButton(s.secondary, "Uninstall", confirmUninstall);
      break;
    }

    case "same":
      s.title.textContent = `Relay ${ship} is installed`;
      s.subtitle.textContent =
        "Repair puts back missing or damaged files. Your settings stay as they are.";
      card.append(
        row("Version", pill(installedVersion(), true)),
        row("Location", pathValue(installedLocation())),
      );
      setButton(s.primary, "Repair", () => start("repair"));
      setButton(s.tertiary, "Open Relay", openRelay);
      setButton(s.secondary, "Uninstall", confirmUninstall);
      break;

    case "newer":
      s.title.textContent = "A newer Relay is installed";
      s.subtitle.textContent = `This setup has Relay ${ship}, so there is nothing to update.`;
      card.append(
        row("Installed", pill(installedVersion(), true)),
        row("This setup", pill(ship)),
        row("Location", pathValue(installedLocation())),
      );
      setButton(s.primary, "Close", quit);
      setButton(s.tertiary, "Open Relay", openRelay);
      setButton(s.secondary, "Uninstall", confirmUninstall);
      break;
  }

  const needsPayload = d.status !== "newer";
  if (needsPayload && !d.has_payload) {
    s.notice.hidden = false;
    s.notice.textContent =
      "This copy of Relay Setup has no installer inside. Download Relay-Setup.exe again.";
    s.primary.disabled = true;
  } else if (d.running) {
    s.notice.hidden = false;
    s.notice.textContent = "Relay is running. Setup will close it before it starts.";
  }
}

function confirmUninstall() {
  const s = mount("t-choose");
  s.title.textContent = "Uninstall Relay?";
  s.subtitle.textContent = `Relay ${installedVersion()} will be removed from this PC.`;
  s.card.append(
    row("Removes", el("span", { class: "value", text: "The app and its Python runtime" })),
    row("Keeps", el("span", { class: "value", text: "Your settings, models and browser profile" })),
    row("Location", pathValue(installedLocation())),
  );
  if (detection.running) {
    s.notice.hidden = false;
    s.notice.textContent = "Relay is running. Setup will close it first.";
  }
  setButton(s.primary, "Uninstall", () => start("uninstall"), "danger");
  setButton(s.tertiary, "Back", renderChoose);
}

// --- Progress -------------------------------------------------------------------

const WORDS = {
  install: { title: "Installing Relay", work: "Installing Relay", done: "is installed" },
  update: { title: "Updating Relay", work: "Updating Relay", done: "is ready" },
  repair: { title: "Repairing Relay", work: "Repairing files", done: "is repaired" },
  uninstall: { title: "Uninstalling Relay", work: "Removing Relay", done: "" },
};

function stepsFor(action) {
  const steps = [];
  if (detection.running) steps.push(["close", "Closing Relay"]);
  if (action !== "uninstall") steps.push(["prepare", "Unpacking the installer"]);
  steps.push(["permission", "Waiting for your OK", "Windows asks once for permission to change Program Files."]);
  steps.push(["work", WORDS[action].work]);
  steps.push(["finish", "Checking everything is in place"]);
  return steps;
}

function setStep(id, state) {
  const li = view.querySelector(`.steps li[data-id="${id}"]`);
  if (!li) return;
  li.className = state;
  const mark = li.querySelector(".mark");
  if (state === "active") mark.innerHTML = '<div class="spinner"></div>';
  else if (state === "done") mark.innerHTML = ICONS.stepDone;
  const hint = li.nextElementSibling;
  if (hint && hint.classList.contains("hint")) hint.hidden = state !== "active";
}

async function start(action) {
  busy = true;
  document.getElementById("tb-close").disabled = true;
  const s = mount("t-progress");
  const ship = detection.payload_version;
  s.title.textContent = action === "uninstall" ? WORDS.uninstall.title : `${WORDS[action].title} ${ship}`;
  s.subtitle.textContent =
    action === "uninstall" ? "This only takes a moment." : "This takes about a minute.";
  for (const [id, label, hint] of stepsFor(action)) {
    s.steps.append(
      el("li", { "data-id": id }, [
        el("span", { class: "mark" }, [el("span", { class: "dot" })]),
        el("span", { text: label }),
      ]),
    );
    if (hint) {
      const h = el("li", { class: "hint", text: hint });
      h.hidden = true;
      s.steps.append(h);
    }
  }

  let outcome;
  try {
    outcome = await bridge.invoke("apply", { action });
  } catch (err) {
    outcome = { ok: false, code: 0, message: String(err), version: null };
  }
  busy = false;
  document.getElementById("tb-close").disabled = false;
  renderDone(action, outcome);
}

// --- Done -----------------------------------------------------------------------

function renderDone(action, outcome) {
  const s = mount("t-done");
  if (outcome.ok) {
    s.badge.className = "badge ok";
    s.badge.innerHTML = ICONS.check;
    if (action === "uninstall") {
      s.title.textContent = "Relay was removed";
      s.subtitle.textContent =
        "Your settings, models and browser profile are still in your user folder, in case you come back.";
      s.secondary.hidden = true;
      setButton(s.primary, "Close", quit);
    } else {
      s.title.textContent = `Relay ${outcome.version || detection.payload_version} ${WORDS[action].done}`;
      s.subtitle.textContent = "Open Relay to pick up where you left off, or finish setting it up.";
      setButton(s.primary, "Open Relay", openRelay);
      setButton(s.secondary, "Close", quit);
    }
  } else {
    const declined = outcome.code === 1223;
    s.badge.className = "badge fail";
    s.badge.innerHTML = ICONS.cross;
    s.title.textContent = declined ? "Nothing was changed" : "Setup didn't finish";
    s.subtitle.textContent = outcome.message;
    if (outcome.code && !declined) {
      s.code.hidden = false;
      s.code.textContent = `Exit code ${outcome.code}`;
    }
    setButton(s.primary, "Try again", () => start(action));
    setButton(s.secondary, "Close", quit);
  }
}

// --- Window and app calls -------------------------------------------------------

async function openRelay() {
  try {
    await bridge.invoke("open_relay");
  } catch (err) {
    renderDone("install", { ok: false, code: 0, message: String(err) });
  }
}

function quit() {
  if (!busy) bridge.invoke("quit");
}

async function init() {
  document.getElementById("tb-min").onclick = () => bridge.minimize();
  document.getElementById("tb-close").onclick = quit;
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") quit();
  });
  mount("t-loading");
  await bridge.listen("setup-step", (e) => setStep(e.payload.id, e.payload.state));
  // Show the window once the page has painted in the right theme (no white flash).
  requestAnimationFrame(() => bridge.show());
  try {
    detection = await bridge.invoke("detect");
    renderChoose();
  } catch (err) {
    detection = { payload_version: "", installed: null, running: false };
    renderDone("install", { ok: false, code: 0, message: String(err) });
    document.querySelector('[data-slot="primary"]').onclick = () => location.reload();
  }
}

if (bridge) {
  init();
} else {
  view.textContent = "Relay Setup must be run as the Relay-Setup.exe app.";
}
