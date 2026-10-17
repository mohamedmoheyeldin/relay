//! `relay-mcp`: Relay as an MCP server over stdio. See `cli.rs` for the options.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(relay_mcp_server::cli::run(&args));
}
