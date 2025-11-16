use crate::config::Config;
use crate::workflow::{StreamCallback, WorkflowEngine, WorkflowExecutionDetails};
use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::{
        sse::{Event, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use futures::{stream, StreamExt};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::UnboundedReceiverStream;

use std::convert::Infallible;
use tower_http::cors::CorsLayer;

type SharedState = Arc<AppState>;

const STREAM_CHUNK_SIZE: usize = 120;

pub struct AppState {
    config: Config,
    workflow_engine: WorkflowEngine,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateRequest {
    pub model: Option<String>,
    pub prompt: String,
    pub stream: Option<bool>,
    pub include_workflow: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: Option<String>,
    pub messages: Vec<Message>,
    pub stream: Option<bool>,
    pub include_workflow: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum PromptInput {
    Single(String),
    Multiple(Vec<String>),
}

impl PromptInput {
    fn into_prompt(self) -> String {
        match self {
            PromptInput::Single(s) => s,
            PromptInput::Multiple(items) => items.join("\n"),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CompletionRequest {
    pub model: Option<String>,
    pub prompt: PromptInput,
    pub stream: Option<bool>,
    pub include_workflow: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateResponse {
    pub model: String,
    pub created_at: String,
    pub response: String,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow: Option<WorkflowExecutionDetails>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatResponse {
    pub model: String,
    pub created_at: String,
    pub message: Message,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow: Option<WorkflowExecutionDetails>,
}

fn chunk_text(content: &str, max_len: usize) -> Vec<String> {
    if content.is_empty() {
        return vec![String::new()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;

    for ch in content.chars() {
        current_len += ch.len_utf8();
        current.push(ch);

        if ch == '\n' || current_len >= max_len {
            chunks.push(current);
            current = String::new();
            current_len = 0;
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn build_prompt_from_messages(messages: &[Message]) -> String {
    messages
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n")
}

async fn execute_workflow(
    state: &AppState,
    prompt: String,
    include_workflow: bool,
    stream: Option<StreamCallback>,
) -> Result<(String, Option<WorkflowExecutionDetails>), AppError> {
    if include_workflow {
        let result = state
            .workflow_engine
            .process_with_details_stream(prompt, stream)
            .await?;
        Ok((result.final_response, Some(result.execution_details)))
    } else {
        let response = state
            .workflow_engine
            .process_with_stream(prompt, stream)
            .await?;
        Ok((response, None))
    }
}

fn insert_workflow_field(
    payload: &mut serde_json::Value,
    details: &Option<WorkflowExecutionDetails>,
) {
    if let Some(details) = details {
        if let serde_json::Value::Object(map) = payload {
            if let Ok(value) = serde_json::to_value(details) {
                map.insert("workflow".to_string(), value);
            }
        }
    }
}

pub async fn start_server(config: Arc<Config>) -> Result<()> {
    let workflow_engine = WorkflowEngine::new((*config).clone());

    let state = Arc::new(AppState {
        config: (*config).clone(),
        workflow_engine,
    });

    let app = Router::new()
        .route("/", get(health_check))
        // API v0 style
        .route("/api/generate", post(generate))
        .route("/api/chat", post(chat))
        .route("/api/tags", get(list_models))
        // API v1 alias (same handlers)
        .route("/v1/generate", post(generate))
        .route("/v1/chat", post(chat))
        // OpenAI-compatible endpoints (for Cherry Studio and similar clients)
        .route("/v1/chat/completions", post(openai_chat_completions))
        .route("/v1/completions", post(openai_completions))
        .route("/v1/models", get(list_models_openai))
        .route("/v1/tags", get(list_models))
        .route("/v1/responses", post(responses))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("{}:{}", config.server.host, config.server.port);

    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("Chorus server listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "service": "Chorus",
        "version": "0.1.0"
    }))
}

async fn generate(
    State(state): State<SharedState>,
    Json(req): Json<GenerateRequest>,
) -> Result<Response, AppError> {
    let GenerateRequest {
        model,
        prompt,
        stream,
        include_workflow,
    } = req;

    tracing::info!(
        "Received generate request, stream: {:?}, include_workflow: {:?}",
        stream,
        include_workflow
    );

    let model_name = model.unwrap_or_else(|| "chorus".to_string());
    let stream_enabled = stream.unwrap_or(false);
    let include_workflow_details = include_workflow.unwrap_or(false);

    if stream_enabled {
        let created_at = chrono::Utc::now().to_rfc3339();
        let (chunk_tx, chunk_rx) = mpsc::unbounded_channel::<String>();
        let (result_tx, result_rx) = oneshot::channel();

        let state_clone = state.clone();
        tokio::spawn(async move {
            let result = execute_workflow(
                &state_clone,
                prompt,
                include_workflow_details,
                Some(chunk_tx.clone()),
            )
            .await;
            drop(chunk_tx);
            let _ = result_tx.send(result);
        });

        let chunk_stream =
            UnboundedReceiverStream::new(chunk_rx).flat_map({
                let model_name = model_name.clone();
                let created_at = created_at.clone();
                move |segment| {
                    let model_name = model_name.clone();
                    let created_at = created_at.clone();
                    let pieces = if segment.is_empty() {
                        vec![String::new()]
                    } else {
                        chunk_text(&segment, STREAM_CHUNK_SIZE)
                    };
                    stream::iter(pieces.into_iter().map(
                        move |piece| -> Result<Event, Infallible> {
                            let payload = serde_json::json!({
                                "model": model_name,
                                "created_at": created_at,
                                "response": piece,
                                "done": false,
                            });
                            Ok(Event::default().json_data(payload).unwrap())
                        },
                    ))
                }
            });

        let completion_stream = futures::stream::once({
            let model_name = model_name.clone();
            let created_at = created_at.clone();
            async move {
                match result_rx.await {
                    Ok(Ok((_, workflow_details))) => {
                        let mut payload = serde_json::json!({
                            "model": model_name.clone(),
                            "created_at": created_at.clone(),
                            "response": "",
                            "done": true,
                        });
                        insert_workflow_field(&mut payload, &workflow_details);
                        Ok::<Event, Infallible>(Event::default().json_data(payload).unwrap())
                    }
                    Ok(Err(err)) => {
                        let error_message = err.error.to_string();
                        let payload = serde_json::json!({
                            "model": model_name.clone(),
                            "created_at": created_at.clone(),
                            "done": true,
                            "error": error_message,
                        });
                        Ok::<Event, Infallible>(Event::default().json_data(payload).unwrap())
                    }
                    Err(_) => {
                        let payload = serde_json::json!({
                            "model": model_name.clone(),
                            "created_at": created_at.clone(),
                            "done": true,
                            "error": "stream cancelled",
                        });
                        Ok::<Event, Infallible>(Event::default().json_data(payload).unwrap())
                    }
                }
            }
        });

        let sse_stream = chunk_stream.chain(completion_stream);
        return Ok(Sse::new(sse_stream).into_response());
    }

    let (response_text, workflow_details) =
        execute_workflow(&state, prompt, include_workflow_details, None).await?;

    Ok(Json(GenerateResponse {
        model: model_name,
        created_at: chrono::Utc::now().to_rfc3339(),
        response: response_text,
        done: true,
        workflow: workflow_details,
    })
    .into_response())
}

async fn chat(
    State(state): State<SharedState>,
    Json(req): Json<ChatRequest>,
) -> Result<Response, AppError> {
    tracing::info!(
        "Received chat request with {} messages, stream: {:?}, include_workflow: {:?}",
        req.messages.len(),
        req.stream,
        req.include_workflow
    );

    let prompt = build_prompt_from_messages(&req.messages);

    let model_name = req.model.unwrap_or_else(|| "chorus".to_string());
    let stream_enabled = req.stream.unwrap_or(false);
    let include_workflow_details = req.include_workflow.unwrap_or(false);

    if stream_enabled {
        let created_at = chrono::Utc::now().to_rfc3339();
        let (chunk_tx, chunk_rx) = mpsc::unbounded_channel::<String>();
        let (result_tx, result_rx) = oneshot::channel();

        let state_clone = state.clone();
        tokio::spawn(async move {
            let result = execute_workflow(
                &state_clone,
                prompt,
                include_workflow_details,
                Some(chunk_tx.clone()),
            )
            .await;
            drop(chunk_tx);
            let _ = result_tx.send(result);
        });

        let chunk_stream =
            UnboundedReceiverStream::new(chunk_rx).flat_map({
                let model_name = model_name.clone();
                let created_at = created_at.clone();
                move |segment| {
                    let model_name = model_name.clone();
                    let created_at = created_at.clone();
                    let pieces = if segment.is_empty() {
                        vec![String::new()]
                    } else {
                        chunk_text(&segment, STREAM_CHUNK_SIZE)
                    };
                    stream::iter(pieces.into_iter().map(
                        move |piece| -> Result<Event, Infallible> {
                            let payload = serde_json::json!({
                                "model": model_name,
                                "created_at": created_at,
                                "message": {
                                    "role": "assistant",
                                    "content": piece,
                                },
                                "done": false,
                            });
                            Ok(Event::default().json_data(payload).unwrap())
                        },
                    ))
                }
            });

        let completion_stream = futures::stream::once({
            let model_name = model_name.clone();
            let created_at = created_at.clone();
            async move {
                match result_rx.await {
                    Ok(Ok((_, workflow_details))) => {
                        let mut payload = serde_json::json!({
                            "model": model_name.clone(),
                            "created_at": created_at.clone(),
                            "message": {
                                "role": "assistant",
                                "content": "",
                            },
                            "done": true,
                        });
                        insert_workflow_field(&mut payload, &workflow_details);
                        Ok::<Event, Infallible>(Event::default().json_data(payload).unwrap())
                    }
                    Ok(Err(err)) => {
                        let error_message = err.error.to_string();
                        let payload = serde_json::json!({
                            "model": model_name.clone(),
                            "created_at": created_at.clone(),
                            "message": {
                                "role": "assistant",
                                "content": "",
                            },
                            "done": true,
                            "error": error_message,
                        });
                        Ok::<Event, Infallible>(Event::default().json_data(payload).unwrap())
                    }
                    Err(_) => {
                        let payload = serde_json::json!({
                            "model": model_name.clone(),
                            "created_at": created_at.clone(),
                            "message": {
                                "role": "assistant",
                                "content": "",
                            },
                            "done": true,
                            "error": "stream cancelled",
                        });
                        Ok::<Event, Infallible>(Event::default().json_data(payload).unwrap())
                    }
                }
            }
        });

        let sse_stream = chunk_stream.chain(completion_stream);
        return Ok(Sse::new(sse_stream).into_response());
    }

    let (response_text, workflow_details) =
        execute_workflow(&state, prompt, include_workflow_details, None).await?;

    Ok(Json(ChatResponse {
        model: model_name,
        created_at: chrono::Utc::now().to_rfc3339(),
        message: Message {
            role: "assistant".to_string(),
            content: response_text,
        },
        done: true,
        workflow: workflow_details,
    })
    .into_response())
}

// OpenAI Chat Completions compatible endpoint
async fn openai_chat_completions(
    State(state): State<SharedState>,
    Json(req): Json<ChatRequest>,
) -> Result<Response, AppError> {
    tracing::info!(
        "Received OpenAI chat.completions request with {} messages, stream: {:?}",
        req.messages.len(),
        req.stream
    );

    let prompt = build_prompt_from_messages(&req.messages);

    let model_name = req.model.unwrap_or_else(|| "chorus".to_string());
    let stream_enabled = req.stream.unwrap_or(false);
    let include_workflow_details = req.include_workflow.unwrap_or(false);

    if stream_enabled {
        let now = chrono::Utc::now();
        let created = now.timestamp();
        let id = format!("chatcmpl_{}", now.timestamp_millis());
        let (chunk_tx, chunk_rx) = mpsc::unbounded_channel::<String>();
        let (result_tx, result_rx) = oneshot::channel();

        let state_clone = state.clone();
        tokio::spawn(async move {
            let result = execute_workflow(
                &state_clone,
                prompt,
                include_workflow_details,
                Some(chunk_tx.clone()),
            )
            .await;
            drop(chunk_tx);
            let _ = result_tx.send(result);
        });

        let initial_stream = futures::stream::once({
            let id = id.clone();
            let model_name = model_name.clone();
            async move {
                let payload = serde_json::json!({
                    "id": id,
                    "object": "chat.completion.chunk",
                    "created": created,
                    "model": model_name,
                    "choices": [ {
                        "index": 0,
                        "delta": { "role": "assistant" },
                        "finish_reason": serde_json::Value::Null
                    } ],
                });
                Ok::<Event, Infallible>(Event::default().json_data(payload).unwrap())
            }
        });

        let chunk_stream =
            UnboundedReceiverStream::new(chunk_rx).flat_map({
                let id = id.clone();
                let model_name = model_name.clone();
                move |segment| {
                    let id = id.clone();
                    let model_name = model_name.clone();
                    let pieces = if segment.is_empty() {
                        vec![String::new()]
                    } else {
                        chunk_text(&segment, STREAM_CHUNK_SIZE)
                    };
                    stream::iter(pieces.into_iter().map(
                        move |piece| -> Result<Event, Infallible> {
                            let payload = serde_json::json!({
                                "id": id.clone(),
                                "object": "chat.completion.chunk",
                                "created": created,
                                "model": model_name.clone(),
                                "choices": [ {
                                    "index": 0,
                                    "delta": { "content": piece },
                                    "finish_reason": serde_json::Value::Null
                                } ]
                            });
                            Ok(Event::default().json_data(payload).unwrap())
                        },
                    ))
                }
            });

        let completion_stream = futures::stream::once({
            let id = id.clone();
            let model_name = model_name.clone();
            async move {
                match result_rx.await {
                    Ok(Ok((_, workflow_details))) => {
                        let mut payload = serde_json::json!({
                            "id": id.clone(),
                            "object": "chat.completion.chunk",
                            "created": created,
                            "model": model_name.clone(),
                            "choices": [ {
                                "index": 0,
                                "delta": serde_json::json!({}),
                                "finish_reason": "stop"
                            } ]
                        });
                        insert_workflow_field(&mut payload, &workflow_details);
                        Ok::<Event, Infallible>(Event::default().json_data(payload).unwrap())
                    }
                    Ok(Err(err)) => {
                        let error_message = err.error.to_string();
                        let payload = serde_json::json!({
                            "id": id.clone(),
                            "object": "chat.completion.chunk",
                            "created": created,
                            "model": model_name.clone(),
                            "choices": [ {
                                "index": 0,
                                "delta": serde_json::json!({}),
                                "finish_reason": "error"
                            } ],
                            "error": error_message,
                        });
                        Ok::<Event, Infallible>(Event::default().json_data(payload).unwrap())
                    }
                    Err(_) => {
                        let payload = serde_json::json!({
                            "id": id.clone(),
                            "object": "chat.completion.chunk",
                            "created": created,
                            "model": model_name.clone(),
                            "choices": [ {
                                "index": 0,
                                "delta": serde_json::json!({}),
                                "finish_reason": "error"
                            } ],
                            "error": "stream cancelled",
                        });
                        Ok::<Event, Infallible>(Event::default().json_data(payload).unwrap())
                    }
                }
            }
        });

        let done_stream = futures::stream::once(async {
            Ok::<Event, Infallible>(Event::default().data("[DONE]"))
        });

        let sse_stream = initial_stream
            .chain(chunk_stream)
            .chain(completion_stream)
            .chain(done_stream);

        return Ok(Sse::new(sse_stream).into_response());
    }

    let (response_text, workflow_details) =
        execute_workflow(&state, prompt, include_workflow_details, None).await?;
    let now = chrono::Utc::now();
    let created = now.timestamp();
    let id = format!("chatcmpl_{}", now.timestamp_millis());

    let body = serde_json::json!({
        "id": id,
        "object": "chat.completion",
        "created": created,
        "model": model_name,
        "choices": [ {
            "index": 0,
            "message": {
                "role": "assistant",
                "content": response_text
            },
            "finish_reason": "stop"
        } ],
        "workflow": workflow_details,
    });
    Ok(Json(body).into_response())
}

async fn openai_completions(
    State(state): State<SharedState>,
    Json(req): Json<CompletionRequest>,
) -> Result<Response, AppError> {
    tracing::info!(
        "Received OpenAI completions request, stream: {:?}",
        req.stream
    );

    let prompt = req.prompt.into_prompt();
    let model_name = req.model.unwrap_or_else(|| "chorus".to_string());
    let stream_enabled = req.stream.unwrap_or(false);
    let include_workflow_details = req.include_workflow.unwrap_or(false);

    if stream_enabled {
        let now = chrono::Utc::now();
        let created = now.timestamp();
        let id = format!("cmpl_{}", now.timestamp_millis());
        let (chunk_tx, chunk_rx) = mpsc::unbounded_channel::<String>();
        let (result_tx, result_rx) = oneshot::channel();

        let state_clone = state.clone();
        tokio::spawn(async move {
            let result = execute_workflow(
                &state_clone,
                prompt,
                include_workflow_details,
                Some(chunk_tx.clone()),
            )
            .await;
            drop(chunk_tx);
            let _ = result_tx.send(result);
        });

        let chunk_stream =
            UnboundedReceiverStream::new(chunk_rx).flat_map({
                let id = id.clone();
                let model_name = model_name.clone();
                move |segment| {
                    let id = id.clone();
                    let model_name = model_name.clone();
                    let pieces = if segment.is_empty() {
                        vec![String::new()]
                    } else {
                        chunk_text(&segment, STREAM_CHUNK_SIZE)
                    };
                    stream::iter(pieces.into_iter().map(
                        move |piece| -> Result<Event, Infallible> {
                            let payload = serde_json::json!({
                                "id": id.clone(),
                                "object": "text_completion",
                                "created": created,
                                "model": model_name.clone(),
                                "choices": [ {
                                    "text": piece,
                                    "index": 0,
                                    "logprobs": serde_json::Value::Null,
                                    "finish_reason": serde_json::Value::Null
                                } ]
                            });
                            Ok(Event::default().json_data(payload).unwrap())
                        },
                    ))
                }
            });

        let completion_stream = futures::stream::once({
            let id = id.clone();
            let model_name = model_name.clone();
            async move {
                match result_rx.await {
                    Ok(Ok((_, workflow_details))) => {
                        let mut payload = serde_json::json!({
                            "id": id.clone(),
                            "object": "text_completion",
                            "created": created,
                            "model": model_name.clone(),
                            "choices": [ {
                                "text": "",
                                "index": 0,
                                "logprobs": serde_json::Value::Null,
                                "finish_reason": "stop"
                            } ]
                        });
                        insert_workflow_field(&mut payload, &workflow_details);
                        Ok::<Event, Infallible>(Event::default().json_data(payload).unwrap())
                    }
                    Ok(Err(err)) => {
                        let error_message = err.error.to_string();
                        let payload = serde_json::json!({
                            "id": id.clone(),
                            "object": "text_completion",
                            "created": created,
                            "model": model_name.clone(),
                            "choices": [ {
                                "text": "",
                                "index": 0,
                                "logprobs": serde_json::Value::Null,
                                "finish_reason": "error"
                            } ],
                            "error": error_message,
                        });
                        Ok::<Event, Infallible>(Event::default().json_data(payload).unwrap())
                    }
                    Err(_) => {
                        let payload = serde_json::json!({
                            "id": id.clone(),
                            "object": "text_completion",
                            "created": created,
                            "model": model_name.clone(),
                            "choices": [ {
                                "text": "",
                                "index": 0,
                                "logprobs": serde_json::Value::Null,
                                "finish_reason": "error"
                            } ],
                            "error": "stream cancelled",
                        });
                        Ok::<Event, Infallible>(Event::default().json_data(payload).unwrap())
                    }
                }
            }
        });

        let done_stream = futures::stream::once(async {
            Ok::<Event, Infallible>(Event::default().data("[DONE]"))
        });

        let sse_stream = chunk_stream.chain(completion_stream).chain(done_stream);
        return Ok(Sse::new(sse_stream).into_response());
    }

    let (response_text, workflow_details) =
        execute_workflow(&state, prompt, include_workflow_details, None).await?;

    let now = chrono::Utc::now();
    let created = now.timestamp();
    let id = format!("cmpl_{}", now.timestamp_millis());

    let body = serde_json::json!({
        "id": id,
        "object": "text_completion",
        "created": created,
        "model": model_name,
        "choices": [ {
            "text": response_text,
            "index": 0,
            "logprobs": serde_json::Value::Null,
            "finish_reason": "stop"
        } ],
        "workflow": workflow_details,
    });

    Ok(Json(body).into_response())
}

fn extract_text_value(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Array(items) => {
            let mut parts = Vec::new();
            for item in items {
                if let Some(text) = extract_text_value(item) {
                    if !text.is_empty() {
                        parts.push(text);
                    }
                }
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        Value::Object(map) => {
            for key in ["text", "input_text", "value", "output_text"] {
                if let Some(Value::String(s)) = map.get(key) {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
            }
            if let Some(content) = map.get("content") {
                if let Some(text) = extract_text_value(content) {
                    if !text.is_empty() {
                        return Some(text);
                    }
                }
            }
            if let Some(parts) = map.get("parts") {
                if let Some(text) = extract_text_value(parts) {
                    if !text.is_empty() {
                        return Some(text);
                    }
                }
            }
            if let Some(messages) = map.get("messages") {
                if let Some(text) = extract_text_value(messages) {
                    if !text.is_empty() {
                        return Some(text);
                    }
                }
            }
            None
        }
    }
}

fn extract_message_text(value: &Value) -> Option<String> {
    if let Value::Object(map) = value {
        let role = match map.get("role").and_then(|v| v.as_str()) {
            Some(role) => role,
            None => return None,
        };
        if let Some(content) = map.get("content") {
            if let Some(text) = extract_text_value(content) {
                if text.is_empty() {
                    return None;
                }
                return Some(format!("{}: {}", role, text));
            }
        }
        if let Some(Value::String(text)) = map.get("text") {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return None;
            }
            return Some(format!("{}: {}", role, trimmed));
        }
    }
    None
}

fn extract_prompt_from_responses_body(payload: &Value) -> Option<String> {
    let mut segments: Vec<String> = Vec::new();

    if let Some(Value::String(instr)) = payload.get("instructions") {
        let trimmed = instr.trim();
        if !trimmed.is_empty() {
            segments.push(format!("system: {}", trimmed));
        }
    }

    if let Some(Value::Array(messages)) = payload.get("messages") {
        for msg in messages {
            if let Some(text) = extract_message_text(msg) {
                segments.push(text);
            } else if let Some(text) = extract_text_value(msg) {
                if !text.is_empty() {
                    segments.push(text);
                }
            }
        }
    }

    if let Some(input) = payload.get("input") {
        match input {
            Value::String(s) => {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    segments.push(trimmed.to_string());
                }
            }
            Value::Array(items) => {
                for item in items {
                    if let Some(text) = extract_message_text(item) {
                        segments.push(text);
                    } else if let Some(text) = extract_text_value(item) {
                        if !text.is_empty() {
                            segments.push(text);
                        }
                    }
                }
            }
            Value::Object(_) => {
                if let Some(text) = extract_message_text(input) {
                    segments.push(text);
                } else if let Some(text) = extract_text_value(input) {
                    if !text.is_empty() {
                        segments.push(text);
                    }
                }
            }
            _ => {}
        }
    }

    for key in ["prompt", "input_text"] {
        if let Some(value) = payload.get(key) {
            if let Some(text) = extract_text_value(value) {
                if !text.is_empty() {
                    segments.push(text);
                }
            }
        }
    }

    if segments.is_empty() {
        None
    } else {
        Some(segments.join("\n"))
    }
}

async fn responses(
    State(state): State<SharedState>,
    Json(req): Json<Value>,
) -> Result<Response, AppError> {
    let model_name = req
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("chorus")
        .to_string();

    let stream_requested = matches!(
        req.get("stream"),
        Some(Value::Bool(true)) | Some(Value::Object(_))
    );

    let include_workflow_details = req
        .get("include_workflow")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    tracing::info!(
        "Received v1/responses request for model {}, stream: {}, include_workflow: {}",
        model_name.as_str(),
        stream_requested,
        include_workflow_details
    );

    let prompt = extract_prompt_from_responses_body(&req).ok_or_else(|| {
        AppError::bad_request(anyhow::anyhow!(
            "invalid request: missing input/messages/prompt/instructions"
        ))
    })?;

    let prompt_len = prompt.len();

    if stream_requested {
        tracing::warn!(
            "Responses stream requested but streaming is not yet implemented; returning single response payload"
        );
    }

    let (response_text, workflow_details) =
        execute_workflow(&state, prompt, include_workflow_details, None).await?;

    tracing::debug!(
        "Generated responses payload (prompt {} bytes, response {} bytes)",
        prompt_len,
        response_text.len()
    );

    let now = chrono::Utc::now();
    let resp = serde_json::json!({
        "id": format!("resp_{}", now.timestamp_millis()),
        "object": "response",
        "created": now.timestamp(),
        "model": model_name,
        "status": "completed",
        "output": [
            {
                "id": format!("msg_{}", now.timestamp_millis()),
                "type": "message",
                "role": "assistant",
                "content": [ { "type": "output_text", "text": response_text } ]
            }
        ],
        "output_text": response_text,
        "workflow": workflow_details,
    });

    Ok(Json(resp).into_response())
}

async fn list_models_openai(State(state): State<SharedState>) -> impl IntoResponse {
    let created = chrono::Utc::now().timestamp();
    let data: Vec<_> = state
        .config
        .models
        .iter()
        .map(|m| {
            serde_json::json!({
                "id": m.name,
                "object": "model",
                "created": created,
                "owned_by": "chorus",
                "permission": Vec::<serde_json::Value>::new(),
            })
        })
        .collect();

    Json(serde_json::json!({
        "object": "list",
        "data": data
    }))
}

async fn list_models(State(state): State<SharedState>) -> impl IntoResponse {
    let models: Vec<_> = state
        .config
        .models
        .iter()
        .map(|m| {
            serde_json::json!({
                "name": m.name,
                "model": m.name,
                "modified_at": chrono::Utc::now().to_rfc3339(),
            })
        })
        .collect();

    Json(serde_json::json!({
        "models": models
    }))
}

#[cfg(test)]
mod responses_tests {
    use super::extract_prompt_from_responses_body;
    use serde_json::json;

    #[test]
    fn extract_prompt_prefers_instructions_and_input() {
        let payload = json!({
            "instructions": "Be helpful",
            "input": "Say hello"
        });
        assert_eq!(
            extract_prompt_from_responses_body(&payload).unwrap(),
            "system: Be helpful\nSay hello"
        );
    }

    #[test]
    fn extract_prompt_handles_message_arrays() {
        let payload = json!({
            "input": [
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": "Hi there"}
                    ]
                },
                {
                    "role": "assistant",
                    "content": [
                        {"type": "text", "text": "Hello!"}
                    ]
                }
            ]
        });
        assert_eq!(
            extract_prompt_from_responses_body(&payload).unwrap(),
            "user: Hi there\nassistant: Hello!"
        );
    }

    #[test]
    fn extract_prompt_handles_messages_field_with_string_content() {
        let payload = json!({
            "messages": [
                {"role": "user", "content": "ping"}
            ]
        });
        assert_eq!(
            extract_prompt_from_responses_body(&payload).unwrap(),
            "user: ping"
        );
    }

    #[test]
    fn extract_prompt_handles_text_blocks() {
        let payload = json!({
            "input": [
                {"type": "text", "text": "First"},
                {"type": "input_text", "text": "Second"}
            ]
        });
        assert_eq!(
            extract_prompt_from_responses_body(&payload).unwrap(),
            "First\nSecond"
        );
    }

    #[test]
    fn extract_prompt_handles_prompt_array() {
        let payload = json!({
            "prompt": [
                "First",
                { "text": "Second" },
                { "content": [{ "text": "Third" }] }
            ]
        });
        assert_eq!(
            extract_prompt_from_responses_body(&payload).unwrap(),
            "First\nSecond\nThird"
        );
    }

    #[test]
    fn extract_prompt_returns_none_when_empty() {
        assert!(extract_prompt_from_responses_body(&json!({})).is_none());
    }

    #[test]
    fn app_error_bad_request_uses_400_status() {
        use axum::http::StatusCode;
        use axum::response::IntoResponse;

        let response = super::AppError::bad_request(anyhow::anyhow!("bad request")).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

// 错误处理
pub struct AppError {
    status: StatusCode,
    error: anyhow::Error,
}

impl AppError {
    pub fn new(status: StatusCode, err: impl Into<anyhow::Error>) -> Self {
        Self {
            status,
            error: err.into(),
        }
    }

    pub fn bad_request(err: impl Into<anyhow::Error>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!(
            status = %self.status,
            error = %self.error,
            "Application error"
        );

        (
            self.status,
            Json(serde_json::json!({
                "error": self.error.to_string()
            })),
        )
            .into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, err)
    }
}
