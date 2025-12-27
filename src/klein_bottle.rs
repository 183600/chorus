use crate::config::{Config, ModelConfig};
use crate::llm::{ChatMessage, LLMClient};
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::time::{timeout, Duration};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KleinBottleConfig {
    /// 最大循环次数
    pub max_iterations: usize,
    /// 收敛阈值 (0-1)，当自评估分数超过此值时停止
    pub convergence_threshold: f32,
    /// 反思提示模板
    pub reflection_prompt_template: String,
    /// 评估提示模板
    pub evaluation_prompt_template: String,
    /// 使用的模型名称
    pub model_name: String,
    /// 每次请求的超时时间（秒）
    pub timeout_secs: u64,
}

impl Default for KleinBottleConfig {
    fn default() -> Self {
        Self {
            max_iterations: 3,
            convergence_threshold: 0.8,
            reflection_prompt_template: "请从逻辑、事实和创造性三个角度批判上文，并撰写一个更完善的版本。保持回答的核心观点，但增强其深度、严谨性和创新性。".to_string(),
            evaluation_prompt_template: "请对以下回答进行评分（0-1分），评估其在逻辑性、事实准确性和创造性方面的综合质量。只需返回一个数字分数。".to_string(),
            model_name: "glm-4.6".to_string(),
            timeout_secs: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionIteration {
    pub iteration_number: usize,
    pub input: String,
    pub output: String,
    pub reflection_prompt: String,
    pub evaluation_score: Option<f32>,
    pub reasoning: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KleinBottleResult {
    pub initial_question: String,
    pub final_answer: String,
    pub iterations: Vec<ReflectionIteration>,
    pub total_iterations: usize,
    pub converged: bool,
    pub final_score: Option<f32>,
    pub execution_time_seconds: f64,
}

pub struct KleinBottleWorkflow {
    config: KleinBottleConfig,
    llm_client: LLMClient,
    model_config: ModelConfig,
}

impl KleinBottleWorkflow {
    pub fn new(config: KleinBottleConfig, global_config: &Config) -> Result<Self> {
        // 查找指定的模型配置
        let model_config = global_config
            .models
            .iter()
            .find(|m| m.name == config.model_name)
            .ok_or_else(|| anyhow!("Model '{}' not found in configuration", config.model_name))?;

        let llm_client = LLMClient::new(
            model_config.api_base.clone(),
            model_config.api_key.clone(),
            config.timeout_secs,
        )?;

        Ok(Self {
            config,
            llm_client,
            model_config: model_config.clone(),
        })
    }

    /// 执行克莱因瓶反思循环
    pub async fn execute_reflection_cycle(&self, question: &str) -> Result<KleinBottleResult> {
        let start_time = std::time::Instant::now();
        let mut iterations = Vec::new();
        let mut current_answer = question.to_string();
        let mut converged = false;
        let mut final_score = None;

        // 第一次迭代：生成初始回答
        let initial_iteration = self
            .generate_initial_answer(question, 0)
            .await?;
        current_answer = initial_iteration.output.clone();
        iterations.push(initial_iteration);

        // 执行反思循环
        for i in 1..=self.config.max_iterations {
            let iteration = self
                .perform_reflection_iteration(&current_answer, i)
                .await?;
            
            // 检查是否收敛
            if let Some(score) = iteration.evaluation_score {
                if score >= self.config.convergence_threshold {
                    converged = true;
                    final_score = Some(score);
                    current_answer = iteration.output.clone();
                    iterations.push(iteration);
                    break;
                }
            }

            current_answer = iteration.output.clone();
            iterations.push(iteration);

            // 如果是最后一次迭代，记录最终分数
            if i == self.config.max_iterations {
                if let Some(last_iteration) = iterations.last() {
                    final_score = last_iteration.evaluation_score;
                }
            }
        }

        let execution_time = start_time.elapsed().as_secs_f64();

        let total_iterations = iterations.len();
        Ok(KleinBottleResult {
            initial_question: question.to_string(),
            final_answer: current_answer,
            iterations,
            total_iterations,
            converged,
            final_score,
            execution_time_seconds: execution_time,
        })
    }

    /// 生成初始回答
    async fn generate_initial_answer(&self, question: &str, iteration: usize) -> Result<ReflectionIteration> {
        let prompt = format!(
            "请详细回答以下问题：\n\n{}\n\n请提供一个深入、全面且富有洞察力的回答。",
            question
        );

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: prompt,
        }];

        let response = self
            .call_llm_with_timeout(&messages)
            .await
            .context("Failed to generate initial answer")?;

        let timestamp = chrono::Utc::now().to_rfc3339();

        Ok(ReflectionIteration {
            iteration_number: iteration,
            input: question.to_string(),
            output: response.clone(),
            reflection_prompt: "生成初始回答".to_string(),
            evaluation_score: None,
            reasoning: None,
            timestamp,
        })
    }

    /// 执行一次反思迭代
    async fn perform_reflection_iteration(
        &self,
        previous_answer: &str,
        iteration: usize,
    ) -> Result<ReflectionIteration> {
        // 构建反思提示
        let reflection_prompt = format!(
            "{}\n\n前文：\n{}",
            self.config.reflection_prompt_template, previous_answer
        );

        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: "你是一个思想深刻、逻辑严谨的思考助手。你的任务是对给定的回答进行批判性反思并改进。".to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: reflection_prompt.clone(),
            },
        ];

        let improved_answer = self
            .call_llm_with_timeout(&messages)
            .await
            .context("Failed to generate reflection")?;

        // 评估改进后的回答
        let evaluation_score = self
            .evaluate_answer(&improved_answer)
            .await
            .context("Failed to evaluate answer")?;

        let timestamp = chrono::Utc::now().to_rfc3339();

        Ok(ReflectionIteration {
            iteration_number: iteration,
            input: previous_answer.to_string(),
            output: improved_answer,
            reflection_prompt: self.config.reflection_prompt_template.clone(),
            evaluation_score: Some(evaluation_score),
            reasoning: None,
            timestamp,
        })
    }

    /// 评估回答质量
    async fn evaluate_answer(&self, answer: &str) -> Result<f32> {
        let evaluation_prompt = format!(
            "{}\n\n回答内容：\n{}",
            self.config.evaluation_prompt_template, answer
        );

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: evaluation_prompt,
        }];

        let response = self
            .call_llm_with_timeout(&messages)
            .await
            .context("Failed to get evaluation")?;

        // 解析分数
        let score_str = response.trim();
        let score: f32 = score_str
            .parse()
            .unwrap_or(0.5); // 如果解析失败，返回中等分数

        // 确保分数在0-1范围内
        Ok(score.clamp(0.0, 1.0))
    }

    /// 调用LLM并处理超时
    async fn call_llm_with_timeout(&self, messages: &[ChatMessage]) -> Result<String> {
        let duration = Duration::from_secs(self.config.timeout_secs);
        
        let result: Result<String> = timeout(duration, async {
            self.llm_client
                .chat_completion(&self.model_config.name, messages.to_vec(), self.model_config.temperature)
                .await
        })
        .await
        .map_err(|_| anyhow!("LLM request timed out after {} seconds", self.config.timeout_secs))?;

        let completion_result = result.context("LLM request failed")?;
        Ok(completion_result)
    }

    /// 打印详细的结果报告
    pub fn print_detailed_report(&self, result: &KleinBottleResult) {
        println!("=== 克莱因瓶反思循环结果报告 ===\n");
        
        println!("初始问题：\n{}\n", result.initial_question);
        println!("最终回答：\n{}\n", result.final_answer);
        println!("总迭代次数：{}", result.total_iterations);
        println!("是否收敛：{}", if result.converged { "是" } else { "否" });
        
        if let Some(score) = result.final_score {
            println!("最终评分：{:.2}/1.00", score);
        }
        
        println!("执行时间：{:.2}秒\n", result.execution_time_seconds);
        
        println!("=== 迭代详情 ===");
        for (i, iteration) in result.iterations.iter().enumerate() {
            println!("\n--- 迭代 {} ---", i);
            println!("时间：{}", iteration.timestamp);
            
            if i == 0 {
                println!("类型：初始回答生成");
            } else {
                println!("反思提示：{}", iteration.reflection_prompt);
                if let Some(score) = iteration.evaluation_score {
                    println!("评估分数：{:.2}/1.00", score);
                }
            }
            
            println!("输入长度：{}字符", iteration.input.len());
            println!("输出长度：{}字符", iteration.output.len());
            
            if iteration.output.len() < 500 {
                println!("输出内容：\n{}", iteration.output);
            } else {
                println!("输出内容：[过长，已省略，见完整结果文件]");
            }
        }
        
        println!("\n=== 思考进化分析 ===");
        if result.iterations.len() >= 2 {
            let initial_length = result.iterations[0].output.len();
            let final_length = result.final_answer.len();
            let length_change = ((final_length as f32 - initial_length as f32) / initial_length as f32) * 100.0;
            
            println!("内容长度变化：{:+.1}% ({} -> {} 字符)", 
                length_change, initial_length, final_length);
            
            if let Some(first_score) = result.iterations.get(1).and_then(|i| i.evaluation_score) {
                if let Some(last_score) = result.final_score {
                    let score_improvement = last_score - first_score;
                    println!("质量评分提升：{:+.2} ({:.2} -> {:.2})", 
                        score_improvement, first_score, last_score);
                }
            }
        }
    }
}

/// 创建示例配置
pub fn create_demo_config() -> KleinBottleConfig {
    KleinBottleConfig {
        max_iterations: 3,
        convergence_threshold: 0.8,
        reflection_prompt_template: "请从逻辑、事实和创造性三个角度批判上文，并撰写一个更完善的版本。保持回答的核心观点，但增强其深度、严谨性和创新性。".to_string(),
        evaluation_prompt_template: "请对以下回答进行评分（0-1分），评估其在逻辑性、事实准确性和创造性方面的综合质量。只需返回一个数字分数，如：0.85".to_string(),
        model_name: "glm-4.6".to_string(),
        timeout_secs: 60,
    }
}

/// 示例问题集合
pub fn get_demo_questions() -> Vec<&'static str> {
    vec![
        "意识的本质是什么？",
        "人工智能是否能够真正理解情感？",
        "时间的本质是什么？它是否是客观存在的？",
        "人类自由意志的存在性及其哲学意义",
        "数学真理是发现还是发明的？",
    ]
}
