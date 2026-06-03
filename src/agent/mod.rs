//! AI agent chat. A server-side tool-calling loop over an OpenRouter
//! (OpenAI-compatible) chat model.
//!
//! The agent can inspect the vault on its own — `list_files`, `read_file`,
//! `search_notes`, and (when configured) `web_search` all run here. Anything
//! that *changes* the vault is never executed in this module: the model's
//! mutating tool calls are returned to the browser as [`tools::PendingAction`]
//! proposals, and only applied — through the existing, audited `/api/files/*`
//! endpoints — once the user approves them. That is what makes the "confirm
//! each change" contract hold end to end.

pub mod openrouter;
pub mod tools;

use age::secrecy::SecretString;
use serde::Deserialize;

use crate::agent::openrouter::{ChatMessage, OpenRouterClient, Role, ToolCall};
use crate::error::{AppError, AppResult};
use crate::space::Space;

const DEFAULT_MODEL: &str = "qwen/qwen3.6-27b";
const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
const STEP_CAP_MESSAGE: &str =
    "I stopped after several steps without finishing. Could you narrow the request, \
     or tell me to keep going?";

/// How the agent reaches the web, decided once at startup from the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebSearch {
    /// No internet access.
    Disabled,
    /// Explicit `web_search` tool backed by the Brave Search API.
    Brave,
    /// OpenRouter's built-in `web` plugin — no extra key required.
    OpenRouterPlugin,
}

impl WebSearch {
    pub fn as_str(self) -> &'static str {
        match self {
            WebSearch::Disabled => "off",
            WebSearch::Brave => "brave",
            WebSearch::OpenRouterPlugin => "builtin",
        }
    }
}

/// Agent configuration, sourced entirely from the environment so secrets never
/// touch the on-disk space config. Deliberately *not* `Debug`: the API keys
/// must never land in a log line.
#[derive(Clone)]
pub struct AgentConfig {
    openrouter_api_key: Option<String>,
    brave_api_key: Option<String>,
    pub model: String,
    pub base_url: String,
    pub web_search: WebSearch,
    pub referer: Option<String>,
    pub title: Option<String>,
    pub max_steps: usize,
}

impl AgentConfig {
    pub fn from_env() -> Self {
        let openrouter_api_key = env_nonempty("HEARTH_OPENROUTER_API_KEY");
        let brave_api_key = env_nonempty("HEARTH_BRAVE_API_KEY");
        let model = env_nonempty("HEARTH_AGENT_MODEL").unwrap_or_else(|| DEFAULT_MODEL.to_string());
        let base_url = env_nonempty("HEARTH_OPENROUTER_BASE_URL")
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        // Web search is on by default; an explicit falsey value forces it off.
        // With a Brave key we expose the explicit tool; otherwise we lean on
        // OpenRouter's built-in plugin so search still works out of the box.
        let web_enabled = std::env::var("HEARTH_AGENT_WEB_SEARCH")
            .ok()
            .map(|v| truthy(&v))
            .unwrap_or(true);
        let web_search = if !web_enabled {
            WebSearch::Disabled
        } else if brave_api_key.is_some() {
            WebSearch::Brave
        } else {
            WebSearch::OpenRouterPlugin
        };

        let referer = env_nonempty("HEARTH_AGENT_REFERER");
        let title = env_nonempty("HEARTH_AGENT_TITLE").or_else(|| Some("Hearth".to_string()));
        let max_steps = env_nonempty("HEARTH_AGENT_MAX_STEPS")
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|n| (1..=24).contains(n))
            .unwrap_or(8);

        Self {
            openrouter_api_key,
            brave_api_key,
            model,
            base_url,
            web_search,
            referer,
            title,
            max_steps,
        }
    }

    /// Whether an OpenRouter key is present. Without it the chat endpoint is
    /// disabled and the UI hides the assistant.
    pub fn is_configured(&self) -> bool {
        self.openrouter_api_key
            .as_deref()
            .is_some_and(|k| !k.is_empty())
    }

    fn api_key(&self) -> AppResult<&str> {
        self.openrouter_api_key.as_deref().ok_or_else(|| {
            AppError::BadRequest(
                "the AI agent is not configured on this server (set HEARTH_OPENROUTER_API_KEY)"
                    .into(),
            )
        })
    }
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn truthy(v: &str) -> bool {
    matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// The result of one `/agent/chat` round-trip.
pub struct AgentTurn {
    /// Updated conversation (without the server-side system prompt) for the
    /// browser to echo back on the next call.
    pub messages: Vec<ChatMessage>,
    /// The assistant's latest prose, if it produced any this turn.
    pub assistant_text: Option<String>,
    /// Mutating tool calls awaiting user confirmation. Empty when the turn is
    /// complete.
    pub pending_actions: Vec<tools::PendingAction>,
    /// True when there is nothing left for the user to confirm.
    pub done: bool,
}

/// Run the model until it produces a final answer or asks to change the vault.
///
/// Read-only tool calls are executed inline and the loop continues. The first
/// time the model proposes a *mutating* change, the loop stops and hands the
/// proposals back for confirmation.
pub async fn run_turn(
    cfg: &AgentConfig,
    space: Space,
    passphrase: SecretString,
    incoming: Vec<ChatMessage>,
) -> AppResult<AgentTurn> {
    let api_key = cfg.api_key()?;
    let web_plugin = cfg.web_search == WebSearch::OpenRouterPlugin;
    let brave_web = cfg.web_search == WebSearch::Brave;
    let client = OpenRouterClient::new(cfg, api_key, web_plugin)?;
    let tools = tools::tool_definitions(brave_web);

    let mut messages = with_system_prompt(incoming, cfg);

    let mut steps = 0usize;
    let (assistant_text, pending) = loop {
        if steps >= cfg.max_steps {
            break (Some(STEP_CAP_MESSAGE.to_string()), Vec::new());
        }
        steps += 1;

        let assistant = client.chat(&messages, &tools).await?;
        let tool_calls = assistant.tool_calls.clone().unwrap_or_default();
        let content = assistant.content.clone();
        messages.push(assistant);

        if tool_calls.is_empty() {
            break (content, Vec::new());
        }

        // Execute read-only calls now; collect mutating ones as proposals.
        // Mutating calls with unparseable arguments are answered with an error
        // tool result instead, so the conversation stays valid and the model
        // can correct itself.
        let mut pending = Vec::new();
        for call in &tool_calls {
            if tools::is_mutating(&call.function.name) {
                match tools::build_pending(call) {
                    Ok(p) => pending.push(p),
                    Err(e) => messages.push(ChatMessage::tool_result(
                        &call.id,
                        &call.function.name,
                        format!("Error: {}", error_text(&e)),
                    )),
                }
            } else {
                let result = run_readonly(cfg, &space, &passphrase, call).await;
                messages.push(ChatMessage::tool_result(
                    &call.id,
                    &call.function.name,
                    result,
                ));
            }
        }

        if !pending.is_empty() {
            break (content, pending);
        }
        // Otherwise every call was answered server-side; loop so the model can
        // act on the results.
    };

    let done = pending.is_empty();
    // The system prompt is server-owned; never echo it back to the browser.
    messages.retain(|m| m.role != Role::System);

    Ok(AgentTurn {
        messages,
        assistant_text,
        pending_actions: pending,
        done,
    })
}

/// Dispatch a single read-only tool call, folding any error into the returned
/// string so a bad call informs the model rather than aborting the turn.
async fn run_readonly(
    cfg: &AgentConfig,
    space: &Space,
    passphrase: &SecretString,
    call: &ToolCall,
) -> String {
    let args = parse_args(&call.function.arguments);

    if call.function.name == "web_search" {
        return match brave_search(cfg, &args).await {
            Ok(s) => s,
            Err(e) => format!("Error: {}", error_text(&e)),
        };
    }

    let space = space.clone();
    let passphrase = passphrase.clone();
    let name = call.function.name.clone();
    match tokio::task::spawn_blocking(move || {
        tools::execute_vault_tool(&space, &passphrase, &name, &args)
    })
    .await
    {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => format!("Error: {}", error_text(&e)),
        Err(e) => format!("Error: tool execution failed: {e}"),
    }
}

fn parse_args(raw: &str) -> serde_json::Value {
    if raw.trim().is_empty() {
        return serde_json::Value::Object(Default::default());
    }
    serde_json::from_str(raw).unwrap_or(serde_json::Value::Null)
}

fn with_system_prompt(incoming: Vec<ChatMessage>, cfg: &AgentConfig) -> Vec<ChatMessage> {
    let mut out = Vec::with_capacity(incoming.len() + 1);
    out.push(ChatMessage::system(system_prompt(cfg)));
    // Drop any client-supplied system messages — the prompt is ours.
    for m in incoming {
        if m.role != Role::System {
            out.push(m);
        }
    }
    out
}

fn system_prompt(cfg: &AgentConfig) -> String {
    let web_line = match cfg.web_search {
        WebSearch::Disabled => "",
        WebSearch::Brave => "\n- You may call web_search to look things up on the public internet.",
        WebSearch::OpenRouterPlugin => {
            "\n- You can search the public internet when a question needs current or \
             external facts."
        }
    };
    format!(
        "You are the assistant built into Hearth, a private, encrypted personal note vault. \
You help the user find, understand, write, and reorganise their notes and files.

The vault is a tree of files and folders. Paths are relative to the vault root and use '/'. \
Notes are Markdown and their paths end in `.md`, for example `Journal/2026/welcome.md`. \
A note's title is its first `# Heading` line.

Read-only tools you may use freely: list_files, read_file, search_notes.
Tools that change the vault — write_file, move_path, delete_path, create_folder, set_tags — \
are PROPOSED to the user and applied only after they approve in the UI. Call them when you \
intend a change, but never claim a change is finished; after a proposal the system tells you \
whether it was applied or declined.

Guidelines:
- Before editing or moving something, read or list the relevant paths so you act on what \
really exists.
- When writing a note, output the complete Markdown content — the file is replaced with \
exactly what you provide. Keep a single top-level `# Title`.
- Prefer small, reversible steps, and briefly explain what you did or propose.
- Use relative paths only — never absolute paths or `..`.{web_line}"
    )
}

/// Translate an `AppError` into a short, non-sensitive string suitable for
/// feeding back to the model as a tool result.
fn error_text(e: &AppError) -> String {
    match e {
        AppError::NotFound => "not found".to_string(),
        AppError::Forbidden => "forbidden path".to_string(),
        AppError::BadRequest(m) => m.clone(),
        AppError::TooManyRequests { .. } => "rate limited".to_string(),
        AppError::Unauthorized | AppError::WrongPassphrase => "not permitted".to_string(),
        AppError::Io(_) | AppError::Internal(_) => "internal error".to_string(),
    }
}

// ---- Brave web search ----

#[derive(Deserialize)]
struct BraveResponse {
    #[serde(default)]
    web: Option<BraveWeb>,
}

#[derive(Deserialize)]
struct BraveWeb {
    #[serde(default)]
    results: Vec<BraveResult>,
}

#[derive(Deserialize)]
struct BraveResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    description: String,
}

const BRAVE_ENDPOINT: &str = "https://api.search.brave.com/res/v1/web/search";
const BRAVE_RESULT_COUNT: usize = 5;

async fn brave_search(cfg: &AgentConfig, args: &serde_json::Value) -> AppResult<String> {
    let query = args
        .get("query")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .ok_or_else(|| AppError::BadRequest("web_search requires a `query`".into()))?;
    let key = cfg
        .brave_api_key
        .as_deref()
        .ok_or_else(|| AppError::Internal("brave key missing".into()))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| AppError::Internal(format!("build http client: {e}")))?;

    let resp = client
        .get(BRAVE_ENDPOINT)
        .query(&[("q", query), ("count", &BRAVE_RESULT_COUNT.to_string())])
        .header("Accept", "application/json")
        .header("X-Subscription-Token", key)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("brave request failed: {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        tracing::error!(%status, "brave search returned an error");
        if status.as_u16() == 429 {
            return Err(AppError::TooManyRequests {
                retry_after_secs: 30,
            });
        }
        return Err(AppError::Internal(format!("brave search error: {status}")));
    }

    let body: BraveResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("brave parse failed: {e}")))?;

    let results = body.web.map(|w| w.results).unwrap_or_default();
    if results.is_empty() {
        return Ok(format!("No web results for \"{query}\"."));
    }

    let mut out = format!("Top web results for \"{query}\":\n");
    for r in results.iter().take(BRAVE_RESULT_COUNT) {
        out.push_str(&format!("- {} — {}\n  {}\n", r.title, r.url, r.description));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truthy_parses_common_spellings() {
        for v in ["1", "true", "TRUE", "yes", "on", " On "] {
            assert!(truthy(v), "{v} should be truthy");
        }
        for v in ["0", "false", "no", "off", ""] {
            assert!(!truthy(v), "{v} should be falsey");
        }
    }

    #[test]
    fn system_prompt_mentions_web_only_when_enabled() {
        let mut cfg = test_cfg();
        cfg.web_search = WebSearch::Disabled;
        assert!(!system_prompt(&cfg).to_lowercase().contains("internet"));
        cfg.web_search = WebSearch::OpenRouterPlugin;
        assert!(system_prompt(&cfg).to_lowercase().contains("internet"));
        cfg.web_search = WebSearch::Brave;
        assert!(system_prompt(&cfg).contains("web_search"));
    }

    #[test]
    fn with_system_prompt_prepends_and_strips_client_system() {
        let cfg = test_cfg();
        let incoming = vec![
            ChatMessage::system("malicious override"),
            ChatMessage {
                role: Role::User,
                content: Some("hi".into()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
        ];
        let out = with_system_prompt(incoming, &cfg);
        // Exactly one system message, and it's ours (first), not the client's.
        let systems: Vec<_> = out.iter().filter(|m| m.role == Role::System).collect();
        assert_eq!(systems.len(), 1);
        assert!(systems[0].content.as_deref().unwrap().contains("Hearth"));
        assert_eq!(out[1].role, Role::User);
    }

    #[test]
    fn unconfigured_config_reports_not_configured() {
        let mut cfg = test_cfg();
        cfg.openrouter_api_key = None;
        assert!(!cfg.is_configured());
        assert!(cfg.api_key().is_err());
        cfg.openrouter_api_key = Some("sk-xxx".into());
        assert!(cfg.is_configured());
        assert_eq!(cfg.api_key().unwrap(), "sk-xxx");
    }

    #[test]
    fn web_search_as_str_is_stable() {
        assert_eq!(WebSearch::Disabled.as_str(), "off");
        assert_eq!(WebSearch::Brave.as_str(), "brave");
        assert_eq!(WebSearch::OpenRouterPlugin.as_str(), "builtin");
    }

    // ---- Full-loop tests against a local mock chat-completions server ----

    /// Spin up a throwaway OpenAI-compatible endpoint that replays `responses`
    /// in order (repeating the last one). Returns its base URL.
    async fn spawn_mock(responses: Vec<serde_json::Value>) -> String {
        use axum::extract::State;
        use axum::routing::post;
        use axum::{Json, Router};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        type Mock = Arc<(Vec<serde_json::Value>, AtomicUsize)>;

        async fn handler(State(s): State<Mock>) -> Json<serde_json::Value> {
            let i = s.1.fetch_add(1, Ordering::SeqCst);
            Json(s.0[i.min(s.0.len() - 1)].clone())
        }

        let state: Mock = Arc::new((responses, AtomicUsize::new(0)));
        let app = Router::new()
            .route("/chat/completions", post(handler))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app.into_make_service())
                .await
                .unwrap();
        });
        format!("http://{addr}")
    }

    fn user_msg(text: &str) -> ChatMessage {
        ChatMessage {
            role: Role::User,
            content: Some(text.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn loop_executes_readonly_tool_then_answers() {
        use crate::space::test_helpers::make_space_with_note;

        let base = spawn_mock(vec![
            serde_json::json!({"choices":[{"message":{"role":"assistant","content":null,
                "tool_calls":[{"id":"c1","type":"function","function":{
                    "name":"read_file","arguments":"{\"path\":\"a.md\"}"}}]}}]}),
            serde_json::json!({"choices":[{"message":{"role":"assistant",
                "content":"The note body is present. DONE"}}]}),
        ])
        .await;

        let mut cfg = test_cfg();
        cfg.base_url = base;
        cfg.web_search = WebSearch::Disabled;

        let (_d, space, pass) = make_space_with_note("p", "a.md", "# Hi\n\nsecret body");
        let turn = run_turn(&cfg, space, pass, vec![user_msg("what does a.md say?")])
            .await
            .unwrap();

        assert!(turn.done);
        assert!(turn.pending_actions.is_empty());
        assert_eq!(
            turn.assistant_text.as_deref(),
            Some("The note body is present. DONE")
        );
        // The decrypted body was fed back as a tool result…
        let tool_msg = turn
            .messages
            .iter()
            .find(|m| m.role == Role::Tool)
            .expect("a tool result in the transcript");
        assert!(tool_msg.content.as_deref().unwrap().contains("secret body"));
        // …and the server-owned system prompt is never echoed back.
        assert!(turn.messages.iter().all(|m| m.role != Role::System));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn loop_yields_mutating_tool_without_applying_it() {
        use crate::space::test_helpers::make_space_with_note;

        let base = spawn_mock(vec![
            serde_json::json!({"choices":[{"message":{"role":"assistant",
                "content":"I'll create that note.",
                "tool_calls":[{"id":"w1","type":"function","function":{
                    "name":"write_file","arguments":"{\"path\":\"new.md\",\"content\":\"# New\"}"}}]}}]}),
            // Must not be reached: the loop stops to ask for confirmation.
            serde_json::json!({"choices":[{"message":{"role":"assistant","content":"unreachable"}}]}),
        ])
        .await;

        let mut cfg = test_cfg();
        cfg.base_url = base;

        let (dir, space, pass) = make_space_with_note("p", "a.md", "x");
        let turn = run_turn(&cfg, space, pass, vec![user_msg("make a note called new")])
            .await
            .unwrap();

        assert!(!turn.done, "a proposed change should pause the turn");
        assert_eq!(turn.pending_actions.len(), 1);
        assert_eq!(turn.pending_actions[0].tool, "write_file");
        assert_eq!(turn.pending_actions[0].args["path"], "new.md");
        // Crucially, nothing was written — the proposal awaits user approval.
        assert!(
            !dir.path().join("space/new.md.age").exists(),
            "mutating tool must not touch disk server-side"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn loop_handles_mutating_tool_call_without_a_provider_id() {
        // Regression: "create a file" returned 500 because the model's
        // write_file tool call arrived with no `id` (common with open-weight
        // models on OpenRouter) and the response failed to deserialise. The
        // turn must now succeed, proposing the change with a synthesised id.
        use crate::space::test_helpers::make_space_with_note;

        let base = spawn_mock(vec![serde_json::json!({"choices":[{"message":{
            "role":"assistant",
            "content":"I'll create that note.",
            // Note: the tool call has no "id" field at all.
            "tool_calls":[{"type":"function","function":{
                "name":"write_file","arguments":"{\"path\":\"new.md\",\"content\":\"# New\"}"}}]}}]})])
        .await;

        let mut cfg = test_cfg();
        cfg.base_url = base;

        let (_dir, space, pass) = make_space_with_note("p", "a.md", "x");
        let turn = run_turn(&cfg, space, pass, vec![user_msg("make a note called new")])
            .await
            .expect("turn must not 500 just because the id was missing");

        assert!(!turn.done);
        assert_eq!(turn.pending_actions.len(), 1);
        assert_eq!(turn.pending_actions[0].tool, "write_file");
        // A non-empty id is essential: the browser echoes it back as the
        // tool_call_id when it reports the result, and the model needs it to
        // match the result to its call.
        assert!(
            !turn.pending_actions[0].tool_call_id.trim().is_empty(),
            "a tool_call_id must be present so the result can be correlated"
        );
    }

    fn test_cfg() -> AgentConfig {
        AgentConfig {
            openrouter_api_key: Some("sk-test".into()),
            brave_api_key: None,
            model: DEFAULT_MODEL.into(),
            base_url: DEFAULT_BASE_URL.into(),
            web_search: WebSearch::OpenRouterPlugin,
            referer: None,
            title: Some("Hearth".into()),
            max_steps: 8,
        }
    }
}
