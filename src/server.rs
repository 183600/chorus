use crate::config::Config;
use crate::error::AppError;
use crate::llm::{
    ChatRequest, ChatResponse, ChatStreamChunk, GenerateRequest, GenerateResponse, LLMClient,
    Message as LLMMessage, Role,
};
use crate::workflow::{WorkflowEngine, WorkflowResult};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{sse::Event, IntoResponse, Sse},
    Json, Router,
};
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, info, instrument};

pub type SharedState = Arc<AppState>;

pub struct AppState {
    pub config: Arc<Config>,
    pub llm_client: Arc<LLMClient>,
    pub workflow_engine: Arc<WorkflowEngine>,
}

pub fn create_router(state: SharedState) -> Router {
    Router::new()
        .route("/api/generate", axum::routing::post(handle_generate))
        .route("/api/chat", axum::routing::post(handle_chat))
        .route("/v1/completions", axum::routing::post(handle_completions))
        .route("/v1/chat/completions", axum::routing::post(handle_chat_completions))
        .route("/v1/responses", axum::routing::post(handle_responses))
        .route("/v1/models", axum::routing::get(handle_models))
        .with_state(state)
}

#[derive(Deserialize)]
struct GenerateParams {
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    include_workflow: bool,
}

#[instrument(skip(state))]
async fn handle_generate(
    State(state): State<SharedState>,
    Query(params): Query<GenerateParams>,
    Json(request): Json<GenerateRequest>,
) -> Result<impl IntoResponse, AppError> {
    info!(model = %request.model, "Generate request received");

    let model = state.config.get_model(&request.model)
        .ok_or_else(|| AppError::ModelNotFound(request.model.clone()))?;

    if params.stream {
        return handle_generate_stream(state, request, model).await;
    }

    // Non-streaming workflow execution
    let workflow_result = state.workflow_engine.execute(request.prompt.clone(), params.include_workflow).await?;

    let response = GenerateResponse {
        response: workflow_result.response,
    };

    if params.include_workflow {
        let mut json_response = serde_json::to_value(&response)
            .map_err(|e| AppError::JsonParse(e))?;
        
        if let Some(details) = workflow_result.details {
            json_response["workflow"] = serde_json::to_value(details)
                .map_err(|e| AppError::JsonParse(e))?;
        }
        
        Ok(Json(json_response).into_response())
    } else {
        Ok(Json(response).into_response())
    }
}

async fn handle_generate_stream(
    state: SharedState,
    request: GenerateRequest,
    model: &crate::config::ModelConfig,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    let (tx, rx) = mpsc::channel(10);

    tokio::spawn(async move {
        // For streaming, we execute workflow but only stream final result
        let workflow_result = state.workflow_engine.execute(request.prompt.clone(), false).await;
        
        match workflow_result {
            Ok(result) => {
                // Stream response as single chunk for simplicity
                let _ = tx.send(Ok(Event::default().json_data(
                    serde_json::json!({
                        "response": result.response,
                        "done": false
                    })
                ))).await;

                // Send done event
                let _ = tx.send(Ok(Event::default().data("[DONE]"))).await;
            }
            Err(e) => {
                let _ = tx.send(Ok(Event::default().json_data(
                    serde_json::json!({
                        "error": e.to_string()
                    })
                ))).await;
            }
        }
    });

    Ok(Sse::new(ReceiverStream::new(rx)))
}

#[derive(Deserialize)]
struct ChatParams {
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    include_workflow: bool,
}

#[instrument(skip(state))]
async fn handle_chat(
    State(state): State<SharedState>,
    Query(params): Query<ChatParams>,
    Json(request): Json<ChatRequest>,
) -> Result<impl IntoResponse, AppError> {
    info!(model = %request.model, "Chat request received");

    let model = state.config.get_model(&request.model)
        .ok_or_else(|| AppError::ModelNotFound(request.model.clone()))?;

    if params.stream {
        return handle_chat_stream(state, request, model).await;
    }

    // Convert chat to workflow execution
    let prompt = request.messages.last()
        .map(|m| m.content.clone())
        .unwrap_or_default();

    let workflow_result = state.workflow_engine.execute(prompt, params.include_workflow).await?;

    let response = ChatResponse {
        message: LLMMessage {
            role: Role::Assistant,
            content: workflow_result.response,
        },
    };

    if params.include_workflow {
        let mut json_response = serde_json::to_value(&response)
            .map_err(|e| AppError::JsonParse(e))?;
        
        if let Some(details) = workflow_result.details {
            json_response["workflow"] = serde_json::to_value(details)
                .map_err(|e| AppError::JsonParse(e))?;
        }
        
        Ok(Json(json_response).into_response())
    } else {
        Ok(Json(response).into_response())
    }
}

async fn handle_chat_stream(
    _state: SharedState,
    request: ChatRequest,
    _model: &crate::config::ModelConfig,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    let (tx, rx) = mpsc::channel(10);

    tokio::spawn(async move {
        // For now, return a simple stream. In production, integrate with LLM streaming
        let _ = tx.send(Ok(Event::default().json_data(
            serde_json::json!({
                "message": {
                    "role": "assistant",
                    "content": "Streaming not yet implemented for workflow execution"
                },
                "done": false
            })
        ))).await;

        let _ = tx.send(Ok(Event::default().data("[DONE]"))).await;
    });

    Ok(Sse::new(ReceiverStream::new(rx)))
}

#[derive(Deserialize)]
struct CompletionsRequest {
    model: String,
    prompt: String,
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    max_tokens: Option<u32>,
    #[serde(default)]
    temperature: Option<f32>,
}

#[instrument(skip(state))]
async fn handle_completions(
    State(state): State<SharedState>,
    Json(request): Json<CompletionsRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(model = %request.model, "Completions request received");
    
    // Convert to workflow execution
    let workflow_result = state.workflow_engine.execute(request.prompt, false).await?;

    let response = serde_json::json!({
        "id": format!("cmpl-{}", uuid::Uuid::new_v4()),
        "object": "text_completion",
        "created": chrono::Utc::now().timestamp(),
        "model": request.model,
        "choices": [
            {
                "text": workflow_result.response,
                "index": 0,
                "logprobs": null,
                "finish_reason": "stop"
            }
        ],
        "usage": {
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "total_tokens": 0
        }
    });

    Ok(Json(response))
}

#[derive(Deserialize)]
struct ChatCompletionsRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    max_tokens: Option<u32>,
    #[serde(default)]
    temperature: Option<f32>,
}

#[derive(Deserialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

#[instrument(skip(state))]
async fn handle_chat_completions(
    State(state): State<SharedState>,
    Json(request): Json<ChatCompletionsRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(model = %request.model, "Chat completions request received");

    let prompt = request.messages.last()
        .map(|m| m.content.clone())
        .unwrap_or_default();

    let workflow_result = state.workflow_engine.execute(prompt, false).await?;

    let response = serde_json::json!({
        "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": request.model,
        "choices": [
            {
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": workflow_result.response
                },
                "finish_reason": "stop"
            }
        ],
        "usage": {
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "total_tokens": 0
        }
    });

    Ok(Json(response))
}

#[derive(Deserialize)]
struct ResponsesRequest {
    instructions: Option<String>,
    input: Option<String>,
    messages: Option<Vec<OpenAIMessage>>,
}

#[instrument(skip(state))]
async fn handle_responses(
    State(state): State<SharedState>,
    Json(request): Json<ResponsesRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    debug!("Responses request: {:?}", request);

    let prompt = if let Some(input) = request.input {
        input
    } else if let Some(messages) = request.messages {
        messages.last()
            .map(|m| m.content.clone())
            .unwrap_or_default()
    } else if let Some(instructions) = request.instructions {
        instructions
    } else {
        return Err(AppError::InvalidRequest(
            "Missing required field: input, messages, or instructions".to_string(),
        ));
    };

    let workflow_result = state.workflow_engine.execute(prompt, false).await?;

    let response = serde_json::json!({
        "id": format!("resp-{}", uuid::Uuid::new_v4()),
        "object": "response",
        "created_at": chrono::Utc::now().to_rfc3339(),
        "output": [
            {
                "type": "text",
                "text": workflow_result.response
            }
        ]
    });

    Ok(Json(response))
}

#[instrument(skip(state))]
async fn handle_models(
    State(state): State<SharedState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let models: Vec<serde_json::Value> = state.config
        .model
        .iter()
        .map(|m| {
            serde_json::json!({
                "id": m.name,
                "object": "model",
                "created": chrono::Utc::now().timestamp(),
                "owned_by": "chorus"
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "object": "list",
        "data": models
    })))
}
