//! `relay mcp-launch <server> -- <command> [args...]`
//!
//! Claude Code starts local MCP servers that have secrets through this launcher (D-047).
//! It reads the server's secret names from settings, fetches the values from
//! Credential Manager, and runs the real command with them as environment variables,
//! passing stdin and stdout straight through. Claude Code's config only ever holds the
//! launcher command line.

use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use crate::{secrets, settings::Settings};

pub const SUBCOMMAND: &str = "mcp-launch";

#[derive(Debug, PartialEq)]
pub struct LaunchArgs {
    pub server: String,
    pub command: String,
    pub args: Vec<String>,
}

/// Parse the arguments that follow `mcp-launch`.
pub fn parse(args: &[String]) -> Result<LaunchArgs> {
    let Some((server, rest)) = args.split_first() else {
        bail!("usage: relay mcp-launch <server> -- <command> [args...]");
    };
    let rest = match rest.split_first() {
        Some((sep, rest)) if sep == "--" => rest,
        _ => rest,
    };
    let Some((command, args)) = rest.split_first() else {
        bail!("mcp-launch needs a command to run");
    };
    Ok(LaunchArgs {
        server: server.clone(),
        command: command.clone(),
        args: args.to_vec(),
    })
}

/// On Windows `npx` and friends are `.cmd` scripts, which need `cmd /C`.
fn command_for(program: &str) -> Command {
    if cfg!(windows) && std::path::Path::new(program).extension().is_none() {
        if let Some(paths) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&paths) {
                if dir.join(format!("{program}.exe")).is_file() {
                    return Command::new(dir.join(format!("{program}.exe")));
                }
                for ext in ["cmd", "bat"] {
                    let script = dir.join(format!("{program}.{ext}"));
                    if script.is_file() {
                        let mut cmd = Command::new("cmd");
                        cmd.arg("/C").arg(script);
                        return cmd;
                    }
                }
            }
        }
    }
    Command::new(program)
}

/// Run the server and return its exit code.
pub fn run(args: &[String]) -> Result<i32> {
    let launch = parse(args)?;
    let settings = Settings::load().unwrap_or_default();
    let mut cmd = command_for(&launch.command);
    cmd.args(&launch.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if let Some(names) = settings.connections.secret_names.get(&launch.server) {
        for name in names {
            let value = secrets::get(&launch.server, name)
                .with_context(|| format!("secret {name} for {} is missing", launch.server))?;
            cmd.env(name, value);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let status = cmd
        .status()
        .with_context(|| format!("could not start {}", launch.command))?;
    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn parses_with_separator() {
        let got = parse(&s(&["brave", "--", "npx", "-y", "pkg"])).unwrap();
        assert_eq!(
            got,
            LaunchArgs {
                server: "brave".into(),
                command: "npx".into(),
                args: s(&["-y", "pkg"]),
            }
        );
    }

    #[test]
    fn rejects_missing_command() {
        assert!(parse(&s(&["brave", "--"])).is_err());
        assert!(parse(&[]).is_err());
    }
}
