//! Command-line construction for Claude Code in headless mode.

use std::path::Path;

use crate::{RunRequest, SessionMode};

/// Arguments for one headless run. The prompt itself goes over stdin, never argv.
pub fn build_args(
    request: &RunRequest,
    mcp_config: &Path,
    system_prompt_file: Option<&Path>,
) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        // Ignore the user's own plugins, hooks and settings: Relay runs a controlled setup.
        // (`--safe-mode` would also drop the MCP servers we pass, so it is not used.)
        "--setting-sources",
        "local",
        "--disable-slash-commands",
        "--strict-mcp-config",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();

    args.push("--mcp-config".into());
    args.push(mcp_config.display().to_string());

    if let Some(tool) = &request.permission_prompt_tool {
        args.push("--permission-prompt-tool".into());
        args.push(tool.clone());
    }
    if !request.allowed_tools.is_empty() {
        args.push("--allowedTools".into());
        args.push(request.allowed_tools.join(","));
    }
    if let Some(path) = system_prompt_file {
        args.push("--append-system-prompt-file".into());
        args.push(path.display().to_string());
    }
    match &request.session {
        SessionMode::New(id) => {
            args.push("--session-id".into());
            args.push(id.clone());
        }
        SessionMode::Resume(id) => {
            args.push("--resume".into());
            args.push(id.clone());
        }
    }
    if let Some(model) = &request.model {
        args.push("--model".into());
        args.push(model.clone());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Timeouts;
    use std::path::PathBuf;

    fn request(session: SessionMode) -> RunRequest {
        RunRequest {
            prompt: "do it".into(),
            session,
            work_dir: PathBuf::from("."),
            mcp_servers: serde_json::json!({}),
            permission_prompt_tool: Some("mcp__relay__approve".into()),
            allowed_tools: vec!["mcp__relay".into(), "Read".into()],
            system_prompt_append: None,
            model: None,
            timeouts: Timeouts::default(),
        }
    }

    fn value_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        let i = args.iter().position(|a| a == flag)?;
        args.get(i + 1).map(String::as_str)
    }

    #[test]
    fn headless_streaming_flags_are_present() {
        let args = build_args(
            &request(SessionMode::New("abc".into())),
            Path::new("m.json"),
            None,
        );
        assert_eq!(args[0], "-p");
        assert_eq!(value_after(&args, "--output-format"), Some("stream-json"));
        assert!(args.contains(&"--verbose".to_string()));
        assert!(args.contains(&"--include-partial-messages".to_string()));
        assert!(args.contains(&"--strict-mcp-config".to_string()));
        assert_eq!(value_after(&args, "--setting-sources"), Some("local"));
        assert!(!args.contains(&"--safe-mode".to_string()));
        assert_eq!(value_after(&args, "--mcp-config"), Some("m.json"));
    }

    #[test]
    fn prompt_is_never_on_the_command_line() {
        let args = build_args(
            &request(SessionMode::New("abc".into())),
            Path::new("m.json"),
            None,
        );
        assert!(!args.iter().any(|a| a == "do it"));
    }

    #[test]
    fn new_and_resume_sessions_use_different_flags() {
        let new = build_args(
            &request(SessionMode::New("id-1".into())),
            Path::new("m"),
            None,
        );
        assert_eq!(value_after(&new, "--session-id"), Some("id-1"));
        assert!(!new.contains(&"--resume".to_string()));
        let resume = build_args(
            &request(SessionMode::Resume("id-1".into())),
            Path::new("m"),
            None,
        );
        assert_eq!(value_after(&resume, "--resume"), Some("id-1"));
        assert!(!resume.contains(&"--session-id".to_string()));
    }

    #[test]
    fn permission_tool_allowed_tools_prompt_and_model() {
        let mut r = request(SessionMode::New("x".into()));
        r.model = Some("opus".into());
        let args = build_args(&r, Path::new("m"), Some(Path::new("sys.md")));
        assert_eq!(
            value_after(&args, "--permission-prompt-tool"),
            Some("mcp__relay__approve")
        );
        assert_eq!(
            value_after(&args, "--allowedTools"),
            Some("mcp__relay,Read")
        );
        assert_eq!(
            value_after(&args, "--append-system-prompt-file"),
            Some("sys.md")
        );
        assert_eq!(value_after(&args, "--model"), Some("opus"));
    }

    #[test]
    fn optional_flags_are_omitted_when_unset() {
        let mut r = request(SessionMode::New("x".into()));
        r.permission_prompt_tool = None;
        r.allowed_tools.clear();
        let args = build_args(&r, Path::new("m"), None);
        for flag in [
            "--permission-prompt-tool",
            "--allowedTools",
            "--append-system-prompt-file",
            "--model",
        ] {
            assert!(!args.contains(&flag.to_string()), "{flag}");
        }
    }
}
