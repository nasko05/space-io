//! Minimal client for an OpenRouter-style (OpenAI-compatible) chat-completions
//! API. We only model the slice of the protocol the agent loop needs:
//! messages, function/tool calls, and an optional web-search plugin.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::AgentConfig;
use crate::error::{AppError, AppResult};

/// Chat role. Lowercased on the wire to match the OpenAI schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// One chat message. The same struct is used for the request we send and the
/// assistant message we get back, so optional fields cover every role:
/// `tool_calls` only appears on assistant turns, `tool_call_id`/`name` only on
/// tool results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ChatMessage {
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: Some(text.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    /// Build the `role: "tool"` reply that answers a specific tool call.
    pub fn tool_result(tool_call_id: &str, name: &str, content: String) -> Self {
        Self {
            role: Role::Tool,
            content: Some(content),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.to_string()),
            name: Some(name.to_string()),
        }
    }
}

/// A function/tool call the model wants to make. `arguments` is a JSON-encoded
/// string per the OpenAI contract (not a parsed object).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Correlates this call with the tool result that answers it. The OpenAI
    /// contract requires it, but several open-weight models proxied through
    /// OpenRouter omit it, so we default to empty here and fill in a stable
    /// placeholder after parsing (see `fill_missing_tool_call_ids`) rather than
    /// failing the whole response to deserialize.
    #[serde(default)]
    pub id: String,
    #[serde(rename = "type", default = "default_tool_type")]
    pub kind: String,
    pub function: FunctionCall,
}

fn default_tool_type() -> String {
    "function".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    #[serde(default)]
    pub arguments: String,
}

/// A tool we advertise to the model. JSON-schema `parameters` describe the
/// arguments it may pass.
#[derive(Debug, Clone, Serialize)]
pub struct ToolDef {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub function: FunctionDef,
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionDef {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
}

impl ToolDef {
    pub fn function(name: &'static str, description: &'static str, parameters: Value) -> Self {
        Self {
            kind: "function",
            function: FunctionDef {
                name,
                description,
                parameters,
            },
        }
    }
}

#[derive(Serialize)]
struct WebPlugin {
    id: &'static str,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    tools: &'a [ToolDef],
    tool_choice: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    plugins: Option<Vec<WebPlugin>>,
    temperature: f32,
}

#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChatMessage,
}

/// Thin async wrapper around the chat-completions endpoint.
pub struct OpenRouterClient {
    http: reqwest::Client,
    endpoint: String,
    model: String,
    api_key: String,
    referer: Option<String>,
    title: Option<String>,
    /// Attach OpenRouter's built-in `web` plugin so the model can search the
    /// internet without a dedicated Brave key.
    web_plugin: bool,
}

impl OpenRouterClient {
    pub fn new(cfg: &AgentConfig, api_key: &str, web_plugin: bool) -> AppResult<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| AppError::Internal(format!("build http client: {e}")))?;
        Ok(Self {
            http,
            endpoint: format!("{}/chat/completions", cfg.base_url.trim_end_matches('/')),
            model: cfg.model.clone(),
            api_key: api_key.to_string(),
            referer: cfg.referer.clone(),
            title: cfg.title.clone(),
            web_plugin,
        })
    }

    /// Send the conversation + tool catalogue and return the assistant's next
    /// message (which may itself contain tool calls).
    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDef],
    ) -> AppResult<ChatMessage> {
        let body = ChatRequest {
            model: &self.model,
            messages,
            tools,
            tool_choice: "auto",
            plugins: self.web_plugin.then(|| vec![WebPlugin { id: "web" }]),
            temperature: 0.3,
        };

        let mut req = self
            .http
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body);
        // OpenRouter uses these for attribution / rankings; harmless elsewhere.
        if let Some(referer) = &self.referer {
            req = req.header("HTTP-Referer", referer);
        }
        if let Some(title) = &self.title {
            req = req.header("X-Title", title);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("agent provider request failed: {e}")))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| AppError::Internal(format!("agent provider body read failed: {e}")))?;

        if !status.is_success() {
            tracing::error!(%status, body = %text, "agent provider returned an error");
            return Err(map_provider_error(status));
        }

        let parsed: ChatResponse = serde_json::from_str(&text)
            .map_err(|e| AppError::Internal(format!("agent provider parse failed: {e}")))?;
        let mut message = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message)
            .ok_or_else(|| AppError::Internal("agent provider returned no choices".into()))?;
        fill_missing_tool_call_ids(&mut message);
        Ok(message)
    }
}

/// Give every tool call a non-empty `id`.
///
/// The OpenAI contract makes `id` mandatory, but open-weight models served
/// through OpenRouter (the default `qwen/*` model among them) routinely emit
/// tool calls with the field absent or blank. We need a stable, non-empty id
/// to thread back to the model as the matching tool result — and, for the
/// confirm-before-write tools, to round-trip through the browser as a
/// `tool_call_id`. Synthesise one from the call's position when the provider
/// leaves it empty; ids the provider did supply are left untouched. Indexing is
/// per-message, which is all the OpenAI tool-result protocol needs.
fn fill_missing_tool_call_ids(message: &mut ChatMessage) {
    if let Some(calls) = message.tool_calls.as_mut() {
        for (i, call) in calls.iter_mut().enumerate() {
            if call.id.trim().is_empty() {
                call.id = format!("call_{i}");
            }
        }
    }
}

/// Map a non-2xx from the provider onto something the operator can act on.
/// 4xx (other than rate limiting) usually means a bad key or model id — safe,
/// helpful detail to echo back. 5xx and rate limits are surfaced as-is.
fn map_provider_error(status: reqwest::StatusCode) -> AppError {
    if status.as_u16() == 429 {
        return AppError::TooManyRequests {
            retry_after_secs: 30,
        };
    }
    if status.is_client_error() {
        return AppError::BadRequest(format!(
            "the AI provider rejected the request ({status}). Check HEARTH_OPENROUTER_API_KEY \
             and HEARTH_AGENT_MODEL."
        ));
    }
    AppError::Internal(format!("AI provider error: {status}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_a_user_message_minimally() {
        let msg = ChatMessage {
            role: Role::User,
            content: Some("hi".into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        };
        let v = serde_json::to_value(&msg).unwrap();
        assert_eq!(v["role"], "user");
        assert_eq!(v["content"], "hi");
        // Absent optional fields must not be emitted.
        assert!(v.get("tool_calls").is_none());
        assert!(v.get("tool_call_id").is_none());
    }

    #[test]
    fn tool_result_carries_call_id_and_name() {
        let msg = ChatMessage::tool_result("call_1", "read_file", "body".into());
        let v = serde_json::to_value(&msg).unwrap();
        assert_eq!(v["role"], "tool");
        assert_eq!(v["tool_call_id"], "call_1");
        assert_eq!(v["name"], "read_file");
        assert_eq!(v["content"], "body");
    }

    #[test]
    fn deserializes_assistant_tool_call() {
        // Shape mirrors a real OpenRouter assistant turn that calls one tool.
        let raw = r#"{
            "role": "assistant",
            "content": null,
            "tool_calls": [
                {"id":"c1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"a.md\"}"}}
            ]
        }"#;
        let msg: ChatMessage = serde_json::from_str(raw).unwrap();
        assert_eq!(msg.role, Role::Assistant);
        assert!(msg.content.is_none());
        let calls = msg.tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "read_file");
        assert_eq!(calls[0].function.arguments, "{\"path\":\"a.md\"}");
    }

    #[test]
    fn deserializes_tool_call_with_missing_id() {
        // Open-weight models proxied through OpenRouter sometimes omit the
        // OpenAI-mandated `id`. That must parse (to an empty id) rather than
        // failing the whole response — otherwise the turn 500s.
        let raw = r#"{
            "role": "assistant",
            "content": null,
            "tool_calls": [
                {"type":"function","function":{"name":"write_file","arguments":"{}"}}
            ]
        }"#;
        let msg: ChatMessage = serde_json::from_str(raw).expect("must parse without an id");
        assert_eq!(msg.tool_calls.unwrap()[0].id, "");
    }

    #[test]
    fn fill_missing_tool_call_ids_synthesizes_only_blank_ids() {
        let mut msg: ChatMessage = serde_json::from_str(
            r#"{"role":"assistant","tool_calls":[
                {"type":"function","function":{"name":"a","arguments":"{}"}},
                {"id":"keep","type":"function","function":{"name":"b","arguments":"{}"}},
                {"id":"   ","type":"function","function":{"name":"c","arguments":"{}"}}
            ]}"#,
        )
        .unwrap();
        fill_missing_tool_call_ids(&mut msg);
        let calls = msg.tool_calls.unwrap();
        assert_eq!(calls[0].id, "call_0", "absent id is synthesised");
        assert_eq!(calls[1].id, "keep", "a real id is left untouched");
        assert_eq!(
            calls[2].id, "call_2",
            "whitespace-only id is treated as missing"
        );
    }

    #[test]
    fn fill_missing_tool_call_ids_is_a_noop_without_tool_calls() {
        let mut msg = ChatMessage {
            role: Role::Assistant,
            content: Some("plain answer".into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        };
        fill_missing_tool_call_ids(&mut msg);
        assert!(msg.tool_calls.is_none());
    }

    #[test]
    fn rate_limit_maps_to_too_many_requests() {
        let err = map_provider_error(reqwest::StatusCode::TOO_MANY_REQUESTS);
        assert!(matches!(err, AppError::TooManyRequests { .. }));
    }

    #[test]
    fn bad_key_maps_to_bad_request() {
        let err = map_provider_error(reqwest::StatusCode::UNAUTHORIZED);
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn provider_5xx_maps_to_internal() {
        let err = map_provider_error(reqwest::StatusCode::BAD_GATEWAY);
        assert!(matches!(err, AppError::Internal(_)));
    }

    /// Proves reqwest + rustls can complete a real TLS handshake against the
    /// provider. Ignored by default so CI (and offline dev) stay hermetic;
    /// run with `cargo test -- --ignored tls_smoke`.
    #[tokio::test]
    #[ignore]
    async fn tls_smoke() {
        let client = reqwest::Client::builder().build().unwrap();
        let resp = client
            .get("https://openrouter.ai/api/v1/models")
            .send()
            .await
            .expect("handshake + request");
        assert!(resp.status().is_success(), "status {}", resp.status());
    }
}
