// No console window for the app itself in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `relay mcp-launch <server> -- <command>`: Cloud AI starts local MCP servers that
    // have secrets through this, so it must run before (and instead of) the app (D-047).
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some(relay_core::launcher::SUBCOMMAND) {
        let code = match relay_core::launcher::run(&args[1..]) {
            Ok(code) => code,
            Err(err) => {
                eprintln!("relay mcp-launch: {err:#}");
                1
            }
        };
        std::process::exit(code);
    }
    relay_ui_lib::run()
}
