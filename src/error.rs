use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;
use tracing::error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Workflow validation failed: {0}")]
    WorkflowValidation(String),

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("LLM API error: {0}")]
    LLMError(String),

    #[error("HTTP request error: {0}")]
    HttpError(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Workflow execution failed: {0}")]
    WorkflowExecution(String),

    #[error("Timeout error: {0}")]
    Timeout(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),
}

impl AppError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::WorkflowValidation(_) => StatusCode::BAD_REQUEST,
            AppError::ModelNotFound(_) => StatusCode::BAD_REQUEST,
            AppError::LLMError(_) => StatusCode::BAD_GATEWAY,
            AppError::HttpError(_) => StatusCode::BAD_GATEWAY,
            AppError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
            AppError::WorkflowExecution(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
            AppError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::JsonParse(_) => StatusCode::BAD_REQUEST,
            AppError::TomlParse(_) => StatusCode::BAD_REQUEST,
        }
    }

    pub fn error_code(&self) -> &'static str {
        match self {
            AppError::Config(_) => "config_error",
            AppError::WorkflowValidation(_) => "workflow_validation_error",
            AppError::ModelNotFound(_) => "model_not_found",
            AppError::LLMError(_) => "llm_error",
            AppError::HttpError(_) => "http_error",
            AppError::InvalidRequest(_) => "invalid_request",
            AppError::WorkflowExecution(_) => "workflow_execution_error",
            AppError::Timeout(_) => "timeout_error",
            AppError::Io(_) => "io_error",
            AppError::JsonParse(_) => "json_parse_error",
            AppError::TomlParse(_) => "toml_parse_error",
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let code = self.error_code();
        let message = self.to_string();

        error!(error_code = code, message = %message, "Request failed");

        let body = Json(json!({
            "error": {
                "message": message,
                "code": code
            }
        }));

        (status, body).into_response()
    }
}
