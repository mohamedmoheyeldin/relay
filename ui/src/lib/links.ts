// Links the backend hands the UI to fix or improve the setup (browser extension, Node.js, ...).
// They open in the default browser through the `open_url` command; the UI never navigates itself.
import { api, errorText } from "./backend";
import { useApp } from "../store/app";

const NODE_HOSTS = ["nodejs.org"];
const STORE_HOSTS = ["chromewebstore.google.com", "chrome.google.com", "microsoftedge.microsoft.com"];

function host(url: string): string {
  try {
    return new URL(url).hostname.toLowerCase();
  } catch {
    return "";
  }
}

/** Button text for a setup link the backend sent without a label. */
export function setupLinkLabel(url: string): string {
  const h = host(url);
  if (NODE_HOSTS.some((x) => h === x || h.endsWith(`.${x}`))) return "Get Node.js";
  if (STORE_HOSTS.includes(h)) return "Get the Playwright extension";
  return "Open the download page";
}

/** Only web links are opened. */
export function isWebUrl(url: string): boolean {
  try {
    const u = new URL(url);
    return u.protocol === "https:" || u.protocol === "http:";
  } catch {
    return false;
  }
}

export async function openExternal(url: string): Promise<void> {
  if (!isWebUrl(url)) {
    useApp.getState().toast("That link can't be opened.", "error");
    return;
  }
  try {
    await api.openUrl(url);
  } catch (e) {
    useApp.getState().toast(`Could not open the link: ${errorText(e)}`, "error");
  }
}
