//! Relay Setup: installs, updates, repairs or removes Relay with Relay's look. The
//! stock NSIS installer rides inside as the payload that does the actual install.

// No console window, ever: this is what the owner double-clicks.
#![cfg_attr(windows, windows_subsystem = "windows")]

// Off Windows only the tests use it.
#[cfg_attr(not(windows), allow(dead_code))]
mod state;

#[cfg(windows)]
mod app;
#[cfg(all(windows, target_env = "gnu"))]
mod webview2_loader;
#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The elevated half of Uninstall: pairs of <uninstaller> <install dir>.
    if args.first().map(String::as_str) == Some("--uninstall-elevated") {
        let mut code = 0;
        for pair in args[1..].chunks(2) {
            if let [uninstaller, dir] = pair {
                let c = win::uninstall_in_place(uninstaller, dir);
                if code == 0 {
                    code = c;
                }
            }
        }
        std::process::exit(code as i32);
    }
    app::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Relay Setup runs on Windows.");
    std::process::exit(1);
}
