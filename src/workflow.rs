use crate::config::{Config, DomainTimeouts, ModelConfig};
use crate::error::AppError;
use crate::llm::{ChatRequest, GenerateRequest, LLMClient, Message, Role};
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct WorkflowEngine {
    config: Arc<Config>,
    llm_client: Arc<LLMClient>,
    model_refs: HashMap<String, Arc<ModelConfig>>,
}

impl WorkflowEngine {
    pub fn new(config: Arc<Config>, llm_client: Arc<LLMClient>) -> Result<Self, AppError> {
        let model_refs = config
            .model
            .iter()
            .map(|m| (m.name.clone(), Arc::new(m.clone())))
            .collect();

        Ok(Self {
            config,
            llm_client,
            model_refs,
        })
    }

    pub async fn execute(
        &self,
        prompt: String,
        include_details: bool,
    ) -> Result<WorkflowResult, AppError> {
        let workflow_id = uuid::Uuid::new_v4().to_string();
        let start_time = Instant::now();
        
        let mut details = if include_details {
            Some(WorkflowExecutionDetails::new(workflow_id.clone()))
        } else {
            None
        };

        // 1. Analyzer phase
        let analysis = self.analyze_task(&prompt, &mut details).await?;

        // 2. Workers phase
        let worker_results = self.execute_workers(&prompt, &analysis, &mut details).await?;

        // 3. Selector phase
        let selection = self.select_best(&worker_results, &mut details).await?;

        // 4. Synthesizer phase
        let final_response = self.synthesize(&selection, &worker_results, &mut details).await?;

        let duration = start_time.elapsed();

        Ok(WorkflowResult {
            response: final_response,
            details: details.map(|d| d.finish(duration)),
        })
    }

    async fn analyze_task(
        &self,
        prompt: &str,
        details: &mut Option<WorkflowExecutionDetails>,
    ) -> Result<TaskAnalysis, AppError> {
        let start = Instant::now();
        let model_name = self.get_model_name("analyzer")?;
        let model = self.model_refs.get(&model_name)
            .ok_or_else(|| AppError::ModelNotFound(model_name.clone()))?;

        let timeouts = self.config.get_domain_timeouts(&model.api_base);

        let analysis_prompt = format!(
            "Analyze this task and determine:\n1. Task complexity (0-10)\n2. Recommended temperature (0.0-2.0)\n3. Task type classification\n4. Key requirements\n\nTask: {}\n\nRespond in JSON format with fields: complexity, temperature, task_type, requirements",
            prompt
        );

        let request = GenerateRequest {
            model: model_name.clone(),
            prompt: analysis_prompt,
            stream: false,
            temperature: Some(0.3), // Low temperature for analysis
        };

        let result = timeout(
            tokio::time::Duration::from_secs(timeouts.analyzer),
            self.llm_client.generate(&model.api_base, &model.api_key, &request),
        )
        .await
        .map_err(|_| AppError::Timeout("Analyzer phase timed out".to_string()))?;

        match result {
            Ok(response) => {
                let analysis = self.parse_analysis(&response.response)?;
                
                if let Some(ref mut d) = details {
                    d.analyzer = Some(PhaseDetails {
                        model: model_name,
                        duration: start.elapsed(),
                        success: true,
                        error: None,
                        output: Some(response.response),
                    });
                }

                debug!("Task analysis complete: {:?}", analysis);
                Ok(analysis)
            }
            Err(e) => {
                error!("Analyzer phase failed: {}", e);
                
                if let Some(ref mut d) = details {
                    d.analyzer = Some(PhaseDetails {
                        model: model_name,
                        duration: start.elapsed(),
                        success: false,
                        error: Some(e.to_string()),
                        output: None,
                    });
                }

                // Fallback to default analysis
                Ok(TaskAnalysis {
                    complexity: 5,
                    recommended_temperature: 1.4,
                    task_type: "general".to_string(),
                    requirements: vec!["accuracy".to_string()],
                })
            }
        }
    }

    async fn execute_workers(
        &self,
        prompt: &str,
        analysis: &TaskAnalysis,
        details: &mut Option<WorkflowExecutionDetails>,
    ) -> Result<Vec<WorkerResult>, AppError> {
        let depth = self.config.workflow_integration.nested_worker_depth;
        let mut all_workers = Vec::new();
        
        // Parse worker configuration
        let workflow: WorkflowDefinition = serde_json::from_str(&self.config.workflow_integration.json)
            .map_err(|e| AppError::WorkflowValidation(format!("Invalid workflow JSON: {}", e)))?;

        // Expand nested workers
        self.expand_workers(&workflow.workers, depth, &mut all_workers)?;

        debug!("Executing {} workers with depth {}", all_workers.len(), depth);

        let mut worker_tasks = Vec::new();
        for (i, worker) in all_workers.iter().enumerate() {
            let model_name = self.get_model_ref_name(worker, "worker")?;
            let model = self.model_refs.get(&model_name)
                .ok_or_else(|| AppError::ModelNotFound(model_name.clone()))?;

            let temperature = self.determine_temperature(worker, model, analysis);
            let timeouts = self.config.get_domain_timeouts(&model.api_base);

            let task = self.execute_single_worker(
                i,
                model.clone(),
                prompt.to_string(),
                temperature,
                timeout::Duration::from_secs(timeouts.worker),
            );

            worker_tasks.push(task);
        }

        let results = join_all(worker_tasks).await;
        
        let mut worker_results = Vec::new();
        let mut successful_count = 0;

        for (i, result) in results.into_iter().enumerate() {
            match result {
                Ok(output) => {
                    successful_count += 1;
                    worker_results.push(WorkerResult {
                        worker_id: i,
                        model: all_workers[i].ref_name.clone().unwrap_or_else(|| "unknown".to_string()),
                        output,
                        success: true,
                        error: None,
                    });
                }
                Err(e) => {
                    error!("Worker {} failed: {}", i, e);
                    worker_results.push(WorkerResult {
                        worker_id: i,
                        model: all_workers[i].ref_name.clone().unwrap_or_else(|| "unknown".to_string()),
                        output: "".to_string(),
                        success: false,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        if let Some(ref mut d) = details {
            d.worker_count = all_workers.len();
            d.worker_successful = successful_count;
            d.worker_failures = all_workers.len() - successful_count;
        }

        debug!("Workers completed: {}/{} successful", successful_count, all_workers.len());
        Ok(worker_results)
    }

    async fn execute_single_worker(
        &self,
        worker_id: usize,
        model: Arc<ModelConfig>,
        prompt: String,
        temperature: f32,
        timeout_duration: timeout::Duration,
    ) -> Result<String, AppError> {
        let request = GenerateRequest {
            model: model.name.clone(),
            prompt,
            stream: false,
            temperature: Some(temperature),
        };

        let result = timeout(
            timeout_duration,
            self.llm_client.generate(&model.api_base, &model.api_key, &request),
        )
        .await
        .map_err(|_| AppError::Timeout(format!("Worker {} timed out", worker_id)))?;

        result.map(|r| r.response).map_err(|e| {
            AppError::WorkflowExecution(format!("Worker {} failed: {}", worker_id, e))
        })
    }

    async fn select_best(
        &self,
        worker_results: &[WorkerResult],
        details: &mut Option<WorkflowExecutionDetails>,
    ) -> Result<SelectionResult, AppError> {
        let start = Instant::now();
        let model_name = self.get_model_name("selector")?;
        let model = self.model_refs.get(&model_name)
            .ok_or_else(|| AppError::ModelNotFound(model_name.clone()))?;

        let timeouts = self.config.get_domain_timeouts(&model.api_base);

        let successful_outputs: Vec<_> = worker_results
            .iter()
            .filter(|r| r.success)
            .map(|r| format!("Worker {} ({}): {}", r.worker_id, r.model, r.output))
            .collect();

        if successful_outputs.is_empty() {
            return Err(AppError::WorkflowExecution("No successful worker outputs to select from".to_string()));
        }

        let selection_prompt = format!(
            "Select the best response from the following candidates. Explain your reasoning.\n\n{}\n\nProvide your selection in JSON format with fields: selected_response, reasoning",
            successful_outputs.join("\n\n---\n\n")
        );

        let request = GenerateRequest {
            model: model_name.clone(),
            prompt: selection_prompt,
            stream: false,
            temperature: Some(0.5),
        };

        let result = timeout(
            tokio::time::Duration::from_secs(timeouts.worker),
            self.llm_client.generate(&model.api_base, &model.api_key, &request),
        )
        .await
        .map_err(|_| AppError::Timeout("Selector phase timed out".to_string()))?;

        match result {
            Ok(response) => {
                let selection = self.parse_selection(&response.response)?;
                
                if let Some(ref mut d) = details {
                    d.selector = Some(PhaseDetails {
                        model: model_name,
                        duration: start.elapsed(),
                        success: true,
                        error: None,
                        output: Some(response.response),
                    });
                }

                Ok(selection)
            }
            Err(e) => {
                error!("Selector phase failed: {}", e);
                
                if let Some(ref mut d) = details {
                    d.selector = Some(PhaseDetails {
                        model: model_name,
                        duration: start.elapsed(),
                        success: false,
                        error: Some(e.to_string()),
                        output: None,
                    });
                }

                // Fallback: select first successful result
                Ok(SelectionResult {
                    selected_response: successful_outputs.first().unwrap().clone(),
                    reasoning: "Fallback selection due to selector failure".to_string(),
                })
            }
        }
    }

    async fn synthesize(
        &self,
        selection: &SelectionResult,
        worker_results: &[WorkerResult],
        details: &mut Option<WorkflowExecutionDetails>,
    ) -> Result<String, AppError> {
        let start = Instant::now();
        let model_name = self.get_model_name("synthesizer")?;
        let model = self.model_refs.get(&model_name)
            .ok_or_else(|| AppError::ModelNotFound(model_name.clone()))?;

        let timeouts = self.config.get_domain_timeouts(&model.api_base);

        let all_outputs = worker_results
            .iter()
            .map(|r| format!("Worker {} ({}): {}", r.worker_id, r.model, r.output))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        let synthesize_prompt = format!(
            "Based on the selected best response and all candidate outputs, synthesize a final comprehensive answer.\n\nSelected Response:\n{}\n\nReasoning:\n{}\n\nAll Outputs:\n{}\n\nProvide a final synthesized response.",
            selection.selected_response,
            selection.reasoning,
            all_outputs
        );

        let request = GenerateRequest {
            model: model_name.clone(),
            prompt: synthesize_prompt,
            stream: false,
            temperature: Some(0.7),
        };

        let result = timeout(
            tokio::time::Duration::from_secs(timeouts.synthesizer),
            self.llm_client.generate(&model.api_base, &model.api_key, &request),
        )
        .await
        .map_err(|_| AppError::Timeout("Synthesizer phase timed out".to_string()))?;

        match result {
            Ok(response) => {
                if let Some(ref mut d) = details {
                    d.synthesizer = Some(PhaseDetails {
                        model: model_name,
                        duration: start.elapsed(),
                        success: true,
                        error: None,
                        output: Some(response.response.clone()),
                    });
                }

                Ok(response.response)
            }
            Err(e) => {
                error!("Synthesizer phase failed: {}", e);
                
                if let Some(ref mut d) = details {
                    d.synthesizer = Some(PhaseDetails {
                        model: model_name,
                        duration: start.elapsed(),
                        success: false,
                        error: Some(e.to_string()),
                        output: None,
                    });
                }

                // Fallback to selector's choice
                Ok(selection.selected_response.clone())
            }
        }
    }

    fn get_model_name(&self, phase: &str) -> Result<String, AppError> {
        let workflow: WorkflowDefinition = serde_json::from_str(&self.config.workflow_integration.json)
            .map_err(|e| AppError::WorkflowValidation(format!("Invalid workflow JSON: {}", e)))?;

        match phase {
            "analyzer" => workflow.analyzer.ref_name,
            "selector" => workflow.selector.ref_name,
            "synthesizer" => workflow.synthesizer.ref_name,
            _ => None,
        }
        .ok_or_else(|| AppError::WorkflowValidation(format!("No model reference for {}", phase)))
    }

    fn get_model_ref_name(&self, node: &WorkerNode, default_phase: &str) -> Result<String, AppError> {
        node.ref_name.clone()
            .or_else(|| self.get_model_name(default_phase).ok())
            .ok_or_else(|| AppError::WorkflowValidation(format!("No model reference for worker: {:?}", node)))
    }

    fn expand_workers(
        &self,
        workers: &[WorkerNode],
        depth: usize,
        expanded: &mut Vec<WorkerNode>,
    ) -> Result<(), AppError> {
        if depth == 0 {
            return Ok(());
        }

        for worker in workers {
            if depth == 1 {
                expanded.push(worker.clone());
            } else {
                // Recursive expansion
                let mut expanded_worker = worker.clone();
                if let Some(children) = &worker.children {
                    let mut child_expanded = Vec::new();
                    self.expand_workers(children, depth - 1, &mut child_expanded)?;
                    expanded_worker.children = Some(child_expanded);
                }
                expanded.push(expanded_worker);
            }
        }

        Ok(())
    }

    fn determine_temperature(
        &self,
        worker: &WorkerNode,
        model: &ModelConfig,
        analysis: &TaskAnalysis,
    ) -> f32 {
        // 1. Explicit worker temperature
        if let Some(temp) = worker.temperature {
            return temp;
        }

        // 2. Auto temperature from analysis
        if model.auto_temperature {
            return analysis.recommended_temperature;
        }

        // 3. Model default temperature
        model.temperature.unwrap_or(1.4)
    }

    fn parse_analysis(&self, text: &str) -> Result<TaskAnalysis, AppError> {
        // Try to extract JSON from response
        if let Some(json_start) = text.find('{') {
            if let Some(json_end) = text.rfind('}') {
                let json_str = &text[json_start..=json_end];
                if let Ok(analysis) = serde_json::from_str::<TaskAnalysis>(json_str) {
                    return Ok(analysis);
                }
            }
        }

        // Fallback parsing
        let complexity = self.extract_number(text, "complexity").unwrap_or(5);
        let temperature = self.extract_number(text, "temperature").unwrap_or(1.4);
        let task_type = self.extract_value(text, "task_type").unwrap_or_else(|| "general".to_string());

        Ok(TaskAnalysis {
            complexity,
            recommended_temperature: temperature,
            task_type,
            requirements: vec!["general".to_string()],
        })
    }

    fn parse_selection(&self, text: &str) -> Result<SelectionResult, AppError> {
        if let Some(json_start) = text.find('{') {
            if let Some(json_end) = text.rfind('}') {
                let json_str = &text[json_start..=json_end];
                if let Ok(selection) = serde_json::from_str::<SelectionResult>(json_str) {
                    return Ok(selection);
                }
            }
        }

        // Fallback: return the whole response as selected
        Ok(SelectionResult {
            selected_response: text.to_string(),
            reasoning: "Direct selection".to_string(),
        })
    }

    fn extract_number(&self, text: &str, key: &str) -> Option<f32> {
        let pattern = format!(r#""{}":\s*([0-9.]+)"#, key);
        regex::Regex::new(&pattern)
            .ok()?
            .captures(text)?
            .get(1)?
            .as_str()
            .parse()
            .ok()
    }

    fn extract_value(&self, text: &str, key: &str) -> Option<String> {
        let pattern = format!(r#""{}":\s*"([^"]+)""#, key);
        regex::Regex::new(&pattern)
            .ok()?
            .captures(text)?
            .get(1)?
            .as_str()
            .to_string()
            .into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkflowDefinition {
    pub analyzer: NodeDefinition,
    pub workers: Vec<WorkerNode>,
    pub selector: NodeDefinition,
    pub synthesizer: NodeDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodeDefinition {
    #[serde(rename = "ref")]
    pub ref_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkerNode {
    #[serde(rename = "ref")]
    pub ref_name: Option<String>,
    pub temperature: Option<f32>,
    pub children: Option<Vec<WorkerNode>>,
}

#[derive(Debug)]
pub struct WorkflowResult {
    pub response: String,
    pub details: Option<WorkflowExecutionDetails>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowExecutionDetails {
    pub workflow_id: String,
    pub total_duration_ms: u128,
    pub analyzer: Option<PhaseDetails>,
    pub worker_count: usize,
    pub worker_successful: usize,
    pub worker_failures: usize,
    pub selector: Option<PhaseDetails>,
    pub synthesizer: Option<PhaseDetails>,
}

impl WorkflowExecutionDetails {
    fn new(workflow_id: String) -> Self {
        Self {
            workflow_id,
            total_duration_ms: 0,
            analyzer: None,
            worker_count: 0,
            worker_successful: 0,
            worker_failures: 0,
            selector: None,
            synthesizer: None,
        }
    }

    fn finish(mut self, duration: std::time::Duration) -> Self {
        self.total_duration_ms = duration.as_millis();
        self
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PhaseDetails {
    pub model: String,
    pub duration: std::time::Duration,
    pub success: bool,
    pub error: Option<String>,
    pub output: Option<String>,
}

#[derive(Debug)]
struct TaskAnalysis {
    complexity: i32,
    recommended_temperature: f32,
    task_type: String,
    requirements: Vec<String>,
}

#[derive(Debug)]
struct SelectionResult {
    selected_response: String,
    reasoning: String,
}

#[derive(Debug, Clone)]
struct WorkerResult {
    worker_id: usize,
    model: String,
    output: String,
    success: bool,
    error: Option<String>,
}
