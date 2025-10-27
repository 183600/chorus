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
    // 尝试从响应中提取temperature值
    // 查找JSON格式的temperature
    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(response) {
        if let Some(temp) = json_value.get("temperature") {
            if let Some(temp_f64) = temp.as_f64() {
                return temp_f64 as f32;
            }
        }
    }

    // 尝试查找文本中的temperature值
    let lines = response.lines();
    for line in lines {
        if line.to_lowercase().contains("temperature") {
            // 尝试提取数字
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 2 {
                let value_part = parts[1].trim();
                if let Ok(temp) = value_part.parse::<f32>() {
                    return temp.max(0.0).min(2.0);
                }
            }
        }
    }

    // 默认返回1.4
    1.4
}