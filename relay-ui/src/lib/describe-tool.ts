// Plain-words lines for Cloud AI's tool calls in the chat ("Searching Dice for ..."), so the
// chat reads like a conversation. The raw call stays one click away.

/** Tools that are Cloud AI's own bookkeeping, not work the owner cares about. */
const HIDDEN = new Set(["ToolSearch", "TodoWrite", "mcp__relay-screen__ping", "mcp__relay-screen__safety_status", "mcp__relay-screen__list_displays"]);

export const isHiddenTool = (name: string) => HIDDEN.has(name);

type Args = Record<string, unknown>;
type Line = [doing: string, done: string];

const parse = (input: string): Args => {
    try {
        const v = JSON.parse(input);
        return v && typeof v === "object" && !Array.isArray(v) ? v : {};
    } catch {
        return {};
    }
};

const str = (v: unknown) => (typeof v === "string" ? v : typeof v === "number" ? String(v) : "");
const clip = (s: string, n = 60) => (s.length > n ? `${s.slice(0, n - 1)}…` : s);
const quote = (s: string) => `“${clip(s)}”`;
const host = (url: string) => {
    try {
        return new URL(url).hostname.replace(/^www\./, "");
    } catch {
        return clip(url, 40);
    }
};
const file = (path: string) => path.split(/[\\/]/).pop() || path;
const words = (s: string) =>
    s
        .replace(/([a-z])([A-Z])/g, "$1 $2")
        .replace(/[_-]+/g, " ")
        .trim()
        .toLowerCase();
const title = (s: string) => s.replace(/\b\w/g, (c) => c.toUpperCase());
const pair = (doing: string, done: string, rest = ""): Line => [`${doing}${rest}`, `${done}${rest}`];

const screen = (tool: string, a: Args): Line => {
    switch (tool) {
        case "screenshot":
            return pair("Looking at", "Looked at", " the screen");
        case "zoom":
            return pair("Zooming in on", "Zoomed in on", " the screen");
        case "left_click":
        case "right_click":
        case "middle_click":
        case "double_click":
        case "triple_click":
        case "left_click_drag":
            return pair("Clicking", "Clicked", " on the screen");
        case "mouse_move":
            return pair("Moving", "Moved", " the mouse");
        case "type":
            return pair("Typing", "Typed", ` ${quote(str(a.text))}`);
        case "key":
        case "hold_key":
            return pair("Pressing", "Pressed", ` ${str(a.text)}`);
        case "scroll":
            return pair("Scrolling", "Scrolled", ` ${str(a.scroll_direction) || "the page"}`);
        case "wait":
            return pair("Waiting", "Waited", " for the screen to settle");
        case "find_text":
            return pair("Looking for", "Looked for", ` ${quote(str(a.text))} on the screen`);
        case "uia_tree":
        case "uia_find":
            return pair("Reading", "Read", " the window's controls");
        case "uia_invoke":
            return pair("Pressing", "Pressed", " a control");
        case "uia_set_value":
            return pair("Filling in", "Filled in", " a field");
        case "launch_app":
            return pair("Opening", "Opened", ` ${str(a.name) || str(a.app) || "an app"}`);
        case "list_apps":
            return pair("Checking", "Checked", " the installed apps");
        case "select_display":
            return pair("Switching", "Switched", " screens");
        case "ui_task":
            return pair("Local AI is working on:", "Local AI worked on:", ` ${clip(str(a.task), 80)}`);
        case "batch":
            return pair("Doing", "Did", ` ${Array.isArray(a.actions) ? a.actions.length : "several"} steps on the screen`);
        case "rearm":
            return pair("Asking", "Asked", " to turn Relay back on");
        default:
            return pair("Working", "Worked", " on the screen");
    }
};

const browser = (tool: string, a: Args): Line => {
    const el = str(a.element);
    switch (tool.replace(/^browser_/, "")) {
        case "navigate":
            return pair("Opening", "Opened", ` ${host(str(a.url))}`);
        case "navigate_back":
            return pair("Going", "Went", " back a page");
        case "snapshot":
        case "take_screenshot":
            return pair("Reading", "Read", " the page");
        case "click":
            return pair("Clicking", "Clicked", el ? ` ${quote(el)}` : " on the page");
        case "type":
        case "fill_form":
            return pair("Typing", "Typed", el ? ` into ${quote(el)}` : " on the page");
        case "select_option":
            return pair("Choosing", "Chose", el ? ` an option in ${quote(el)}` : " an option");
        case "press_key":
            return pair("Pressing", "Pressed", ` ${str(a.key)}`);
        case "tabs":
            return pair("Checking", "Checked", " the browser tabs");
        case "wait_for":
            return pair("Waiting", "Waited", " for the page");
        case "file_upload":
            return pair("Uploading", "Uploaded", " a file");
        default:
            return pair("Working", "Worked", " in the browser");
    }
};

// Any other connector: "mcp__cloud_ai_Dice__search_jobs" -> "Searching Dice for “...”".
const connector = (server: string, tool: string, a: Args): Line => {
    const service = title(words(server.replace(/^cloud_ai_/, "").replace(/^plugin_[^_]+_/, "")));
    const [verb, ...rest] = words(tool).split(" ");
    const thing = rest.join(" ");
    const query = str(a.keyword) || str(a.query) || str(a.q) || str(a.search) || str(a.text);
    switch (verb) {
        case "search":
        case "find":
            return query ? pair("Searching", "Searched", ` ${service} for ${quote(query)}`) : pair("Searching", "Searched", ` ${service}`);
        case "get":
        case "read":
        case "fetch":
        case "download":
            return pair("Reading", "Read", ` ${thing || "details"} from ${service}`);
        case "list":
            return pair("Listing", "Listed", ` ${thing || "items"} in ${service}`);
        case "create":
            return pair("Creating", "Created", ` ${thing ? `a ${thing}` : "something"} in ${service}`);
        case "update":
            return pair("Updating", "Updated", ` ${thing || "something"} in ${service}`);
        case "send":
            return pair("Sending", "Sent", ` ${thing ? `a ${thing}` : "it"} with ${service}`);
        case "delete":
        case "trash":
            return pair("Deleting", "Deleted", ` ${thing || "something"} in ${service}`);
        default:
            return pair("Using", "Used", ` ${service}${thing || verb ? `: ${[verb, thing].filter(Boolean).join(" ")}` : ""}`);
    }
};

const builtin = (name: string, a: Args): Line => {
    const path = str(a.file_path) || str(a.path);
    switch (name) {
        case "Bash":
        case "PowerShell":
            return str(a.description) ? pair("", "", str(a.description)) : pair("Running", "Ran", " a command");
        case "WebFetch":
            return pair("Reading", "Read", ` ${host(str(a.url))}`);
        case "WebSearch":
            return pair("Searching", "Searched", ` the web for ${quote(str(a.query))}`);
        case "Read":
            return pair("Reading", "Read", ` ${file(path) || "a file"}`);
        case "Write":
            return pair("Saving", "Saved", ` ${file(path) || "a file"}`);
        case "Edit":
        case "MultiEdit":
            return pair("Editing", "Edited", ` ${file(path) || "a file"}`);
        case "Glob":
        case "Grep":
            return pair("Searching", "Searched", " files");
        case "Task":
        case "Agent":
            return pair("Working on", "Worked on", ` ${clip(str(a.description) || "a side task")}`);
        default:
            return pair("Using", "Used", ` ${words(name)}`);
    }
};

/** One readable line for a tool call; `done` picks the past tense. */
export const describeTool = (name: string, input: string, done: boolean): string => {
    const a = parse(input);
    let line: Line;
    const mcp = name.match(/^mcp__(.+?)__(.+)$/);
    if (!mcp) line = builtin(name, a);
    else if (mcp[1] === "relay-screen") line = screen(mcp[2], a);
    else if (mcp[1] === "relay-browser" || mcp[1] === "playwright" || mcp[1].endsWith("_playwright")) line = browser(mcp[2], a);
    else if (/(^|_)(PowerShell|start_process)$/.test(mcp[2])) line = builtin("PowerShell", a);
    else line = connector(mcp[1], mcp[2], a);
    const text = (done ? line[1] : line[0]).trim();
    return text.charAt(0).toUpperCase() + text.slice(1);
};
