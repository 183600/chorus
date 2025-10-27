use url::Url;
use crate::config::Config;
use crate::llm::{ChatMessage, LLMClient, parse_temperature_from_response};
use anyhow::Result;
use std::collections::HashMap;

pub struct WorkflowEngine {
    config: Config,
    model_configs: HashMap<String, crate::config::ModelConfig>,
}

impl WorkflowEngine {
    pub fn new(config: Config) -> Self {
        let model_configs = config.build_model_map();
        Self {
            config,
            model_configs,
        }
    }

    pub async fn process(&self, prompt: String) -> Result<String> {
        tracing::info!("Starting workflow processing");

        // 第一步：分析prompt并确定temperature
        let temperature = self.analyze_prompt(&prompt).await?;
        tracing::info!("Step 1 completed - Temperature: {}", temperature);

        // 第二步：调用所有工作模型
        let worker_responses = self.call_workers(&prompt, temperature).await?;
        tracing::info!("Step 2 completed - Collected {} worker responses", worker_responses.len());

        // 第三步：综合所有响应
        let final_response = self.synthesize_responses(&prompt, &worker_responses).await?;
        tracing::info!("Step 3 completed - Final response generated");

        Ok(final_response)
    }

    async fn analyze_prompt(&self, prompt: &str) -> Result<f32> {
        let analyzer_model = &self.config.workflow_integration.analyzer_model;
        let model_config = self.model_configs.get(analyzer_model)
            .ok_or_else(|| anyhow::anyhow!("Analyzer model '{}' not found in config", analyzer_model))?;

        // 检查是否配置了固定的temperature值
        if let Some(temp) = model_config.temperature {
            tracing::info!("Using configured temperature: {}", temp);
            return Ok(temp);
        }

        // 检查是否启用了自动temperature选择
        let auto_temp = model_config.auto_temperature.unwrap_or(false);
        if !auto_temp {
            // 如果没有启用自动选择，使用默认值
            tracing::info!("Auto temperature disabled, using default: 1.4");
            return Ok(1.4);
        }

        // 启用了自动temperature选择，调用分析器模型
        tracing::info!("Auto temperature enabled, analyzing prompt...");
        let domain = extract_domain_from_url(&model_config.api_base);
        let t = self.config.effective_timeouts_for_domain(domain.as_deref());
        let client = LLMClient::new(
            model_config.api_base.clone(),
            model_config.api_key.clone(),
            t.analyzer_timeout_secs,
        );

        let analysis_prompt = format!(
            r#"请分析以下用户提示，并为其推荐一个合适的temperature参数（0.0-2.0之间的浮点数）。
Temperature越低（接近0），输出越确定和保守；temperature越高（接近2），输出越有创造性和随机性。

用户提示: {}

请只返回一个JSON对象，格式如下：
{{
    "temperature": 0.7,
    "reasoning": "简短说明为什么选择这个temperature值"
}}
"#,
            prompt
        );

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: analysis_prompt,
        }];

        let response = client.chat_completion(analyzer_model, messages, Some(0.3)).await?;
        
        let temperature = parse_temperature_from_response(&response);
        tracing::debug!("Analyzed temperature: {} (from response: {})", temperature, response);

        Ok(temperature)
    }

    async fn call_workers(&self, prompt: &str, temperature: f32) -> Result<Vec<(String, String)>> {
        let mut responses = Vec::new();

        for worker_model in &self.config.workflow_integration.worker_models {
            let model_config = self.model_configs.get(worker_model)
                .ok_or_else(|| anyhow::anyhow!("Worker model '{}' not found in config", worker_model))?;

            tracing::info!("Calling worker model: {}", worker_model);

            let domain = extract_domain_from_url(&model_config.api_base);
            let t = self.config.effective_timeouts_for_domain(domain.as_deref());
            let client = LLMClient::new(
                model_config.api_base.clone(),
                model_config.api_key.clone(),
                t.worker_timeout_secs,
            );

            let messages = vec![ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }];

            // 使用模型配置的temperature，如果没有则使用分析器确定的temperature
            let model_temp = model_config.temperature.unwrap_or(temperature);
            tracing::debug!("Using temperature {} for worker {}", model_temp, worker_model);

            match client.chat_completion(worker_model, messages, Some(model_temp)).await {
                Ok(response) => {
                    tracing::debug!("Worker {} response: {}", worker_model, response);
                    responses.push((worker_model.clone(), response));
                }
                Err(e) => {
                    tracing::warn!("Worker {} failed: {}", worker_model, e);
                    // 继续处理其他模型
                }
            }
        }

        if responses.is_empty() {
            return Err(anyhow::anyhow!("All worker models failed"));
        }

        Ok(responses)
    }

    async fn synthesize_responses(
        &self,
        original_prompt: &str,
        worker_responses: &[(String, String)],
    ) -> Result<String> {
        let synthesizer_model = &self.config.workflow_integration.synthesizer_model;
        let model_config = self.model_configs.get(synthesizer_model)
            .ok_or_else(|| anyhow::anyhow!("Synthesizer model '{}' not found in config", synthesizer_model))?;

        let domain = extract_domain_from_url(&model_config.api_base);
        let t = self.config.effective_timeouts_for_domain(domain.as_deref());
        let client = LLMClient::new(
            model_config.api_base.clone(),
            model_config.api_key.clone(),
            t.synthesizer_timeout_secs,
        );

        // 构建综合提示
        let mut synthesis_prompt = format!(
            "原始用户问题：\n{}\n\n以下是多个AI模型对该问题的回答：\n\n",
            original_prompt
        );

        for (i, (model_name, response)) in worker_responses.iter().enumerate() {
            synthesis_prompt.push_str(&format!(
                "【模型{}：{}】\n{}\n\n",
                i + 1,
                model_name,
                response
            ));
        }

        synthesis_prompt.push_str(
            "请综合以上所有回答，生成一个高质量的最终答案。要求：\n\
            1. 综合各个模型的优点\n\
            2. 确保答案准确、完整\n\
            3. 保持清晰的逻辑和结构\n\
            4. 直接给出最终答案，不要提及\"综合以上回答\"等元信息\n"
        );

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: synthesis_prompt,
        }];

        // 使用模型配置的temperature，如果没有则使用默认值1.4
        let synth_temp = model_config.temperature.unwrap_or(1.4);
        tracing::debug!("Using temperature {} for synthesizer", synth_temp);

        let final_response = client
            .chat_completion(synthesizer_model, messages, Some(synth_temp))
            .await?;

        Ok(final_response)
    }
}
fn extract_domain_from_url(url: &str) -> Option<String> {
    Url::parse(url).ok().and_then(|u| u.host_str().map(|s| s.to_string()))
}
