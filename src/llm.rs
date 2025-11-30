use crate::error::AppError;
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use tracing::{debug, error, info_span, Instrument};

#[derive(Debug, Clone)]
pub struct LLMClient {
    client: Client,
}

impl LLMClient {
    pub fn new() -> Result<Self, AppError> {
        let client = Client::builder()
            .use_rustls_tls()
            .timeout(Duration::from_secs(120))
            .pool_idle_timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(10)
            .build()
            .map_err(|e| AppError::LLMError(format!("Failed to build HTTP client: {}", e)))?;

        Ok(Self { client })
    }

    pub async fn generate(
        &self,
        api_base: &str,
        api_key: &str,
        request: &GenerateRequest,
    ) -> Result<GenerateResponse, AppError> {
        let url = format!("{}/api/generate", api_base.trim_end_matches('/'));
        
        let payload = serde_json::to_string(&request)
            .map_err(|e| AppError::LLMError(format!("Failed to serialize request: {}", e)))?;

        // Log sanitized payload
        debug!(
            url = %url,
            payload = %self.sanitize_payload(&payload),
            "Sending generate request"
        );

        let request_builder = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .body(payload);

        self.execute_request(request_builder, "generate").await
    }

    pub async fn chat(
        &self,
        api_base: &str,
        api_key: &str,
        request: &ChatRequest,
    ) -> Result<ChatResponse, AppError> {
        let url = format!("{}/api/chat", api_base.trim_end_matches('/'));
        
        let payload = serde_json::to_string(&request)
            .map_err(|e| AppError::LLMError(format!("Failed to serialize request: {}", e)))?;

        debug!(
            url = %url,
            payload = %self.sanitize_payload(&payload),
            "Sending chat request"
        );

        let request_builder = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .body(payload);

        self.execute_request(request_builder, "chat").await
    }

    pub async fn chat_stream(
        &self,
        api_base: &str,
        api_key: &str,
        request: &ChatRequest,
    ) -> Result<impl futures::Stream<Item = Result<ChatStreamChunk, AppError>>, AppError> {
        let url = format!("{}/api/chat", api_base.trim_end_matches('/'));
        
        let mut request = request.clone();
        request.stream = true;

        let payload = serde_json::to_string(&request)
            .map_err(|e| AppError::LLMError(format!("Failed to serialize request: {}", e)))?;

        debug!(
            url = %url,
            payload = %self.sanitize_payload(&payload),
            "Sending streaming chat request"
        );

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .body(payload)
            .send()
            .await
            .map_err(|e| AppError::HttpError(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::LLMError(format!(
                "Streaming request failed with status {}: {}",
                status, body
            )));
        }

        let stream = response.bytes_stream().map(|chunk| {
            chunk
                .map_err(|e| AppError::HttpError(format!("Stream error: {}", e)))
                .and_then(|bytes| {
                    let line = String::from_utf8_lossy(&bytes);
                    debug!("Received stream chunk: {}", line.trim());
                    
                    if line.starts_with("data: ") {
                        let json_str = line.trim_start_matches("data: ").trim();
                        if json_str == "[DONE]" {
                            Ok(ChatStreamChunk::Done)
                        } else {
                            serde_json::from_str(json_str)
                                .map(ChatStreamChunk::Data)
                                .map_err(AppError::JsonParse)
                        }
                    } else {
                        Ok(ChatStreamChunk::Empty)
                    }
                })
        });

        Ok(stream)
    }

    async fn execute_request<T: serde::de::DeserializeOwned>(
        &self,
        request_builder: RequestBuilder,
        operation: &str,
    ) -> Result<T, AppError> {
        let span = info_span!("llm_request", operation = %operation);
        let _enter = span.enter();

        let response = request_builder
            .send()
            .await
            .map_err(|e| AppError::HttpError(format!("Request failed: {}", e)))?;

        let status = response.status();
        let body = response.text().await.map_err(|e| {
            AppError::HttpError(format!("Failed to read response body: {}", e))
        })?;

        if !status.is_success() {
            error!(
                status = %status,
                body = %body,
                "LLM API returned error"
            );
            return Err(AppError::LLMError(format!(
                "API returned status {}: {}",
                status, body
            )));
        }

        serde_json::from_str(&body).map_err(|e| {
            error!("Failed to parse response: {}", e);
            AppError::JsonParse(e)
        })
    }

    fn sanitize_payload(&self, payload: &str) -> String {
        // Simple heuristic to truncate long prompts while preserving structure
        if payload.len() > 200 {
            if let Ok(json) = serde_json::from_str::<Value>(payload) {
                if let Some(mut obj) = json.as_object().cloned() {
                    // Truncate prompt/messages fields
                    if let Some(prompt) = obj.get("prompt").and_then(|p| p.as_str()) {
                        if prompt.len() > 100 {
                            obj.insert(
                                "prompt".to_string(),
                                Value::String(format!("{}...<truncated>", &prompt[..100])),
                            );
                        }
                    }
                    if let Some(messages) = obj.get_mut("messages").and_then(|m| m.as_array_mut()) {
                        for msg in messages.iter_mut() {
                            if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                                if content.len() > 100 {
                                    msg["content"] = Value::String(format!(
                                        "{}...<truncated>",
                                        &content[..100]
                                    ));
                                }
                            }
                        }
                    }
                    return serde_json::to_string(&obj).unwrap_or_else(|_| "<truncated>".to_string());
                }
            }
            format!("{}...<truncated {} bytes>", &payload[..200], payload.len())
        } else {
            payload.to_string()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateRequest {
    pub model: String,
    pub prompt: String,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateResponse {
    pub response: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub message: Message,
}

#[derive(Debug)]
pub enum ChatStreamChunk {
    Data(ChatStreamResponse),
    Done,
    Empty,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatStreamResponse {
    pub message: Message,
}
