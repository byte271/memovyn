use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::app::Memovyn;
use crate::domain::{
    AddMemoryRequest, ArchiveRequest, FeedbackRequest, ReflectionRequest, SearchRequest,
};
use crate::error::Result;

pub fn router(app: Arc<Memovyn>) -> Router {
    Router::new()
        .route("/mcp", post(handle_http))
        .with_state(app)
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

pub async fn serve_stdio(app: Arc<Memovyn>) -> Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin).lines();
    let mut writer = tokio::io::BufWriter::new(stdout);

    while let Some(line) = reader.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(error) => {
                let response = JsonRpcResponse {
                    jsonrpc: "2.0",
                    id: None,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32700,
                        message: error.to_string(),
                    }),
                };
                writer
                    .write_all(serde_json::to_string(&response)?.as_bytes())
                    .await?;
                writer.write_all(b"\n").await?;
                writer.flush().await?;
                continue;
            }
        };

        let response = dispatch(app.clone(), request).await;
        writer
            .write_all(serde_json::to_string(&response)?.as_bytes())
            .await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
    }

    Ok(())
}

async fn handle_http(
    State(app): State<Arc<Memovyn>>,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    Json(dispatch(app, request).await)
}

async fn dispatch(app: Arc<Memovyn>, request: JsonRpcRequest) -> JsonRpcResponse {
    let id = request.id.clone();
    let result = match request.method.as_str() {
        "initialize" => Ok(serde_json::json!({
            "protocolVersion": "2025-11-05",
            "serverInfo": { "name": "memovyn", "version": "0.1.0" },
            "capabilities": {
                "tools": { "listChanged": false },
                "logging": {},
                "resources": {},
                "prompts": {}
            }
        })),
        "tools/list" => Ok(tool_list()),
        "tools/call" => handle_tool_call(app, request.params).await,
        "ping" => Ok(serde_json::json!({ "ok": true })),
        _ => Err(JsonRpcError {
            code: -32601,
            message: format!("unknown method {}", request.method),
        }),
    };

    match result {
        Ok(result) => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        },
        Err(error) => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(error),
        },
    }
}

fn tool_list() -> Value {
    serde_json::json!({
        "tools": [
            {
                "name": "add_memory",
                "description": "Add a project-scoped permanent memory and auto-classify it with Memovyn's multi-dimensional taxonomy compiler.",
                "inputSchema": {
                    "type": "object",
                    "required": ["project_id", "content"],
                    "properties": {
                        "project_id": { "type": "string" },
                        "content": { "type": "string" },
                        "metadata": { "type": "object" },
                        "kind": { "type": "string" }
                    }
                }
            },
            {
                "name": "search_memories",
                "description": "Search a project's memories with progressive disclosure output across index, summary, timeline, and detail layers.",
                "inputSchema": {
                    "type": "object",
                    "required": ["project_id", "query"],
                    "properties": {
                        "project_id": { "type": "string" },
                        "query": { "type": "string" },
                        "limit": { "type": "integer" },
                        "filters": { "type": "object" }
                    }
                }
            },
            {
                "name": "get_project_context",
                "description": "Return ready-to-inject project context, taxonomy summary, relation graph summary, and debugging notes.",
                "inputSchema": {
                    "type": "object",
                    "required": ["project_id"],
                    "properties": {
                        "project_id": { "type": "string" }
                    }
                }
            },
            {
                "name": "reflect_memory",
                "description": "Reflect on a task result, reinforce good outcomes, surface avoid-patterns, and return interactive save confirmation metadata.",
                "inputSchema": {
                    "type": "object",
                    "required": ["project_id", "task_result", "outcome"],
                    "properties": {
                        "project_id": { "type": "string" },
                        "task_result": { "type": "string" },
                        "outcome": { "type": "string" },
                        "metadata": { "type": "object" }
                    }
                }
            },
            {
                "name": "feedback_memory",
                "description": "Apply explicit success or failure feedback to an existing memory so Memovyn can reinforce good patterns and punish repeated mistakes.",
                "inputSchema": {
                    "type": "object",
                    "required": ["memory_id", "outcome"],
                    "properties": {
                        "memory_id": { "type": "string" },
                        "outcome": { "type": "string" },
                        "repeated_mistake": { "type": "boolean" },
                        "weight": { "type": "number" },
                        "cross_project_influence": { "type": "boolean" },
                        "avoid_patterns": { "type": "array", "items": { "type": "string" } },
                        "note": { "type": "string" }
                    }
                }
            },
            {
                "name": "archive_memory",
                "description": "Archive a memory so it leaves the active retrieval stream but remains inspectable and present in history.",
                "inputSchema": {
                    "type": "object",
                    "required": ["memory_id"],
                    "properties": {
                        "memory_id": { "type": "string" }
                    }
                }
            },
            {
                "name": "get_project_analytics",
                "description": "Return visible analytics showing what Memovyn remembers, how often it is recalled, conflict heatmaps, and estimated token savings.",
                "inputSchema": {
                    "type": "object",
                    "required": ["project_id"],
                    "properties": {
                        "project_id": { "type": "string" }
                    }
                }
            }
        ]
    })
}

async fn handle_tool_call(
    app: Arc<Memovyn>,
    params: Option<Value>,
) -> std::result::Result<Value, JsonRpcError> {
    let params = params.unwrap_or_default();
    let tool_name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcError {
            code: -32602,
            message: "missing tools/call name".to_string(),
        })?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    let payload = match tool_name {
        "add_memory" => {
            let request: AddMemoryRequest =
                serde_json::from_value(arguments).map_err(invalid_params)?;
            serde_json::to_value(app.add_memory(request).map_err(internal_error)?)
                .map_err(invalid_params)?
        }
        "search_memories" => {
            let request: SearchRequest =
                serde_json::from_value(arguments).map_err(invalid_params)?;
            serde_json::to_value(app.search_memories(request).map_err(internal_error)?)
                .map_err(invalid_params)?
        }
        "get_project_context" => {
            let project_id = arguments
                .get("project_id")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError {
                    code: -32602,
                    message: "missing project_id".to_string(),
                })?;
            serde_json::to_value(
                app.get_project_context(project_id)
                    .map_err(internal_error)?,
            )
            .map_err(invalid_params)?
        }
        "reflect_memory" => {
            let request: ReflectionRequest =
                serde_json::from_value(arguments).map_err(invalid_params)?;
            serde_json::to_value(app.reflect_memory(request).map_err(internal_error)?)
                .map_err(invalid_params)?
        }
        "feedback_memory" => {
            let request: FeedbackRequest =
                serde_json::from_value(arguments).map_err(invalid_params)?;
            serde_json::to_value(app.feedback_memory(request).map_err(internal_error)?)
                .map_err(invalid_params)?
        }
        "archive_memory" => {
            let request: ArchiveRequest =
                serde_json::from_value(arguments).map_err(invalid_params)?;
            serde_json::to_value(app.archive_memory(request).map_err(internal_error)?)
                .map_err(invalid_params)?
        }
        "get_project_analytics" => {
            let project_id = arguments
                .get("project_id")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError {
                    code: -32602,
                    message: "missing project_id".to_string(),
                })?;
            serde_json::to_value(app.analytics(project_id).map_err(internal_error)?)
                .map_err(invalid_params)?
        }
        _ => {
            return Err(JsonRpcError {
                code: -32601,
                message: format!("unknown tool {tool_name}"),
            });
        }
    };

    Ok(serde_json::json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
            },
            {
                "type": "resource",
                "mimeType": "application/json",
                "data": payload
            }
        ]
    }))
}

fn invalid_params(error: impl std::fmt::Display) -> JsonRpcError {
    JsonRpcError {
        code: -32602,
        message: error.to_string(),
    }
}

fn internal_error(error: impl std::fmt::Display) -> JsonRpcError {
    JsonRpcError {
        code: -32000,
        message: error.to_string(),
    }
}
