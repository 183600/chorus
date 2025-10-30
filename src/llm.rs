use anyhow::{anyhow, Result};
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
    pub fn new(api_base: String, api_key: String, timeout_secs: u64) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .unwrap();

        Self {
            client,
            api_base,
            api_key,
        }
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
        tracing::debug!("Request body: {}", serde_json::to_string_pretty(&request_body)?);

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

        Err(anyhow!(
            "LLM response missing content field: {}",
            v
        ))
    }
}

pub fn parse_temperature_from_response(response: &str) -> f32 {
    if let Some(value) = parse_temperature_from_json(response) {
        return clamp_temperature(value);
    }

    for line in response.lines() {
        if line.to_lowercase().contains("temperature") {
            if let Some(value_part) = line.splitn(2, ':').nth(1) {
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
}
