use crate::config::Config;
use crate::workflow::WorkflowEngine;
use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response, sse::{Event, Sse}},
    routing::{get, post},
    Json, Router,
};
use futures::stream;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::convert::Infallible;
use tower_http::cors::CorsLayer;

type SharedState = Arc<AppState>;

pub struct AppState {
    config: Config,
    workflow_engine: WorkflowEngine,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateRequest {
    pub model: Option<String>,
    pub prompt: String,
    pub stream: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: Option<String>,
    pub messages: Vec<Message>,
    pub stream: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateResponse {
    pub model: String,
    pub created_at: String,
    pub response: String,
    pub done: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatResponse {
    pub model: String,
    pub created_at: String,
    pub message: Message,
    pub done: bool,
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
        // OpenAI-compatible Chat Completions (for Cherry Studio)
        .route("/v1/chat/completions", post(openai_chat_completions))
        .route("/v1/tags", get(list_models))
        .route("/v1/responses", post(responses))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!(
        "{}:{}",
        config.server.host, config.server.port
    );

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
) -> Result<Json<GenerateResponse>, AppError> {
    tracing::info!("Received generate request");
    let response = state.workflow_engine.process(req.prompt).await?;

    let model_name = req.model.unwrap_or_else(|| "chorus".to_string());

    Ok(Json(GenerateResponse {
        model: model_name,
        created_at: chrono::Utc::now().to_rfc3339(),
        response,
        done: true,
    }))
}

async fn chat(
    State(state): State<SharedState>,
    Json(req): Json<ChatRequest>,
) -> Result<Response, AppError> {
    tracing::info!("Received chat request with {} messages, stream: {:?}", req.messages.len(), req.stream);

    // 将消息转换为单个prompt
    let prompt = req
        .messages
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n");

    let response_text = state.workflow_engine.process(prompt).await?;
    let model_name = req.model.unwrap_or_else(|| "chorus".to_string());

    // 如果请求流式响应
    if req.stream.unwrap_or(false) {
        let stream = stream::iter(vec![
            Ok::<_, Infallible>(Event::default().json_data(serde_json::json!({
                "model": model_name,
                "created_at": chrono::Utc::now().to_rfc3339(),
                "message": {
                    "role": "assistant",
                    "content": response_text
                },
                "done": false,
            })).unwrap()),
            Ok(Event::default().json_data(serde_json::json!({
                "model": model_name,
                "created_at": chrono::Utc::now().to_rfc3339(),
                "message": {
                    "role": "assistant",
                    "content": ""
                },
                "done": true,
            })).unwrap()),
        ]);
        
        Ok(Sse::new(stream).into_response())
    } else {
        // 非流式响应
        Ok(Json(ChatResponse {
            model: model_name,
            created_at: chrono::Utc::now().to_rfc3339(),
            message: Message {
                role: "assistant".to_string(),
                content: response_text,
            },
            done: true,
        }).into_response())
    }
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

    let prompt = req
        .messages
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n");

    let response_text = state.workflow_engine.process(prompt).await?;
    let model_name = req.model.unwrap_or_else(|| "chorus".to_string());

    let now = chrono::Utc::now();
    let created = now.timestamp();
    let id = format!("chatcmpl_{}", now.timestamp_millis());

    if req.stream.unwrap_or(false) {
        let stream = stream::iter(vec![
            // send role delta first
            Ok::<_, Infallible>(Event::default().json_data(serde_json::json!({
                "id": id,
                "object": "chat.completion.chunk",
                "created": created,
                "model": model_name,
                "choices": [ {
                    "index": 0,
                    "delta": { "role": "assistant" },
                    "finish_reason": serde_json::Value::Null
                } ]
            })).unwrap()),
            // send content delta
            Ok(Event::default().json_data(serde_json::json!({
                "id": id,
                "object": "chat.completion.chunk",
                "created": created,
                "model": model_name,
                "choices": [ {
                    "index": 0,
                    "delta": { "content": response_text },
                    "finish_reason": serde_json::Value::Null
                } ]
            })).unwrap()),
            // send finish
            Ok(Event::default().json_data(serde_json::json!({
                "id": id,
                "object": "chat.completion.chunk",
                "created": created,
                "model": model_name,
                "choices": [ {
                    "index": 0,
                    "delta": {},
                    "finish_reason": "stop"
                } ]
            })).unwrap()),
            // final sentinel
            Ok(Event::default().data("[DONE]")),
        ]);

        Ok(Sse::new(stream).into_response())
    } else {
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
            } ]
        });
        Ok(Json(body).into_response())
    }
}

async fn responses(
    State(state): State<SharedState>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    tracing::info!("Received v1/responses request");

    let model_name = req
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("chorus")
        .to_string();

    // Build prompt from messages | input | prompt
    let build_from_blocks = |blocks: &Vec<serde_json::Value>| -> String {
        let mut out = String::new();
        for b in blocks {
            if let Some(s) = b.as_str() {
                if !out.is_empty() { out.push_str("\n"); }
                out.push_str(s);
                continue;
            }
            if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                if !out.is_empty() { out.push_str("\n"); }
                out.push_str(t);
                continue;
            }
            if let Some(t) = b
                .get("content")
                .and_then(|c| c.as_str())
            {
                if !out.is_empty() { out.push_str("\n"); }
                out.push_str(t);
                continue;
            }
            if let Some(arr) = b.get("content").and_then(|c| c.as_array()) {
                let part = arr
                    .iter()
                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()).map(|s| s.to_string()))
                    .collect::<Vec<_>>()
                    .join("\n");
                if !part.is_empty() {
                    if !out.is_empty() { out.push_str("\n"); }
                    out.push_str(&part);
                }
            }
        }
        out
    };

    let prompt = if let Some(messages) = req.get("messages").and_then(|v| v.as_array()) {
        let mut parts = Vec::new();
        for m in messages {
            let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("user");
            if let Some(s) = m.get("content").and_then(|c| c.as_str()) {
                parts.push(format!("{}: {}", role, s));
            } else if let Some(arr) = m.get("content").and_then(|c| c.as_array()) {
                let text = build_from_blocks(arr);
                if !text.is_empty() { parts.push(format!("{}: {}", role, text)); }
            }
        }
        parts.join("\n")
    } else if let Some(input) = req.get("input") {
        if let Some(s) = input.as_str() {
            s.to_string()
        } else if let Some(arr) = input.as_array() {
            build_from_blocks(arr)
        } else {
            String::new()
        }
    } else if let Some(s) = req.get("prompt").and_then(|v| v.as_str()) {
        s.to_string()
    } else {
        String::new()
    };

    if prompt.trim().is_empty() {
        return Err(AppError(anyhow::anyhow!("invalid request: missing input/messages/prompt")));
    }

    let response_text = state.workflow_engine.process(prompt).await?;

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
    });

    Ok(Json(resp))
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

// 错误处理
pub struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("Application error: {:?}", self.0);
        
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": self.0.to_string()
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
        Self(err.into())
    }
}
