//! The MCP hub (D-047): one list of MCP servers shared by Cloud AI and Relay.
//!
//! The hub keeps no copy of the list. It reads and writes Cloud AI's own config
//! through `cloud-cli mcp list/get/add/remove` at user scope. Paused servers are the one
//! exception: they are removed from Cloud AI and remembered in Relay's settings so
//! they can be put back unchanged.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::{cli_names, cloud, proc, secrets};

/// Relay's own MCP server, registered by Relay and shown as built in.
pub const RELAY_SCREEN: &str = "relay-screen";
/// Relay's browser lane: Playwright MCP driving the owner's Chromium browser.
pub const RELAY_BROWSER: &str = "relay-browser";
pub const BUILTIN_SERVERS: &[&str] = &[RELAY_SCREEN, RELAY_BROWSER];

pub fn is_builtin(name: &str) -> bool {
    BUILTIN_SERVERS.contains(&name)
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Relay's own servers (relay-screen, later the browser bridge).
    Builtin,
    /// Connectors from the owner's cloud.ai account (Gmail, Calendar, ...).
    CloudAi,
    /// Servers that come with a Cloud AI plugin.
    Plugin,
    /// Servers added by hand or from Relay's catalog.
    Local,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Connected,
    NeedsAuth,
    Failed,
    Disabled,
    NotConfigured,
    Paused,
    Unknown,
}

/// One row of the Connections page.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Entry {
    pub name: String,
    pub kind: Kind,
    pub target: String,
    pub transport: String,
    pub status: Status,
    pub status_text: String,
    /// For plugin servers: the plugin that brings them ("productivity@synced").
    pub plugin: Option<String>,
}

fn classify(name: &str) -> Kind {
    if is_builtin(name) {
        Kind::Builtin
    } else if name.starts_with(cli_names::ACCOUNT_CONNECTOR_PREFIX) {
        Kind::CloudAi
    } else if name.starts_with("plugin:") {
        Kind::Plugin
    } else {
        Kind::Local
    }
}

fn parse_status(text: &str) -> Status {
    let t = text.to_lowercase();
    if t.contains("connected") && !t.contains("failed") {
        Status::Connected
    } else if t.contains("needs authentication") {
        Status::NeedsAuth
    } else if t.contains("failed") {
        Status::Failed
    } else if t.contains("disabled") {
        Status::Disabled
    } else if t.contains("not configured") {
        Status::NotConfigured
    } else {
        Status::Unknown
    }
}

/// Parse `cloud-cli mcp list`. Each server line reads `<name>: <target> - <status>`;
/// names can contain colons (`plugin:a:b`) but not ": ".
pub fn parse_list(text: &str) -> Vec<Entry> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let (name, rest) = line.split_once(": ")?;
            let (target, status_text) = match rest.rsplit_once(" - ") {
                Some((t, s)) => (t.trim(), s.trim()),
                None => return None,
            };
            let (target, transport) = if let Some(t) = target.strip_suffix("(HTTP)") {
                (t.trim(), "http")
            } else if let Some(t) = target.strip_suffix("(SSE)") {
                (t.trim(), "sse")
            } else if target.starts_with("http://") || target.starts_with("https://") {
                (target, "http")
            } else {
                (target, "stdio")
            };
            // Drop the leading status glyph ("√ Connected", "! Needs authentication").
            let status_text = status_text
                .trim_start_matches(|c: char| !c.is_alphanumeric())
                .to_string();
            Some(Entry {
                name: name.trim().to_string(),
                kind: classify(name.trim()),
                target: target.to_string(),
                transport: transport.to_string(),
                status: parse_status(&status_text),
                status_text,
                plugin: None,
            })
        })
        .collect()
}

/// A server as the hub adds it. Stored as-is for paused servers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum ServerSpec {
    Stdio {
        name: String,
        command: String,
        #[serde(default)]
        args: Vec<String>,
        /// Plain (non-secret) environment variables.
        #[serde(default)]
        env: BTreeMap<String, String>,
    },
    Http {
        name: String,
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
    },
    Sse {
        name: String,
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
    },
}

impl ServerSpec {
    pub fn name(&self) -> &str {
        match self {
            Self::Stdio { name, .. } | Self::Http { name, .. } | Self::Sse { name, .. } => name,
        }
    }

    /// The JSON `cloud-cli mcp add-json` takes. With `launcher`, a stdio server is
    /// started through Relay's launcher so its secrets reach it from Credential
    /// Manager as environment variables and never sit in Cloud AI's config.
    pub fn config_json(&self, launcher: Option<&Path>) -> serde_json::Value {
        match self {
            Self::Stdio {
                name,
                command,
                args,
                env,
            } => {
                let (command, args) = match launcher {
                    Some(launcher) => {
                        let mut full = vec![
                            crate::launcher::SUBCOMMAND.to_string(),
                            name.clone(),
                            "--".into(),
                            command.clone(),
                        ];
                        full.extend(args.iter().cloned());
                        (launcher.to_string_lossy().into_owned(), full)
                    }
                    None => (command.clone(), args.clone()),
                };
                serde_json::json!({ "type": "stdio", "command": command, "args": args, "env": env })
            }
            Self::Http { url, headers, .. } => {
                serde_json::json!({ "type": "http", "url": url, "headers": headers })
            }
            Self::Sse { url, headers, .. } => {
                serde_json::json!({ "type": "sse", "url": url, "headers": headers })
            }
        }
    }
}

/// A server paused in Relay: its exact Cloud AI config, put back on resume.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PausedServer {
    pub name: String,
    pub config: serde_json::Value,
}

/// Cloud AI's user-scope config for one server, read from `~/.cloud.json`.
/// Only the `mcpServers` entry is read; nothing else in the file is touched.
pub fn user_config(name: &str) -> Result<serde_json::Value> {
    let path = dirs::home_dir()
        .context("no home folder")?
        .join(cli_names::USER_CONFIG);
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("could not read {}", path.display()))?;
    let root: serde_json::Value = serde_json::from_str(&text)?;
    root.get("mcpServers")
        .and_then(|m| m.get(name))
        .cloned()
        .with_context(|| format!("{name} is not a user-scope server, so Relay cannot pause it"))
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        bail!("a server name may only use letters, digits, '-', '_' and '.'");
    }
    Ok(())
}

async fn cloud_mcp(args: &[String]) -> Result<proc::Output> {
    let cloud = cloud::find().context("Cloud AI is not installed")?;
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    proc::run(&cloud, &refs).await
}

/// Every server Cloud AI sees, plus paused ones from Relay's settings.
pub async fn list(paused: &[PausedServer]) -> Result<Vec<Entry>> {
    let args = ["mcp".to_string(), "list".to_string()];
    let (out, plugins) = tokio::join!(cloud_mcp(&args), plugins());
    let mut entries = parse_list(&out?.stdout);
    let plugins = plugins.unwrap_or_default();
    for e in entries.iter_mut().filter(|e| e.kind == Kind::Plugin) {
        e.plugin = plugin_name(&e.name)
            .and_then(|p| plugins.iter().find(|x| x.name() == p))
            .map(|p| p.id.clone());
    }
    for p in paused {
        if entries.iter().any(|e| e.name == p.name) {
            continue;
        }
        let field = |k: &str| p.config.get(k).and_then(|v| v.as_str()).unwrap_or_default();
        let transport = match field("type") {
            "" => "stdio",
            t => t,
        };
        let target = if transport == "stdio" {
            let args: Vec<&str> = p
                .config
                .get("args")
                .and_then(|a| a.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
                .unwrap_or_default();
            format!("{} {}", field("command"), args.join(" "))
                .trim()
                .to_string()
        } else {
            field("url").to_string()
        };
        entries.push(Entry {
            name: p.name.clone(),
            kind: classify(&p.name),
            target,
            transport: transport.into(),
            status: Status::Paused,
            status_text: "Paused in Relay".into(),
            plugin: None,
        });
    }
    Ok(entries)
}

/// The plugin part of a plugin server's name: "plugin:productivity:slack" -> "productivity".
pub fn plugin_name(server: &str) -> Option<&str> {
    server.strip_prefix("plugin:")?.split(':').next()
}

/// A Cloud AI plugin, from `cloud-cli plugin list --json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Plugin {
    /// "name@marketplace".
    pub id: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub enabled: bool,
}

impl Plugin {
    pub fn name(&self) -> &str {
        self.id.split('@').next().unwrap_or(&self.id)
    }
}

pub async fn plugins() -> Result<Vec<Plugin>> {
    let out = cloud_mcp(&["plugin".into(), "list".into(), "--json".into()]).await?;
    if !out.ok {
        bail!("Cloud AI CLI plugin list failed: {}", out.stderr.trim());
    }
    Ok(serde_json::from_str(out.stdout.trim())?)
}

/// `cloud-cli plugin enable|disable <id>`: turns every server, skill and command of
/// the plugin on or off together.
pub async fn set_plugin_enabled(id: &str, enabled: bool) -> Result<()> {
    let verb = if enabled { "enable" } else { "disable" };
    let out = cloud_mcp(&["plugin".into(), verb.into(), id.into()]).await?;
    if !out.ok {
        bail!(
            "Cloud AI CLI plugin {verb} failed: {}{}",
            out.stderr.trim(),
            out.stdout.trim()
        );
    }
    Ok(())
}

/// `cloud-cli mcp login <name>`: Cloud AI opens the browser for the server's own
/// sign-in and keeps the token itself. Relay only waits for it to finish.
pub async fn login(name: &str, on_line: impl Fn(String) + Send + Sync + 'static) -> Result<()> {
    let cloud = cloud::find().context("Cloud AI is not installed")?;
    let mut cmd = proc::command(&cloud);
    cmd.current_dir(crate::paths::work_dir())
        .args(["mcp", "login", name]);
    let run = proc::run_streaming(cmd, on_line);
    let out = tokio::time::timeout(std::time::Duration::from_secs(600), run)
        .await
        .context("the sign-in was not finished within 10 minutes")??;
    if !out.ok {
        bail!(
            "sign-in did not finish: {}",
            last_line(&out.stderr)
                .or(last_line(&out.stdout))
                .unwrap_or("unknown error")
        );
    }
    Ok(())
}

pub async fn logout(name: &str) -> Result<()> {
    let out = cloud_mcp(&["mcp".into(), "logout".into(), name.into()]).await?;
    if !out.ok {
        bail!("Cloud AI CLI mcp logout failed: {}", out.stderr.trim());
    }
    Ok(())
}

fn last_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).rfind(|l| !l.is_empty())
}

/// `cloud-cli mcp get <name>`: the full details as Cloud AI prints them.
pub async fn get(name: &str) -> Result<String> {
    let out = cloud_mcp(&["mcp".into(), "get".into(), name.into()]).await?;
    if !out.ok {
        bail!("{}", out.stderr.trim());
    }
    Ok(out.stdout)
}

/// Add a server at user scope. `secrets` go to Credential Manager.
pub async fn add(
    spec: &ServerSpec,
    secret_env: &[(String, String)],
    launcher: &Path,
) -> Result<()> {
    validate_name(spec.name())?;
    if !secret_env.is_empty() && !matches!(spec, ServerSpec::Stdio { .. }) {
        bail!("secrets can only be passed to local (stdio) servers; HTTP servers sign in through Cloud AI");
    }
    for (key, value) in secret_env {
        secrets::set(spec.name(), key, value)?;
    }
    let use_launcher = (!secret_env.is_empty()).then_some(launcher);
    add_json(spec.name(), &spec.config_json(use_launcher)).await
}

/// `cloud-cli mcp add-json --scope user <name> <json>`.
pub async fn add_json(name: &str, config: &serde_json::Value) -> Result<()> {
    let out = cloud_mcp(&[
        "mcp".into(),
        "add-json".into(),
        "--scope".into(),
        "user".into(),
        name.into(),
        config.to_string(),
    ])
    .await?;
    if !out.ok {
        bail!(
            "Cloud AI CLI mcp add-json failed: {}{}",
            out.stderr.trim(),
            out.stdout.trim()
        );
    }
    Ok(())
}

/// Remove a server from Cloud AI's user config. Secrets are deleted with it unless
/// the server is only being paused.
pub async fn remove(name: &str, secret_names: &[String], keep_secrets: bool) -> Result<()> {
    let out = cloud_mcp(&[
        "mcp".into(),
        "remove".into(),
        "--scope".into(),
        "user".into(),
        name.into(),
    ])
    .await?;
    if !out.ok {
        bail!("Cloud AI CLI mcp remove failed: {}", out.stderr.trim());
    }
    if !keep_secrets {
        for key in secret_names {
            let _ = secrets::delete(name, key);
        }
    }
    Ok(())
}

/// Whether Cloud AI has a server with this name (any scope).
pub async fn exists(name: &str) -> bool {
    cloud_mcp(&["mcp".into(), "get".into(), name.into()])
        .await
        .map(|o| o.ok)
        .unwrap_or(false)
}

/// A short catalog of well-known local servers for the "Add" dialog.
#[derive(Debug, Clone, Serialize)]
pub struct CatalogServer {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub transport: &'static str,
    pub command: &'static str,
    pub args: &'static [&'static str],
    pub url: &'static str,
    pub secret_env: &'static [&'static str],
}

pub fn catalog() -> Vec<CatalogServer> {
    vec![
        CatalogServer {
            id: "filesystem",
            name: "Filesystem",
            description: "Read and write files in folders you choose.",
            transport: "stdio",
            command: "npx",
            args: &["-y", "@modelcontextprotocol/server-filesystem"],
            url: "",
            secret_env: &[],
        },
        CatalogServer {
            id: "playwright",
            name: "Playwright",
            description: "Drive a separate browser for web tasks.",
            transport: "stdio",
            command: "npx",
            args: &["-y", "@playwright/mcp@latest"],
            url: "",
            secret_env: &[],
        },
        CatalogServer {
            id: "github",
            name: "GitHub",
            description: "Issues, pull requests and code on GitHub.",
            transport: "http",
            command: "",
            args: &[],
            url: "https://api.githubcopilot.com/mcp/",
            secret_env: &[],
        },
        CatalogServer {
            id: "context7",
            name: "Context7",
            description: "Current documentation for libraries and tools.",
            transport: "http",
            command: "",
            args: &[],
            url: "https://mcp.context7.com/mcp",
            secret_env: &[],
        },
        CatalogServer {
            id: "brave-search",
            name: "Brave Search",
            description: "Web search through the Brave Search API.",
            transport: "stdio",
            command: "npx",
            args: &["-y", "@brave/brave-search-mcp-server"],
            url: "",
            secret_env: &["BRAVE_API_KEY"],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(name: &str) -> String {
        format!("{}{name}", cli_names::ACCOUNT_CONNECTOR_PREFIX)
    }

    const SAMPLE: &str = "Checking MCP server health…\n\n\
{acct}Gmail: https://gmailmcp.googleapis.com/mcp/v1 - √ Connected\n\
{acct}Uber Eats: https://mcp.ubereats.com/eats/mcp - ⊘ Disabled for this project (re-enable via /mcp)\n\
{acct}Indeed: https://mcp.indeed.com/jobs/mcp - × Failed to connect — -32429: Rate limit exceeded\n\
plugin:cloudflare:cloudflare: https://mcp.cloudflare.com/mcp (HTTP) - ! Needs authentication\n\
plugin:playwright:playwright: npx @playwright/mcp@latest - √ Connected\n\
plugin:productivity:gmail:  (HTTP) - - Not configured\n\
context7: https://mcp.context7.com/mcp (HTTP) - √ Connected\n\
relay-screen: C:\\Program Files\\Relay\\python\\pythonw.exe -m relay_screen.server - √ Connected\n";

    #[test]
    fn plugin_names_and_ids() {
        assert_eq!(
            plugin_name("plugin:productivity:slack"),
            Some("productivity")
        );
        assert_eq!(plugin_name("plugin:design:google calendar"), Some("design"));
        assert_eq!(plugin_name(&account("Gmail")), None);
        let list: Vec<Plugin> = serde_json::from_str(
            r#"[{"id":"productivity@synced","version":"1.0.0","scope":"synced","enabled":true,"installPath":"x"}]"#,
        )
        .unwrap();
        assert_eq!(list[0].name(), "productivity");
        assert!(list[0].enabled);
    }

    #[test]
    fn parses_cloud_mcp_list() {
        let entries = parse_list(&SAMPLE.replace("{acct}", cli_names::ACCOUNT_CONNECTOR_PREFIX));
        assert_eq!(entries.len(), 8);
        let gmail = &entries[0];
        assert_eq!(gmail.name, account("Gmail"));
        assert_eq!(gmail.kind, Kind::CloudAi);
        assert_eq!(gmail.status, Status::Connected);
        assert_eq!(gmail.transport, "http");
        assert_eq!(entries[1].status, Status::Disabled);
        assert_eq!(entries[2].status, Status::Failed);
        let cf = &entries[3];
        assert_eq!(cf.name, "plugin:cloudflare:cloudflare");
        assert_eq!(cf.kind, Kind::Plugin);
        assert_eq!(cf.status, Status::NeedsAuth);
        assert_eq!(cf.target, "https://mcp.cloudflare.com/mcp");
        assert_eq!(entries[4].transport, "stdio");
        assert_eq!(entries[5].status, Status::NotConfigured);
        assert_eq!(entries[6].kind, Kind::Local);
        let relay = &entries[7];
        assert_eq!(relay.kind, Kind::Builtin);
        assert!(relay.target.contains("pythonw.exe"));
    }

    #[test]
    fn stdio_json_with_launcher() {
        let spec = ServerSpec::Stdio {
            name: "brave".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "pkg".into()],
            env: BTreeMap::from([("MODE".into(), "x".into())]),
        };
        let json = spec.config_json(Some(Path::new("C:/Program Files/Relay/relay.exe")));
        assert_eq!(
            json,
            serde_json::json!({
                "type": "stdio",
                "command": "C:/Program Files/Relay/relay.exe",
                "args": ["mcp-launch", "brave", "--", "npx", "-y", "pkg"],
                "env": {"MODE": "x"}
            })
        );
        let plain = spec.config_json(None);
        assert_eq!(plain["command"], "npx");
    }

    #[test]
    fn http_json() {
        let spec = ServerSpec::Http {
            name: "ctx".into(),
            url: "https://example.com/mcp".into(),
            headers: BTreeMap::new(),
        };
        assert_eq!(
            spec.config_json(None),
            serde_json::json!({"type": "http", "url": "https://example.com/mcp", "headers": {}})
        );
    }

    #[test]
    fn spec_json_round_trip() {
        let spec = ServerSpec::Sse {
            name: "s".into(),
            url: "https://e/sse".into(),
            headers: BTreeMap::new(),
        };
        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains("\"transport\":\"sse\""));
        assert_eq!(serde_json::from_str::<ServerSpec>(&json).unwrap(), spec);
    }

    #[test]
    fn names_are_validated() {
        assert!(validate_name("my-server_1.0").is_ok());
        assert!(validate_name("bad name").is_err());
        assert!(validate_name("").is_err());
    }
}
