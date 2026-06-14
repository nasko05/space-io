use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::{self, openrouter::ChatMessage, openrouter::Role};
use crate::error::{AppError, AppResult};
use crate::routes::auth::require_session;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/agent/status", get(status))
        .route("/agent/chat", post(chat))
}

#[derive(Serialize)]
struct StatusResponse {
    configured: bool,
    model: String,
    /// `"brave"`, `"builtin"`, or `"off"`.
    web_search: &'static str,
}

/// Report whether the agent is usable. Session-gated so the server config isn't
/// exposed to anonymous probes.
async fn status(State(state): State<AppState>, jar: CookieJar) -> AppResult<Json<StatusResponse>> {
    require_session(&state, &jar)?;
    let cfg = &state.agent;
    Ok(Json(StatusResponse {
        configured: cfg.is_configured(),
        model: cfg.model.clone(),
        web_search: cfg.web_search.as_str(),
    }))
}

/// Upper bounds on the conversation a single request may carry. The server is
/// stateless between turns, so the full history rides along each time; clamp it
/// to bound request size and token spend.
const MAX_MESSAGES: usize = 200;
const MAX_TOTAL_CHARS: usize = 600_000;

#[derive(Deserialize)]
struct ChatRequestBody {
    messages: Vec<ChatMessage>,
}

#[derive(Serialize)]
struct PendingActionDto {
    tool_call_id: String,
    tool: String,
    args: Value,
    summary: String,
}

#[derive(Serialize)]
struct ChatResponseBody {
    messages: Vec<ChatMessage>,
    assistant_text: Option<String>,
    pending_actions: Vec<PendingActionDto>,
    done: bool,
}

/// Run one agent turn. The request shape is validated before the provider-key
/// check so a malformed call is rejected identically whether or not a provider
/// key is configured.
async fn chat(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<ChatRequestBody>,
) -> AppResult<Json<ChatResponseBody>> {
    let (passphrase, space) = require_session(&state, &jar)?;

    if body.messages.is_empty() {
        return Err(AppError::BadRequest("messages must not be empty".into()));
    }
    if body.messages.len() > MAX_MESSAGES {
        return Err(AppError::BadRequest(
            "conversation is too long; start a new chat".into(),
        ));
    }
    let total: usize = body
        .messages
        .iter()
        .filter_map(|m| m.content.as_deref().map(str::len))
        .sum();
    if total > MAX_TOTAL_CHARS {
        return Err(AppError::BadRequest(
            "conversation is too large; start a new chat".into(),
        ));
    }
    let last_ok = matches!(
        body.messages.last().map(|m| m.role),
        Some(Role::User) | Some(Role::Tool)
    );
    if !last_ok {
        return Err(AppError::BadRequest(
            "the last message must be from the user or a tool result".into(),
        ));
    }

    if !state.agent.is_configured() {
        return Err(AppError::BadRequest(
            "the AI agent is not configured on this server".into(),
        ));
    }

    let turn = agent::run_turn(&state.agent, space, passphrase, body.messages).await?;

    Ok(Json(ChatResponseBody {
        messages: turn.messages,
        assistant_text: turn.assistant_text,
        pending_actions: turn
            .pending_actions
            .into_iter()
            .map(|p| PendingActionDto {
                tool_call_id: p.tool_call_id,
                tool: p.tool,
                args: p.args,
                summary: p.summary,
            })
            .collect(),
        done: turn.done,
    }))
}
