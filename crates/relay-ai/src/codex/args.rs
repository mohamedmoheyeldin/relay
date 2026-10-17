//! Command-line construction for `codex exec` in non-interactive JSON mode.

use std::path::Path;

use crate::{RunRequest, SessionMode};

/// Environment variable that carries the MCP bearer token (never put it on the command line).
pub const MCP_TOKEN_ENV: &str = "RELAY_MCP_TOKEN";

/// Quote a string as a TOML basic string for `-c key="value"`.
pub fn toml_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The `url` and bearer token of Relay's own MCP server, if the request carries one.
pub fn relay_mcp(request: &RunRequest) -> Option<(String, Option<String>)> {
    let entry = request.mcp_servers.get("relay")?;
    let url = entry.get("url")?.as_str()?.to_string();
    let token = entry
        .get("headers")
        .and_then(|h| h.get("Authorization"))
        .and_then(|v| v.as_str())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string);
    Some((url, token))
}

/// Arguments for one run. The prompt goes over stdin (`-`), never argv.
pub fn build_args(request: &RunRequest, schema_file: Option<&Path>) -> Vec<String> {
    let mut args: Vec<String> = ["exec"].iter().map(|s| (*s).to_string()).collect();
    // Flags first, then the optional `resume <id>` subcommand, then the stdin marker.
    for flag in ["--json", "--skip-git-repo-check", "--color", "never"] {
        args.push(flag.into());
    }
    args.push("--sandbox".into());
    // Plans are proposals: Codex may read but never change anything. Agent runs may write inside
    // the working folder only.
    args.push(
        if request.plan_only {
            "read-only"
        } else {
            "workspace-write"
        }
        .into(),
    );
    args.push("-c".into());
    args.push(format!("approval_policy={}", toml_string("never")));
    if request.plan_only {
        args.push("--ephemeral".into());
        args.push("--ignore-user-config".into());
    }
    args.push("-C".into());
    args.push(request.work_dir.display().to_string());
    if let Some(model) = &request.model {
        args.push("-m".into());
        args.push(model.clone());
    }
    if let Some(effort) = &request.reasoning {
        args.push("-c".into());
        args.push(format!("model_reasoning_effort={}", toml_string(effort)));
    }
    if let Some(path) = schema_file {
        args.push("--output-schema".into());
        args.push(path.display().to_string());
    }
    if !request.plan_only {
        if let Some((url, token)) = relay_mcp(request) {
            for (key, value) in [
                ("url", toml_string(&url)),
                ("required", "true".to_string()),
                ("startup_timeout_sec", "20".to_string()),
            ] {
                args.push("-c".into());
                args.push(format!("mcp_servers.relay.{key}={value}"));
            }
            if token.is_some() {
                args.push("-c".into());
                args.push(format!(
                    "mcp_servers.relay.bearer_token_env_var={}",
                    toml_string(MCP_TOKEN_ENV)
                ));
            }
        }
    }
    if let SessionMode::Resume(id) = &request.session {
        args.push("resume".into());
        args.push(id.clone());
    }
    args.push("-".into());
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Timeouts;
    use serde_json::json;
    use std::path::PathBuf;

    fn request() -> RunRequest {
        RunRequest {
            run_id: 1,
            prompt: "do it".into(),
            session: SessionMode::New("ignored".into()),
            work_dir: PathBuf::from(r"C:\work"),
            mcp_servers: json!({"relay": {"type":"http","url":"http://127.0.0.1:5000/mcp","headers":{"Authorization":"Bearer tok123"}}}),
            permission_prompt_tool: None,
            allowed_tools: vec![],
            disallowed_tools: vec![],
            system_prompt_append: None,
            model: None,
            reasoning: None,
            output_schema: None,
            plan_only: false,
            timeouts: Timeouts::default(),
        }
    }

    fn has(args: &[String], pair: [&str; 2]) -> bool {
        args.windows(2).any(|w| w[0] == pair[0] && w[1] == pair[1])
    }

    #[test]
    fn the_prompt_is_read_from_stdin_and_output_is_json() {
        let a = build_args(&request(), None);
        assert_eq!(a[0], "exec");
        assert!(a.contains(&"--json".to_string()));
        assert_eq!(a.last().map(String::as_str), Some("-"));
        assert!(!a.iter().any(|x| x == "do it"));
        assert!(has(&a, ["-C", r"C:\work"]));
    }

    #[test]
    fn plan_only_runs_are_read_only_and_isolated() {
        let mut r = request();
        r.plan_only = true;
        let a = build_args(&r, Some(Path::new("schema.json")));
        assert!(has(&a, ["--sandbox", "read-only"]));
        assert!(
            a.contains(&"--ephemeral".to_string())
                && a.contains(&"--ignore-user-config".to_string())
        );
        assert!(has(&a, ["--output-schema", "schema.json"]));
        assert!(
            !a.iter().any(|x| x.contains("mcp_servers")),
            "no tools while planning"
        );
        assert!(!a.iter().any(|x| x.contains("dangerously") || x == "--yolo"));
    }

    #[test]
    fn agent_runs_write_only_inside_the_workspace_and_attach_relay_tools() {
        let a = build_args(&request(), None);
        assert!(has(&a, ["--sandbox", "workspace-write"]));
        assert!(a.contains(&r#"mcp_servers.relay.url="http://127.0.0.1:5000/mcp""#.to_string()));
        assert!(a.contains(&"mcp_servers.relay.required=true".to_string()));
        assert!(
            a.contains(&r#"mcp_servers.relay.bearer_token_env_var="RELAY_MCP_TOKEN""#.to_string())
        );
        assert!(
            !a.iter().any(|x| x.contains("tok123")),
            "the token never appears in argv"
        );
        assert!(a.contains(&r#"approval_policy="never""#.to_string()));
    }

    #[test]
    fn model_reasoning_and_resume() {
        let mut r = request();
        r.model = Some("gpt-5".into());
        r.reasoning = Some("high".into());
        r.session = SessionMode::Resume("thread-1".into());
        let a = build_args(&r, None);
        assert!(has(&a, ["-m", "gpt-5"]));
        assert!(a.contains(&r#"model_reasoning_effort="high""#.to_string()));
        let n = a.len();
        assert_eq!(&a[n - 3..], ["resume", "thread-1", "-"]);
    }

    #[test]
    fn toml_strings_are_escaped() {
        assert_eq!(toml_string(r#"a"b\c"#), r#""a\"b\\c""#);
        assert_eq!(toml_string("x\ny"), r#""x\ny""#);
    }

    #[test]
    fn relay_mcp_is_extracted() {
        let (url, token) = relay_mcp(&request()).unwrap();
        assert_eq!(url, "http://127.0.0.1:5000/mcp");
        assert_eq!(token.as_deref(), Some("tok123"));
        let mut none = request();
        none.mcp_servers = json!({});
        assert!(relay_mcp(&none).is_none());
    }
}
