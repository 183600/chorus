use crate::config::{Config, ModelConfig, WorkflowModelTarget, WorkflowPlan, WorkflowWorker};
use crate::llm::{parse_temperature_from_response, ChatMessage, LLMClient};
use anyhow::{anyhow, Result};
use async_recursion::async_recursion;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowResult {
    pub final_response: String,
    pub execution_details: WorkflowExecutionDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowExecutionDetails {
    pub analyzer: AnalyzerDetails,
    pub workers: Vec<WorkerDetails>,
    pub synthesizer: SynthesizerDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzerDetails {
    pub model: String,
    pub temperature: f32,
    pub auto_temperature: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerDetails {
    pub name: String,
    pub temperature: Option<f32>,
    pub response: Option<String>,
    pub success: bool,
    pub error: Option<String>,
    pub nested: Option<Box<WorkflowExecutionDetails>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesizerDetails {
    pub model: String,
    pub temperature: f32,
}

pub struct WorkflowEngine {
    config: Config,
    model_configs: HashMap<String, ModelConfig>,
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
        self.run_plan(&self.config.workflow_integration, &prompt, 0)
            .await
    }

    pub async fn process_with_details(&self, prompt: String) -> Result<WorkflowResult> {
        self.run_plan_with_details(&self.config.workflow_integration, &prompt, 0)
            .await
    }

    #[async_recursion]
    async fn run_plan_with_details(
        &self,
        plan: &WorkflowPlan,
        prompt: &str,
        depth: usize,
    ) -> Result<WorkflowResult> {
        if depth == 0 {
            tracing::info!("Starting workflow processing with details");
        } else {
            tracing::debug!(
                "Starting nested workflow at depth {} ({})",
                depth,
                plan.label()
            );
        }

        let target = &plan.analyzer;
        let model_config = self.lookup_model(&target.model)?;

        let auto_temperature = target
            .auto_temperature
            .or(model_config.auto_temperature)
            .unwrap_or(false);

        let temperature = self
            .resolve_analyzer_temperature(plan, prompt, depth)
            .await?;

        let analyzer_details = AnalyzerDetails {
            model: target.model.clone(),
            temperature,
            auto_temperature,
        };

        if depth == 0 {
            tracing::info!("Step 1 completed - Temperature: {}", temperature);
        } else {
            tracing::debug!(
                "Nested workflow depth {} analyzer temperature resolved to {}",
                depth,
                temperature
            );
        }

        let worker_details = self
            .run_workers_with_details(plan, prompt, temperature, depth)
            .await?;

        if depth == 0 {
            tracing::info!(
                "Step 2 completed - Collected {} worker responses",
                worker_details.iter().filter(|w| w.success).count()
            );
        } else {
            tracing::debug!(
                "Nested workflow depth {} collected {} worker responses",
                depth,
                worker_details.iter().filter(|w| w.success).count()
            );
        }

        let worker_responses: Vec<(String, String)> = worker_details
            .iter()
            .filter_map(|w| {
                if w.success {
                    w.response.as_ref().map(|r| (w.name.clone(), r.clone()))
                } else {
                    None
                }
            })
            .collect();

        let synthesizer_target = &plan.synthesizer;
        let synthesizer_model_config = self.lookup_model(&synthesizer_target.model)?;
        let synthesizer_temperature = self.resolve_synthesizer_temperature(
            synthesizer_target,
            synthesizer_model_config,
            depth,
        );

        let synthesizer_details = SynthesizerDetails {
            model: synthesizer_target.model.clone(),
            temperature: synthesizer_temperature,
        };

        let final_response = self
            .call_synthesizer(plan, prompt, &worker_responses, depth)
            .await?;

        if depth == 0 {
            tracing::info!("Step 3 completed - Final response generated");
        } else {
            tracing::debug!(
                "Nested workflow depth {} synthesized response successfully",
                depth
            );
        }

        Ok(WorkflowResult {
            final_response,
            execution_details: WorkflowExecutionDetails {
                analyzer: analyzer_details,
                workers: worker_details,
                synthesizer: synthesizer_details,
            },
        })
    }

    async fn run_plan(&self, plan: &WorkflowPlan, prompt: &str, depth: usize) -> Result<String> {
        let result = self.run_plan_with_details(plan, prompt, depth).await?;
        Ok(result.final_response)
    }

    async fn resolve_analyzer_temperature(
        &self,
        plan: &WorkflowPlan,
        prompt: &str,
        depth: usize,
    ) -> Result<f32> {
        let target = &plan.analyzer;
        let model_config = self.lookup_model(&target.model)?;

        if let Some(explicit) = target.temperature.or(model_config.temperature) {
            if depth == 0 {
                tracing::info!("Using configured analyzer temperature: {}", explicit);
            } else {
                tracing::debug!(
                    "Depth {} using configured analyzer temperature {}",
                    depth,
                    explicit
                );
            }
            return Ok(explicit);
        }

        let auto = target
            .auto_temperature
            .or(model_config.auto_temperature)
            .unwrap_or(false);

        if !auto {
            if depth == 0 {
                tracing::info!("Analyzer auto temperature disabled, using default: 1.4");
            } else {
                tracing::debug!(
                    "Depth {} analyzer auto temperature disabled, using default 1.4",
                    depth
                );
            }
            return Ok(1.4);
        }

        if depth == 0 {
            tracing::info!(
                "Auto temperature enabled for analyzer {} at depth {}, analyzing prompt",
                target.model,
                depth
            );
        } else {
            tracing::debug!(
                "Auto temperature enabled for analyzer {} at depth {}, analyzing prompt",
                target.model,
                depth
            );
        }

        let domain = extract_domain_from_url(&model_config.api_base);
        let timeouts = self.config.effective_timeouts_for_domain(domain.as_deref());
        let client = LLMClient::new(
            model_config.api_base.clone(),
            model_config.api_key.clone(),
            timeouts.analyzer_timeout_secs,
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

        let response = client
            .chat_completion(&target.model, messages, Some(0.3))
            .await?;

        let temperature = parse_temperature_from_response(&response);
        tracing::debug!(
            "Analyzer {} produced temperature {} (depth {}), response: {}",
            target.model,
            temperature,
            depth,
            response
        );

        Ok(temperature)
    }

    #[allow(dead_code)]
    async fn run_workers(
        &self,
        plan: &WorkflowPlan,
        prompt: &str,
        base_temperature: f32,
        depth: usize,
    ) -> Result<Vec<(String, String)>> {
        let details = self
            .run_workers_with_details(plan, prompt, base_temperature, depth)
            .await?;
        let responses = details
            .into_iter()
            .filter_map(|worker| {
                if worker.success {
                    worker.response.map(|resp| (worker.name, resp))
                } else {
                    None
                }
            })
            .collect();
        Ok(responses)
    }

    #[async_recursion]
    async fn run_workers_with_details(
        &self,
        plan: &WorkflowPlan,
        prompt: &str,
        base_temperature: f32,
        depth: usize,
    ) -> Result<Vec<WorkerDetails>> {
        let plan_label = plan.label();

        if plan.workers.is_empty() {
            tracing::error!(
                plan = %plan_label,
                depth,
                "Workflow plan is missing worker nodes"
            );
            return Err(anyhow!(
                "Workflow plan {} has no worker nodes configured at depth {}. Please define at least one worker in the workflow configuration.",
                plan_label,
                depth
            ));
        }

        let mut worker_details = Vec::new();

        for worker in &plan.workers {
            match worker {
                WorkflowWorker::Model(target) => {
                    if depth == 0 {
                        tracing::info!("Calling worker model: {}", target.model);
                    } else {
                        tracing::debug!("Calling worker model {} at depth {}", target.model, depth);
                    }

                    let temperature = if let Ok(model_config) = self.lookup_model(&target.model) {
                        self.resolve_worker_temperature(target, model_config, base_temperature)
                    } else {
                        let err = self.lookup_model(&target.model);
                        let err_display = err.expect_err("lookup should have failed").to_string();
                        tracing::warn!(
                            worker = %target.model,
                            depth,
                            error = %err_display,
                            "Worker lookup failed"
                        );
                        worker_details.push(WorkerDetails {
                            name: target.model.clone(),
                            temperature: None,
                            response: None,
                            success: false,
                            error: Some(err_display),
                            nested: None,
                        });
                        continue;
                    };

                    match self
                        .call_worker_model(target, prompt, base_temperature, depth)
                        .await
                    {
                        Ok(response) => {
                            tracing::debug!("Worker {} succeeded at depth {}", target.model, depth);
                            worker_details.push(WorkerDetails {
                                name: target.model.clone(),
                                temperature: Some(temperature),
                                response: Some(response),
                                success: true,
                                error: None,
                                nested: None,
                            });
                        }
                        Err(err) => {
                            let err_display = err.to_string();
                            tracing::warn!(
                                worker = %target.model,
                                depth,
                                error = %err_display,
                                "Worker call failed"
                            );
                            worker_details.push(WorkerDetails {
                                name: target.model.clone(),
                                temperature: Some(temperature),
                                response: None,
                                success: false,
                                error: Some(err_display),
                                nested: None,
                            });
                        }
                    }
                }
                WorkflowWorker::Workflow(sub_plan) => {
                    let label = sub_plan.label();
                    if depth == 0 {
                        tracing::info!("Executing nested workflow worker: {}", label);
                    } else {
                        tracing::debug!(
                            "Executing nested workflow worker {} at depth {}",
                            label,
                            depth
                        );
                    }

                    match self
                        .run_plan_with_details(sub_plan, prompt, depth + 1)
                        .await
                    {
                        Ok(result) => {
                            tracing::debug!(
                                "Nested workflow {} succeeded at depth {}",
                                label,
                                depth
                            );
                            worker_details.push(WorkerDetails {
                                name: label.clone(),
                                temperature: None,
                                response: Some(result.final_response),
                                success: true,
                                error: None,
                                nested: Some(Box::new(result.execution_details)),
                            });
                        }
                        Err(err) => {
                            let err_display = err.to_string();
                            tracing::warn!(
                                workflow = %label,
                                depth,
                                error = %err_display,
                                "Nested workflow failed"
                            );
                            worker_details.push(WorkerDetails {
                                name: label,
                                temperature: None,
                                response: None,
                                success: false,
                                error: Some(err_display),
                                nested: None,
                            });
                        }
                    }
                }
            }
        }

        if worker_details.iter().filter(|w| w.success).count() == 0 {
            let worker_errors: Vec<String> = worker_details
                .iter()
                .filter_map(|w| w.error.as_ref().map(|e| format!("{}: {}", w.name, e)))
                .collect();

            let mut message = format!(
                "All worker nodes failed at depth {} for plan {}",
                depth, plan_label
            );
            if !worker_errors.is_empty() {
                message.push_str(". Worker errors: ");
                message.push_str(&worker_errors.join(" | "));
            } else {
                let labels = plan.worker_labels();
                if !labels.is_empty() {
                    message.push_str(". Configured workers: ");
                    message.push_str(&labels.join(", "));
                }
            }
            return Err(anyhow!(message));
        }

        Ok(worker_details)
    }

    async fn call_worker_model(
        &self,
        target: &WorkflowModelTarget,
        prompt: &str,
        base_temperature: f32,
        depth: usize,
    ) -> Result<String> {
        let model_config = self.lookup_model(&target.model)?;

        let domain = extract_domain_from_url(&model_config.api_base);
        let timeouts = self.config.effective_timeouts_for_domain(domain.as_deref());
        let client = LLMClient::new(
            model_config.api_base.clone(),
            model_config.api_key.clone(),
            timeouts.worker_timeout_secs,
        );

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: prompt.to_string(),
        }];

        let temperature = self.resolve_worker_temperature(target, model_config, base_temperature);

        tracing::debug!(
            "Using temperature {} for worker {} at depth {}",
            temperature,
            target.model,
            depth
        );

        let response = client
            .chat_completion(&target.model, messages, Some(temperature))
            .await?;

        tracing::debug!(
            "Worker {} returned response at depth {}",
            target.model,
            depth
        );

        Ok(response)
    }

    async fn call_synthesizer(
        &self,
        plan: &WorkflowPlan,
        original_prompt: &str,
        worker_responses: &[(String, String)],
        depth: usize,
    ) -> Result<String> {
        let target = &plan.synthesizer;
        let model_config = self.lookup_model(&target.model)?;

        let domain = extract_domain_from_url(&model_config.api_base);
        let timeouts = self.config.effective_timeouts_for_domain(domain.as_deref());
        let client = LLMClient::new(
            model_config.api_base.clone(),
            model_config.api_key.clone(),
            timeouts.synthesizer_timeout_secs,
        );

        let mut synthesis_prompt = format!(
            "原始用户问题：\n{}\n\n以下是多个AI模型对该问题的回答：\n\n",
            original_prompt
        );

        for (i, (label, response)) in worker_responses.iter().enumerate() {
            synthesis_prompt.push_str(&format!("【模型{}：{}】\n{}\n\n", i + 1, label, response));
        }

        synthesis_prompt.push_str(
            "请综合以上所有回答，生成一个高质量的最终答案。要求：\n\
            1. 综合各个模型的优点\n\
            2. 确保答案准确、完整\n\
            3. 保持清晰的逻辑和结构\n\
            4. 直接给出最终答案，不要提及\"综合以上回答\"等元信息\n",
        );

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: synthesis_prompt,
        }];

        let temperature = self.resolve_synthesizer_temperature(target, model_config, depth);

        tracing::debug!(
            "Using temperature {} for synthesizer {} at depth {}",
            temperature,
            target.model,
            depth
        );

        let final_response = client
            .chat_completion(&target.model, messages, Some(temperature))
            .await?;

        Ok(final_response)
    }

    fn resolve_worker_temperature(
        &self,
        target: &WorkflowModelTarget,
        model_config: &ModelConfig,
        base_temperature: f32,
    ) -> f32 {
        if let Some(t) = target.temperature {
            return t;
        }
        if let Some(t) = model_config.temperature {
            return t;
        }
        base_temperature
    }

    fn resolve_synthesizer_temperature(
        &self,
        target: &WorkflowModelTarget,
        model_config: &ModelConfig,
        depth: usize,
    ) -> f32 {
        if let Some(t) = target.temperature {
            return t;
        }
        if let Some(t) = model_config.temperature {
            return t;
        }
        if matches!(target.auto_temperature, Some(true))
            || matches!(model_config.auto_temperature, Some(true))
        {
            tracing::debug!(
                "Synthesizer {} requested auto temperature at depth {}, defaulting to 1.4",
                target.model,
                depth
            );
        }
        1.4
    }

    fn lookup_model(&self, name: &str) -> Result<&ModelConfig> {
        self.model_configs.get(name).ok_or_else(|| {
            anyhow!(
                "Model '{}' not found in configuration. Did you define it under [[model]]?",
                name
            )
        })
    }
}

fn extract_domain_from_url(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        Config, ModelConfig, ServerConfig, TimeoutConfig, WorkflowConfig, WorkflowModelTarget,
        WorkflowPlan, WorkflowWorker,
    };
    use std::collections::HashMap;

    fn build_test_config_with_workers(workers: Vec<WorkflowWorker>) -> Config {
        Config {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 11435,
            },
            models: vec![ModelConfig {
                name: "primary".to_string(),
                api_base: "http://localhost".to_string(),
                api_key: "sk-test".to_string(),
                temperature: Some(0.2),
                auto_temperature: None,
            }],
            workflow_integration: WorkflowPlan {
                analyzer: WorkflowModelTarget {
                    model: "primary".to_string(),
                    temperature: Some(0.2),
                    auto_temperature: None,
                },
                workers,
                synthesizer: WorkflowModelTarget {
                    model: "primary".to_string(),
                    temperature: Some(0.2),
                    auto_temperature: None,
                },
            },
            workflow: WorkflowConfig {
                timeouts: TimeoutConfig {
                    analyzer_timeout_secs: 30,
                    worker_timeout_secs: 30,
                    synthesizer_timeout_secs: 30,
                },
                domains: HashMap::new(),
            },
        }
    }

    #[tokio::test]
    async fn reports_missing_workers_in_error() {
        let config = build_test_config_with_workers(Vec::new());
        let engine = WorkflowEngine::new(config);
        let err = engine
            .process("hello world".to_string())
            .await
            .expect_err("expected failure when no workers configured");
        let message = err.to_string();
        assert!(
            message.contains("workflow:primary"),
            "message did not include plan label: {}",
            message
        );
        assert!(
            message.contains("no worker nodes configured"),
            "message did not explain worker misconfiguration: {}",
            message
        );
    }

    #[tokio::test]
    async fn includes_worker_failure_details() {
        let workers = vec![WorkflowWorker::Model(WorkflowModelTarget {
            model: "missing".to_string(),
            temperature: None,
            auto_temperature: None,
        })];
        let config = build_test_config_with_workers(workers);
        let engine = WorkflowEngine::new(config);
        let err = engine
            .process("hello world".to_string())
            .await
            .expect_err("expected failure when worker model missing");
        let message = err.to_string();
        assert!(
            message.contains("missing"),
            "message did not include worker label: {}",
            message
        );
        assert!(
            message.contains("Model 'missing' not found in configuration"),
            "message did not include underlying error: {}",
            message
        );
        assert!(
            message.contains("Worker errors"),
            "message did not include worker errors section: {}",
            message
        );
    }
}
