//! Minimal client for a local Ollama server (HTTP on loopback).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, thiserror::Error)]
pub enum OllamaError {
    #[error("the local AI server is not running at {0}")]
    Unreachable(String),
    #[error("local AI server error: {0}")]
    Server(String),
}

/// What the local AI server knows about a model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub size_bytes: u64,
    pub vision: bool,
    pub tools: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCall {
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(default)]
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

impl ChatMessage {
    pub fn new(role: &str, content: impl Into<String>) -> Self {
        Self {
            role: role.to_string(),
            content: content.into(),
            ..Self::default()
        }
    }

    pub fn tool_result(name: &str, content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: content.into(),
            tool_name: Some(name.to_string()),
            ..Self::default()
        }
    }

    pub fn with_images(mut self, images: Vec<String>) -> Self {
        self.images = Some(images);
        self
    }
}

/// Progress of a model download.
#[derive(Debug, Clone, PartialEq)]
pub struct PullProgress {
    pub status: String,
    pub completed: u64,
    pub total: u64,
}

impl PullProgress {
    pub fn percent(&self) -> Option<u8> {
        (self.total > 0).then(|| ((self.completed.saturating_mul(100)) / self.total).min(100) as u8)
    }
}

#[derive(Clone)]
pub struct OllamaClient {
    base: String,
    http: reqwest::Client,
}

fn parse_models(tags: &Value) -> Vec<(String, u64)> {
    tags.get("models")
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter_map(|m| {
                    let name = m.get("name").and_then(Value::as_str)?.to_string();
                    let size = m.get("size").and_then(Value::as_u64).unwrap_or_default();
                    Some((name, size))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn capabilities(show: &Value) -> (bool, bool) {
    let caps: Vec<&str> = show
        .get("capabilities")
        .and_then(Value::as_array)
        .map(|c| c.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    (caps.contains(&"vision"), caps.contains(&"tools"))
}

impl OllamaClient {
    pub fn new(base: impl Into<String>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(2))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    fn unreachable(&self, _e: reqwest::Error) -> OllamaError {
        OllamaError::Unreachable(self.base.clone())
    }

    /// Whether a server answers at the configured address.
    pub async fn available(&self) -> bool {
        self.http
            .get(format!("{}/api/tags", self.base))
            .timeout(Duration::from_secs(3))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// Installed models with their capabilities.
    pub async fn models(&self) -> Result<Vec<ModelInfo>, OllamaError> {
        let tags: Value = self
            .http
            .get(format!("{}/api/tags", self.base))
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| self.unreachable(e))?
            .json()
            .await
            .map_err(|e| OllamaError::Server(e.to_string()))?;
        let mut out = Vec::new();
        for (name, size_bytes) in parse_models(&tags) {
            let show: Value = match self
                .http
                .post(format!("{}/api/show", self.base))
                .json(&json!({ "model": name }))
                .timeout(Duration::from_secs(10))
                .send()
                .await
            {
                Ok(resp) => resp.json().await.unwrap_or(Value::Null),
                Err(_) => Value::Null,
            };
            let (vision, tools) = capabilities(&show);
            out.push(ModelInfo {
                name,
                size_bytes,
                vision,
                tools,
            });
        }
        Ok(out)
    }

    /// One non-streaming chat step.
    pub async fn chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: &[Value],
        timeout: Duration,
    ) -> Result<ChatMessage, OllamaError> {
        let mut body = json!({
            "model": model,
            "messages": messages,
            "stream": false,
            "keep_alive": "30m",
            "options": { "num_ctx": 16384, "temperature": 0.2 }
        });
        if !tools.is_empty() {
            body["tools"] = Value::Array(tools.to_vec());
        }
        let response = self
            .http
            .post(format!("{}/api/chat", self.base))
            .json(&body)
            .timeout(timeout)
            .send()
            .await
            .map_err(|e| self.unreachable(e))?;
        let status = response.status();
        let value: Value = response
            .json()
            .await
            .map_err(|e| OllamaError::Server(e.to_string()))?;
        if !status.is_success() {
            let message = value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("request failed");
            return Err(OllamaError::Server(message.to_string()));
        }
        serde_json::from_value(value.get("message").cloned().unwrap_or(Value::Null))
            .map_err(|e| OllamaError::Server(format!("unexpected reply: {e}")))
    }

    /// Download a model, reporting progress.
    pub async fn pull(
        &self,
        model: &str,
        mut on_progress: impl FnMut(PullProgress),
    ) -> Result<(), OllamaError> {
        let mut response = self
            .http
            .post(format!("{}/api/pull", self.base))
            .json(&json!({ "model": model, "stream": true }))
            .send()
            .await
            .map_err(|e| self.unreachable(e))?;
        if !response.status().is_success() {
            return Err(OllamaError::Server(format!(
                "pull failed with status {}",
                response.status()
            )));
        }
        let mut buffer = String::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| OllamaError::Server(e.to_string()))?
        {
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(i) = buffer.find('\n') {
                let line: String = buffer.drain(..=i).collect();
                let Ok(v) = serde_json::from_str::<Value>(line.trim()) else {
                    continue;
                };
                if let Some(err) = v.get("error").and_then(Value::as_str) {
                    return Err(OllamaError::Server(err.to_string()));
                }
                on_progress(PullProgress {
                    status: v
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    completed: v.get("completed").and_then(Value::as_u64).unwrap_or(0),
                    total: v.get("total").and_then(Value::as_u64).unwrap_or(0),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_and_capabilities_parse() {
        let tags =
            json!({"models":[{"name":"qwen3:14b","size":9000000000u64},{"name":"x"}, {"bad":1}]});
        assert_eq!(
            parse_models(&tags),
            vec![
                ("qwen3:14b".to_string(), 9_000_000_000),
                ("x".to_string(), 0)
            ]
        );
        assert_eq!(
            capabilities(&json!({"capabilities":["completion","tools","vision"]})),
            (true, true)
        );
        assert_eq!(
            capabilities(&json!({"capabilities":["completion"]})),
            (false, false)
        );
        assert_eq!(capabilities(&Value::Null), (false, false));
    }

    #[test]
    fn progress_percent() {
        let p = PullProgress {
            status: "x".into(),
            completed: 50,
            total: 200,
        };
        assert_eq!(p.percent(), Some(25));
        assert_eq!(
            PullProgress {
                status: "x".into(),
                completed: 0,
                total: 0
            }
            .percent(),
            None
        );
    }

    #[test]
    fn messages_serialise_without_empty_fields() {
        let v = serde_json::to_value(ChatMessage::new("user", "hi")).unwrap();
        assert_eq!(v, json!({"role":"user","content":"hi"}));
        let v = serde_json::to_value(ChatMessage::tool_result("powershell", "ok")).unwrap();
        assert_eq!(v["tool_name"], "powershell");
        let parsed: ChatMessage = serde_json::from_value(json!({
            "role":"assistant","content":"",
            "tool_calls":[{"function":{"name":"powershell","arguments":{"command":"dir"}}}]
        }))
        .unwrap();
        assert_eq!(parsed.tool_calls[0].function.arguments["command"], "dir");
    }
}
