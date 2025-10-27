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
# temperature = 1.4  # 可选：设置温度值（0.0-2.0），不设置则使用模型默认值
# auto_temperature = false  # 可选：是否由大模型自动选择温度，默认 false

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "qwen3-vl-plus"
# temperature = 1.4
# auto_temperature = false

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "kimi-k2-0905"
# temperature = 1.4
# auto_temperature = false

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "glm-4.6"
# temperature = 1.4
# auto_temperature = false

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "deepseek-v3.2"
# temperature = 1.4
# auto_temperature = false

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "deepseek-v3.1"
# temperature = 1.4
# auto_temperature = false

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "deepseek-r1"
# temperature = 1.4
# auto_temperature = false

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

# 旧配置（兼容）
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
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub auto_temperature: Option<bool>,
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
    pub domains: HashMap<String, DomainTimeoutOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    pub analyzer_timeout_secs: u64,
    pub worker_timeout_secs: u64,
    pub synthesizer_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DomainTimeoutOverride {
    pub analyzer_timeout_secs: Option<u64>,
    pub worker_timeout_secs: Option<u64>,
    pub synthesizer_timeout_secs: Option<u64>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ModelOneOrMany {
    One(ModelConfig),
    Many(Vec<ModelConfig>),
}

fn deserialize_models<'de, D>(deserializer: D) -> std::result::Result<Vec<ModelConfig>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = ModelOneOrMany::deserialize(deserializer)?;
    Ok(match v {
        ModelOneOrMany::One(m) => vec![m],
        ModelOneOrMany::Many(vs) => vs,
    })
}

#[cfg(test)]
mod tests_support { pub use super::*; }

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
            .with_context(|| format!("Failed to read config file: {}", path))?;
        let cfg: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse TOML from {}", path))?;
        Ok(cfg)
    }

    fn user_config_path() -> Result<PathBuf> {
        let home = env::var("HOME").context("HOME env var not set")?;
        Ok(Path::new(&home).join(".config").join("chorus").join("config.toml"))
    }

    fn ensure_user_config_exists() -> Result<PathBuf> {
        let path = Self::user_config_path()?;
        if let Some(dir) = path.parent() {
            if !dir.exists() {
                fs::create_dir_all(dir)
                    .with_context(|| format!("Failed to create config dir: {}", dir.display()))?;
            }
        }
        if !path.exists() {
            fs::write(&path, DEFAULT_CONFIG)
                .with_context(|| format!("Failed to write default config to {}", path.display()))?;
        } else {
            // 配置文件存在，检查是否需要迁移
            Self::migrate_config_if_needed(&path)?;
        }
        Ok(path)
    }

    /// 检查并迁移旧配置文件到新格式
    /// 如果配置文件是旧格式（没有 temperature/auto_temperature 字段），则：
    /// 1. 备份旧配置文件为 config.toml.backup.{timestamp}
    /// 2. 将配置转换为新格式（添加注释说明新字段）
    fn migrate_config_if_needed(config_path: &Path) -> Result<()> {
        let content = fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;
        
        // 检查是否已经包含新字段的注释或配置
        let has_temperature_config = content.contains("temperature") || content.contains("auto_temperature");
        
        if has_temperature_config {
            // 配置已经是新格式，无需迁移
            return Ok(());
        }

        tracing::info!("Detected old config format, migrating to new format...");

        // 创建备份文件
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let backup_path = config_path.with_extension(format!("toml.backup.{}", timestamp));
        
        fs::copy(config_path, &backup_path)
            .with_context(|| format!("Failed to backup config to {}", backup_path.display()))?;
        
        tracing::info!("Old config backed up to: {}", backup_path.display());

        // 解析旧配置
        let old_config: Config = toml::from_str(&content)
            .with_context(|| "Failed to parse old config")?;

        // 生成新配置内容（添加注释说明）
        let mut new_content = String::new();
        
        // 添加文件头注释
        new_content.push_str("# Chorus 配置文件\n");
        new_content.push_str("# 此文件已自动从旧格式迁移到新格式\n");
        new_content.push_str(&format!("# 原配置已备份到: {}\n", backup_path.display()));
        new_content.push_str("#\n");
        new_content.push_str("# 新增功能：Temperature 配置\n");
        new_content.push_str("# - temperature: 设置固定的温度值 (0.0-2.0)\n");
        new_content.push_str("# - auto_temperature: 让大模型自动选择温度 (true/false)\n");
        new_content.push_str("# 详细说明请参考: TEMPERATURE_CONFIG.md\n\n");

        // Server 配置
        new_content.push_str("[server]\n");
        new_content.push_str(&format!("host = \"{}\"\n", old_config.server.host));
        new_content.push_str(&format!("port = {}\n\n", old_config.server.port));

        // Model 配置
        for model in &old_config.models {
            new_content.push_str("[[model]]\n");
            new_content.push_str(&format!("api_base = \"{}\"\n", model.api_base));
            new_content.push_str(&format!("api_key = \"{}\"\n", model.api_key));
            new_content.push_str(&format!("name = \"{}\"\n", model.name));
            new_content.push_str("# temperature = 1.4  # 可选：设置温度值（0.0-2.0），不设置则使用默认值 1.4\n");
            new_content.push_str("# auto_temperature = false  # 可选：是否由大模型自动选择温度，默认 false\n\n");
        }

        // Workflow Integration 配置
        new_content.push_str("[workflow-integration]\n");
        new_content.push_str(&format!("analyzer_model = \"{}\"\n", old_config.workflow_integration.analyzer_model));
        new_content.push_str("worker_models = [\n");
        for (i, model) in old_config.workflow_integration.worker_models.iter().enumerate() {
            if i == old_config.workflow_integration.worker_models.len() - 1 {
                new_content.push_str(&format!("    \"{}\"\n", model));
            } else {
                new_content.push_str(&format!("    \"{}\",\n", model));
            }
        }
        new_content.push_str("]\n");
        new_content.push_str(&format!("synthesizer_model = \"{}\"\n\n", old_config.workflow_integration.synthesizer_model));

        // Workflow Timeouts 配置
        new_content.push_str("[workflow.timeouts]\n");
        new_content.push_str(&format!("analyzer_timeout_secs = {}\n", old_config.workflow.timeouts.analyzer_timeout_secs));
        new_content.push_str(&format!("worker_timeout_secs = {}\n", old_config.workflow.timeouts.worker_timeout_secs));
        new_content.push_str(&format!("synthesizer_timeout_secs = {}\n\n", old_config.workflow.timeouts.synthesizer_timeout_secs));

        // Workflow Domains 配置
        if !old_config.workflow.domains.is_empty() {
            new_content.push_str("[workflow.domains]\n\n");
            for (domain, override_config) in &old_config.workflow.domains {
                new_content.push_str(&format!("[workflow.domains.\"{}\"]\n", domain));
                if let Some(timeout) = override_config.analyzer_timeout_secs {
                    new_content.push_str(&format!("analyzer_timeout_secs = {}\n", timeout));
                }
                if let Some(timeout) = override_config.worker_timeout_secs {
                    new_content.push_str(&format!("worker_timeout_secs = {}\n", timeout));
                }
                if let Some(timeout) = override_config.synthesizer_timeout_secs {
                    new_content.push_str(&format!("synthesizer_timeout_secs = {}\n", timeout));
                }
                new_content.push_str("\n");
            }
        }

        // 写入新配置
        fs::write(config_path, new_content)
            .with_context(|| format!("Failed to write migrated config to {}", config_path.display()))?;

        tracing::info!("Config migration completed successfully!");
        tracing::info!("New config written to: {}", config_path.display());

        Ok(())
    }

    pub fn load_from_user_config() -> Result<Self> {
        let path = Self::ensure_user_config_exists()?;
        Self::load(&path.to_string_lossy())
    }

    pub fn build_model_map(&self) -> HashMap<String, ModelConfig> {
        self.models
            .iter()
            .cloned()
            .map(|m| (m.name.clone(), m))
            .collect()
    }

    pub fn effective_timeouts_for_domain<'a>(&'a self, domain: Option<&str>) -> TimeoutConfig {
        if let Some(d) = domain {
            if let Some(ovr) = self.workflow.domains.get(d) {
                return TimeoutConfig {
                    analyzer_timeout_secs: ovr
                        .analyzer_timeout_secs
                        .unwrap_or(self.workflow.timeouts.analyzer_timeout_secs),
                    worker_timeout_secs: ovr
                        .worker_timeout_secs
                        .unwrap_or(self.workflow.timeouts.worker_timeout_secs),
                    synthesizer_timeout_secs: ovr
                        .synthesizer_timeout_secs
                        .unwrap_or(self.workflow.timeouts.synthesizer_timeout_secs),
                };
            }
        }
        self.workflow.timeouts.clone()
    }
}
