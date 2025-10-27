use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde::de::Deserializer;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::env;

// 内置默认配置（不再引用项目根目录的文件）
const DEFAULT_CONFIG: &str = r#"[server]
host = "127.0.0.1"
port = 11435

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "qwen3-max"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "qwen3-vl-plus"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "kimi-k2-0905"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "glm-4.6"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "deepseek-v3.2"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "deepseek-v3.1"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "deepseek-r1"

[workflow-integration]
analyzer_model = "glm-4.6"
worker_models = [
    "qwen3-max",
    "qwen3-vl-plus",
    "kimi-k2-0905",
    "glm-4.6",
    "deepseek-v3.2",
    "deepseek-v3.1",
    "deepseek-r1"
]
synthesizer_model = "glm-4.6"

[workflow.timeouts]
analyzer_timeout_secs = 30000
worker_timeout_secs = 60000
synthesizer_timeout_secs = 60000

# 新配置：域名覆盖
[workflow.domains]

[workflow.domains."api.example.com"]
analyzer_timeout_secs = 40000
worker_timeout_secs = 80000

[workflow.domains."app.example.com"]
analyzer_timeout_secs = 20000
synthesizer_timeout_secs = 30000
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    // 兼容 README 中的单表 [model] 与 标准的 [[model]] 数组写法
    #[serde(rename = "model", deserialize_with = "deserialize_models")]
    pub models: Vec<ModelConfig>,
    #[serde(rename = "workflow-integration")]
    pub workflow_integration: WorkflowIntegration,
    pub workflow: WorkflowConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    pub api_base: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowIntegration {
    pub analyzer_model: String,
    pub worker_models: Vec<String>,
    pub synthesizer_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConfig {
    pub timeouts: TimeoutConfig,
    #[serde(default)]
    pub domains: HashMap<String, TimeoutConfig>,
    #[serde(default)]
    pub domains: HashMap<String, TimeoutConfig>,
    #[serde(default)]
    #[serde(default)]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    pub analyzer_timeout_secs: u64,
    pub worker_timeout_secs: u64,
    pub synthesizer_timeout_secs: u64,
}

impl Config {
    // 自动加载配置，优先级：
    // 1) 环境变量 CHORUS_CONFIG 指定的路径
    // 2) 用户配置：~/.config/chorus/config.toml（若不存在将写入默认配置）
    pub fn load_auto() -> Result<Self> {
        if let Ok(path) = env::var("CHORUS_CONFIG") {
            let path = PathBuf::from(path);
            if path.exists() {
                return Self::load(&path.to_string_lossy());
            } else {
                tracing::warn!("CHORUS_CONFIG points to non-existent file: {}", path.display());
            }
        }

        Self::load_from_user_config()
    }

    pub fn load(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(
