//! The agent's tool catalogue and dispatch for the read-only tools.
//!
//! Read-only tools (`list_files`, `read_file`, `search_notes`) run here against
//! the unlocked [`Space`]. Mutating tools (`write_file`, `move_path`,
//! `delete_path`, `create_folder`, `set_tags`) are never executed here:
//! [`build_pending`] turns them into a [`PendingAction`] the browser confirms
//! and applies via `/api/files/*`. `web_search` lives in the loop because it is
//! async network I/O, not a vault operation.

use age::secrecy::SecretString;
use serde::Serialize;
use serde_json::{json, Value};

use crate::agent::openrouter::{ToolCall, ToolDef};
use crate::error::{AppError, AppResult};
use crate::space::{read, search, tree, Space};

/// Tools that change the vault. The loop refuses to execute these and routes
/// them to the user for confirmation instead.
pub const MUTATING_TOOLS: &[&str] = &[
    "write_file",
    "move_path",
    "delete_path",
    "create_folder",
    "set_tags",
];

pub fn is_mutating(name: &str) -> bool {
    MUTATING_TOOLS.contains(&name)
}

/// A proposed vault change awaiting user approval. `args` is the parsed tool
/// arguments; the browser uses `tool` + `args` to pick the right endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct PendingAction {
    pub tool_call_id: String,
    pub tool: String,
    pub args: Value,
    pub summary: String,
}

/// Build the full tool catalogue we advertise to the model. `brave_web` adds
/// the explicit `web_search` function; when web search runs through
/// OpenRouter's built-in plugin instead, the model needs no function for it.
pub fn tool_definitions(brave_web: bool) -> Vec<ToolDef> {
    let mut tools = vec![
        ToolDef::function(
            "list_files",
            "List every file and folder in the vault as relative paths. Call this \
             to discover what exists before reading, editing, or moving anything.",
            json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        ),
        ToolDef::function(
            "read_file",
            "Read and return the full decrypted text of one note. `path` is the \
             relative path shown by list_files (for example `Journal/2026/welcome.md`).",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "Relative path to the note." } },
                "required": ["path"],
                "additionalProperties": false
            }),
        ),
        ToolDef::function(
            "search_notes",
            "Full-text search across note titles, bodies, and tags. Returns the \
             best-matching paths with short snippets.",
            json!({
                "type": "object",
                "properties": { "query": { "type": "string", "description": "Words to search for." } },
                "required": ["query"],
                "additionalProperties": false
            }),
        ),
        ToolDef::function(
            "write_file",
            "Create or overwrite a note. The file is replaced with exactly the \
             `content` you provide, so include the whole note (a single top-level \
             `# Title` is conventional). Requires user approval before it is applied.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path of the note to write." },
                    "content": { "type": "string", "description": "Full Markdown content for the note." },
                    "message": { "type": "string", "description": "Optional short commit message." }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        ),
        ToolDef::function(
            "move_path",
            "Move or rename a file or folder. Moving a folder carries its whole \
             subtree. Requires user approval before it is applied.",
            json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Current relative path." },
                    "to": { "type": "string", "description": "New relative path." }
                },
                "required": ["from", "to"],
                "additionalProperties": false
            }),
        ),
        ToolDef::function(
            "delete_path",
            "Delete a file or folder. Deletes are reversible — they move to the \
             vault trash. Requires user approval before it is applied.",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "Relative path to delete." } },
                "required": ["path"],
                "additionalProperties": false
            }),
        ),
        ToolDef::function(
            "create_folder",
            "Create a new (empty) folder. Requires user approval before it is applied.",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "Relative path of the folder to create." } },
                "required": ["path"],
                "additionalProperties": false
            }),
        ),
        ToolDef::function(
            "set_tags",
            "Replace the tag list on a note. Pass an empty array to clear its tags. \
             Requires user approval before it is applied.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path of the note." },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "The complete new tag list." }
                },
                "required": ["path", "tags"],
                "additionalProperties": false
            }),
        ),
    ];

    if brave_web {
        tools.push(ToolDef::function(
            "web_search",
            "Search the public internet and return the top results (title, url, \
             snippet). Use it for facts that are not in the vault.",
            json!({
                "type": "object",
                "properties": { "query": { "type": "string", "description": "What to search the web for." } },
                "required": ["query"],
                "additionalProperties": false
            }),
        ));
    }

    tools
}

/// Run one of the read-only vault tools. Errors are returned as `AppError` so
/// the caller can fold them back into the conversation as a tool result the
/// model can recover from.
pub fn execute_vault_tool(
    space: &Space,
    passphrase: &SecretString,
    name: &str,
    args: &Value,
) -> AppResult<String> {
    match name {
        "list_files" => {
            let nodes = tree::build_tree(space)?;
            let mut out = String::new();
            render_tree(&nodes, &mut out);
            if out.is_empty() {
                out.push_str("(the vault is empty)");
            }
            Ok(out)
        }
        "read_file" => {
            let path = str_arg(args, "path")?;
            let file = read::read_file(space, passphrase, &path)?;
            Ok(truncate(&file.content))
        }
        "search_notes" => {
            let query = str_arg(args, "query")?;
            let hits = search::search(space, passphrase, &query)?;
            if hits.is_empty() {
                return Ok(format!("No notes match \"{query}\"."));
            }
            let mut out = String::new();
            for hit in &hits {
                let title = hit.title.as_deref().unwrap_or("(untitled)");
                out.push_str(&format!("- {} — {}\n  {}\n", hit.path, title, hit.snippet));
            }
            Ok(out)
        }
        other => Err(AppError::BadRequest(format!(
            "unknown read-only tool: {other}"
        ))),
    }
}

/// Turn a mutating tool call into a confirmable proposal. Fails only if the
/// model emitted arguments that aren't valid JSON.
pub fn build_pending(call: &ToolCall) -> AppResult<PendingAction> {
    let args: Value = if call.function.arguments.trim().is_empty() {
        Value::Object(Default::default())
    } else {
        serde_json::from_str(&call.function.arguments)
            .map_err(|_| AppError::BadRequest("model produced invalid tool arguments".into()))?
    };
    let summary = summarize(&call.function.name, &args);
    Ok(PendingAction {
        tool_call_id: call.id.clone(),
        tool: call.function.name.clone(),
        args,
        summary,
    })
}

/// Human one-liner describing a proposed change, shown on the confirmation card.
fn summarize(name: &str, args: &Value) -> String {
    let arg = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or("?");
    match name {
        "write_file" => {
            let chars = args
                .get("content")
                .and_then(Value::as_str)
                .map(str::len)
                .unwrap_or(0);
            format!("Write “{}” ({chars} chars)", arg("path"))
        }
        "move_path" => format!("Move {} → {}", arg("from"), arg("to")),
        "delete_path" => format!("Delete {} (to trash)", arg("path")),
        "create_folder" => format!("Create folder {}", arg("path")),
        "set_tags" => {
            let tags = args
                .get("tags")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            format!("Set tags on {} to [{tags}]", arg("path"))
        }
        other => format!("{other} {args}"),
    }
}

fn str_arg(args: &Value, key: &str) -> AppResult<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest(format!("tool argument `{key}` is required")))
}

/// Cap how much note text we hand the model in one tool result. Big enough for
/// any reasonable note; bounds token spend on a pathological one.
const MAX_TOOL_OUTPUT: usize = 80_000;

fn truncate(text: &str) -> String {
    if text.len() <= MAX_TOOL_OUTPUT {
        return text.to_string();
    }
    let mut end = MAX_TOOL_OUTPUT;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n\n…[truncated, note is longer]", &text[..end])
}

fn render_tree(nodes: &[tree::TreeNode], out: &mut String) {
    for node in nodes {
        match node {
            tree::TreeNode::Folder { path, children, .. } => {
                out.push_str(&format!("{path}/\n"));
                render_tree(children, out);
            }
            tree::TreeNode::File { path, kind, .. } => {
                out.push_str(&format!("{path} ({kind})\n"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::openrouter::FunctionCall;
    use crate::space::test_helpers::make_space_with_note;

    fn call(name: &str, args: &str) -> ToolCall {
        ToolCall {
            id: "c1".into(),
            kind: "function".into(),
            function: FunctionCall {
                name: name.into(),
                arguments: args.into(),
            },
        }
    }

    #[test]
    fn mutating_classification_is_exhaustive() {
        for m in MUTATING_TOOLS {
            assert!(is_mutating(m));
        }
        for ro in ["read_file", "list_files", "search_notes", "web_search"] {
            assert!(!is_mutating(ro));
        }
    }

    #[test]
    fn web_tool_only_present_when_brave_enabled() {
        let without = tool_definitions(false);
        assert!(!without.iter().any(|t| t.function.name == "web_search"));
        let with = tool_definitions(true);
        assert!(with.iter().any(|t| t.function.name == "web_search"));
    }

    #[test]
    fn read_file_tool_returns_decrypted_body() {
        let (_d, space, pass) =
            make_space_with_note("p", "Journal/2026/note.md", "# Title\n\nBody.");
        let out = execute_vault_tool(
            &space,
            &pass,
            "read_file",
            &json!({ "path": "Journal/2026/note.md" }),
        )
        .unwrap();
        assert!(out.contains("# Title"));
        assert!(out.contains("Body."));
    }

    #[test]
    fn read_file_missing_path_arg_is_bad_request() {
        let (_d, space, pass) = make_space_with_note("p", "a.md", "x");
        let err = execute_vault_tool(&space, &pass, "read_file", &json!({})).unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn list_files_renders_paths_and_kinds() {
        let (_d, space, pass) = make_space_with_note("p", "Journal/2026/note.md", "x");
        let out = execute_vault_tool(&space, &pass, "list_files", &json!({})).unwrap();
        assert!(out.contains("Journal/"), "got: {out}");
        assert!(out.contains("Journal/2026/note.md (md)"), "got: {out}");
    }

    #[test]
    fn search_notes_finds_body_match() {
        let (_d, space, pass) =
            make_space_with_note("p", "a.md", "# Sunday\n\nThe quick brown fox");
        let out =
            execute_vault_tool(&space, &pass, "search_notes", &json!({ "query": "fox" })).unwrap();
        assert!(out.contains("a.md"));
    }

    #[test]
    fn unknown_tool_is_bad_request() {
        let (_d, space, pass) = make_space_with_note("p", "a.md", "x");
        let err = execute_vault_tool(&space, &pass, "nope", &json!({})).unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn build_pending_summarizes_write() {
        let p = build_pending(&call("write_file", r#"{"path":"a.md","content":"hello"}"#)).unwrap();
        assert_eq!(p.tool, "write_file");
        assert_eq!(p.args["path"], "a.md");
        assert!(p.summary.contains("a.md"));
        assert!(p.summary.contains("5 chars"));
    }

    #[test]
    fn build_pending_summarizes_move_and_delete() {
        let mv = build_pending(&call("move_path", r#"{"from":"a.md","to":"b.md"}"#)).unwrap();
        assert!(mv.summary.contains("a.md → b.md"));
        let del = build_pending(&call("delete_path", r#"{"path":"a.md"}"#)).unwrap();
        assert!(del.summary.contains("Delete a.md"));
    }

    #[test]
    fn build_pending_rejects_invalid_json_args() {
        let err = build_pending(&call("write_file", "{not json")).unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn build_pending_tolerates_empty_args() {
        // Some models send an empty string for no-arg calls.
        let p = build_pending(&call("create_folder", "")).unwrap();
        assert_eq!(p.tool, "create_folder");
    }
}
