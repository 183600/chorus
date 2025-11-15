use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ChatResponse {
    pub id: Option<String>,
    pub object: Option<String>,
    pub created: Option<i64>,
    pub model: Option<String>,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Choice {
    pub index: i32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Usage {
    pub prompt_tokens: Option<i32>,
    pub completion_tokens: Option<i32>,
    pub total_tokens: Option<i32>,
}

pub struct LLMClient {
    client: Client,
    api_base: String,
    api_key: String,
}

impl LLMClient {
    pub fn new(api_base: String, api_key: String, timeout_secs: u64) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .with_context(|| format!("Failed to build HTTP client for {}", api_base))?;

        Ok(Self {
            client,
            api_base,
            api_key,
        })
    }

    pub async fn chat_completion(
        &self,
        model: &str,
        messages: Vec<ChatMessage>,
        temperature: Option<f32>,
    ) -> Result<String> {
        let url = format!("{}/chat/completions", self.api_base.trim_end_matches('/'));

        let request_body = json!({
            "model": model,
            "messages": messages,
            "temperature": temperature,
            "stream": false,
        });

        tracing::debug!("Calling LLM API: {} with model: {}", url, model);
        tracing::debug!(
            "Request body: {}",
            serde_json::to_string_pretty(&request_body)?
        );

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            return Err(anyhow!(
                "LLM API request failed with status {}: {}",
                status,
                error_text
            ));
        }

        // Be tolerant to different provider response shapes
        let v: serde_json::Value = response.json().await?;

        // Try OpenAI-compatible: choices[0].message.content as string
        if let Some(s) = v
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c0| c0.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
        {
            return Ok(s.to_string());
        }

        // Some providers return content as array of parts
        if let Some(parts) = v
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c0| c0.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        {
            let mut out = String::new();
            for p in parts {
                if let Some(s) = p.as_str() {
                    out.push_str(s);
                } else if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                    out.push_str(t);
                }
            }
            if !out.is_empty() {
                return Ok(out);
            }
        }

        // Some providers (e.g., GLM-4.x) use reasoning_content instead of content
        if let Some(s) = v
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c0| c0.get("message"))
            .and_then(|m| m.get("reasoning_content"))
            .and_then(|c| c.as_str())
        {
            return Ok(s.to_string());
        }
        if let Some(parts) = v
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c0| c0.get("message"))
            .and_then(|m| m.get("reasoning_content"))
            .and_then(|c| c.as_array())
        {
            let mut out = String::new();
            for p in parts {
                if let Some(s) = p.as_str() {
                    out.push_str(s);
                } else if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                    out.push_str(t);
                }
            }
            if !out.is_empty() {
                return Ok(out);
            }
        }

        // Some providers use choices[0].text
        if let Some(s) = v
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c0| c0.get("text"))
            .and_then(|t| t.as_str())
        {
            return Ok(s.to_string());
        }

        // Fallback fields like output_text
        if let Some(s) = v.get("output_text").and_then(|t| t.as_str()) {
            return Ok(s.to_string());
        }

        // Last resort: stringify first choice.message object
        if let Some(obj) = v
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c0| c0.get("message"))
        {
            return Ok(obj.to_string());
        }

        if let Some(err_msg) = detect_provider_error(&v) {
            return Err(anyhow!(
                "LLM provider {} (model {}) returned error: {}",
                self.api_base,
                model,
                err_msg
            ));
        }

        Err(anyhow!("LLM response missing content field: {}", v))
    }
}

fn detect_provider_error(value: &serde_json::Value) -> Option<String> {
    if let Some(error_val) = value.get("error") {
        if let Some(obj) = error_val.as_object() {
            let message = ["message", "msg", "error_message", "error_msg", "detail"]
                .iter()
                .filter_map(|key| obj.get(*key))
                .filter_map(json_value_to_string)
                .map(|s| s.trim().to_string())
                .find(|s| !s.is_empty());
            let code = ["code", "status", "type"]
                .iter()
                .filter_map(|key| obj.get(*key))
                .filter_map(json_value_to_string)
                .map(|s| s.trim().to_string())
                .find(|s| !s.is_empty());
            if let Some(msg) = message {
                if let Some(code_str) = code {
                    return Some(format!("{}: {}", code_str, msg));
                } else {
                    return Some(msg);
                }
            } else {
                return Some(error_val.to_string());
            }
        } else if let Some(text) = json_value_to_string(error_val) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    if let Some(status_val) = value.get("status") {
        if let Some(status_str) = interpret_status_like_error(status_val) {
            let message = extract_message_fields(
                value,
                &[
                    "msg",
                    "message",
                    "error_message",
                    "error_msg",
                    "cause",
                    "detail",
                ],
            );
            return Some(match message {
                Some(msg) => format!("status {}: {}", status_str, msg),
                None => format!("status {}", status_str),
            });
        }
    }

    if let Some(code_val) = value.get("code") {
        if let Some(code_str) = interpret_status_like_error(code_val) {
            let message = extract_message_fields(
                value,
                &[
                    "message",
                    "msg",
                    "error_message",
                    "error_msg",
                    "cause",
                    "detail",
                ],
            );
            return Some(match message {
                Some(msg) => format!("code {}: {}", code_str, msg),
                None => format!("code {}", code_str),
            });
        }
    }

    if let Some(success) = value.get("success").and_then(|v| v.as_bool()) {
        if !success {
            if let Some(msg) = extract_message_fields(
                value,
                &["message", "msg", "error_message", "error_msg", "error"],
            ) {
                return Some(msg);
            }
            return Some("success flag was false".to_string());
        }
    }

    if let Some(msg) = extract_message_fields(value, &["message", "msg"]) {
        let lowered = msg.to_ascii_lowercase();
        if lowered.contains("error")
            || lowered.contains("invalid")
            || lowered.contains("fail")
            || lowered.contains("denied")
            || lowered.contains("unauthorized")
        {
            return Some(msg);
        }
    }

    None
}

fn interpret_status_like_error(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Number(n) => {
            if let Some(int) = n.as_i64() {
                if int != 0 && int != 200 {
                    return Some(int.to_string());
                }
            } else if let Some(f) = n.as_f64() {
                if f != 0.0 && f != 200.0 {
                    return Some(f.to_string());
                }
            }
            None
        }
        serde_json::Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return None;
            }
            if let Ok(int) = trimmed.parse::<i64>() {
                if int != 0 && int != 200 {
                    return Some(int.to_string());
                }
                return None;
            }
            let lowered = trimmed.to_ascii_lowercase();
            if lowered == "ok"
                || lowered == "success"
                || lowered == "succeeded"
                || lowered == "true"
                || lowered == "0"
                || lowered == "200"
            {
                None
            } else if lowered.contains("error")
                || lowered.contains("fail")
                || lowered.contains("invalid")
                || lowered.contains("denied")
                || lowered.contains("unauthorized")
            {
                Some(trimmed.to_string())
            } else {
                None
            }
        }
        serde_json::Value::Bool(b) => {
            if *b {
                None
            } else {
                Some("false".to_string())
            }
        }
        _ => None,
    }
}

fn extract_message_fields(value: &serde_json::Value, fields: &[&str]) -> Option<String> {
    for key in fields {
        if let Some(inner) = value.get(*key) {
            if let Some(text) = json_value_to_string(inner) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

fn json_value_to_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Array(items) => {
            let mut parts = Vec::new();
            for item in items {
                if let Some(text) = json_value_to_string(item) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        parts.push(trimmed.to_string());
                    }
                }
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(" "))
            }
        }
        serde_json::Value::Object(map) => {
            for key in &[
                "message",
                "msg",
                "error_message",
                "error_msg",
                "detail",
                "description",
            ] {
                if let Some(inner) = map.get(*key) {
                    if let Some(text) = json_value_to_string(inner) {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            return Some(trimmed.to_string());
                        }
                    }
                }
            }
            Some(value.to_string())
        }
    }
}

pub fn parse_temperature_from_response(response: &str) -> f32 {
    if let Some(value) = parse_temperature_from_json(response) {
        return clamp_temperature(value);
    }

    for line in response.lines() {
        if line.to_lowercase().contains("temperature") {
            if let Some(value_part) = line.split_once(':').map(|x| x.1) {
                if let Some(value) = parse_numeric_fragment(value_part) {
                    return clamp_temperature(value);
                }
            }

            if let Some(value) = parse_numeric_fragment(line) {
                return clamp_temperature(value);
            }
        }
    }

    1.4
}

fn parse_temperature_from_json(response: &str) -> Option<f32> {
    let json_value = serde_json::from_str::<serde_json::Value>(response).ok()?;
    extract_temperature_from_json(&json_value)
}

fn extract_temperature_from_json(value: &serde_json::Value) -> Option<f32> {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(temp_value) = map.get("temperature") {
                return match temp_value {
                    serde_json::Value::Number(n) => n.as_f64().map(|v| v as f32),
                    serde_json::Value::String(s) => parse_numeric_fragment(s),
                    _ => None,
                };
            }

            map.values().find_map(extract_temperature_from_json)
        }
        serde_json::Value::Array(items) => items.iter().find_map(extract_temperature_from_json),
        _ => None,
    }
}

fn parse_numeric_fragment(input: &str) -> Option<f32> {
    let trimmed = input.trim().trim_matches(|c| c == '"' || c == '\'');
    if let Ok(value) = trimmed.parse::<f32>() {
        return Some(value);
    }

    trimmed
        .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .find(|segment| !segment.is_empty())
        .and_then(|segment| segment.parse::<f32>().ok())
}

fn clamp_temperature(value: f32) -> f32 {
    value.clamp(0.0, 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_temperature_from_json_string_value() {
        let response = r#"{"temperature":"0.65","reasoning":"ok"}"#;
        let value = parse_temperature_from_response(response);
        assert!((value - 0.65).abs() < 1e-6);
    }

    #[test]
    fn parses_temperature_from_text_fragment() {
        let response = "Temperature: \"0.42\", reasoning: details";
        let value = parse_temperature_from_response(response);
        assert!((value - 0.42).abs() < 1e-6);
    }

    #[test]
    fn clamps_out_of_range_values() {
        let response = r#"{"temperature": 3.5}"#;
        assert_eq!(parse_temperature_from_response(response), 2.0);
    }

    #[test]
    fn detects_status_based_error_message() {
        let value = serde_json::json!({
            "status": "434",
            "msg": "Invalid apiKey"
        });
        let err = detect_provider_error(&value).expect("expected error");
        assert!(err.contains("434"));
        assert!(err.to_ascii_lowercase().contains("invalid"));
    }

    #[test]
    fn detects_error_object_code() {
        let value = serde_json::json!({
            "error": {
                "code": "invalid_api_key",
                "message": "No API key provided"
            }
        });
        let err = detect_provider_error(&value).expect("expected error");
        assert!(err.to_ascii_lowercase().contains("invalid_api_key"));
        assert!(err.contains("No API key provided"));
    }

    #[test]
    fn detects_success_flag_false_message() {
        let value = serde_json::json!({
            "success": false,
            "message": "Request failed"
        });
        let err = detect_provider_error(&value).expect("expected error");
        assert!(err.contains("Request failed"));
    }

    #[test]
    fn ignores_successful_status_values() {
        let value = serde_json::json!({
            "status": 0,
            "msg": "ok"
        });
        assert!(detect_provider_error(&value).is_none());
    }
}
