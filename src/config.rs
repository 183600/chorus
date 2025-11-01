use anyhow::{Context, Result};
use serde::{de::Deserializer, Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use toml::Value;

const DEFAULT_CONFIG: &str = r#"# Chorus 默认配置
[server]
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

auto_temperature = true

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "deepseek-v3.2"

auto_temperature = true

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "deepseek-v3.1"

auto_temperature = true

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "deepseek-r1"

auto_temperature = true

[workflow-integration.analyzer]
ref = "glm-4.6"
auto_temperature = true

[[workflow-integration.workers]]
name = "qwen3-max"
temperature = 0.4

[[workflow-integration.workers]]
name = "qwen3-vl-plus"
temperature = 0.4

[[workflow-integration.workers]]
name = "kimi-k2-0905"
temperature = 0.4

[[workflow-integration.workers]]
name = "glm-4.6"
temperature = 0.4

[[workflow-integration.workers]]
name = "deepseek-v3.2"
temperature = 0.4

[[workflow-integration.workers]]
name = "deepseek-v3.1"
temperature = 0.4

[[workflow-integration.workers]]
name = "deepseek-r1"
temperature = 0.4

[workflow-integration.synthesizer]
ref = "glm-4.6"

[workflow.timeouts]
analyzer_timeout_secs = 30000
worker_timeout_secs = 60000
synthesizer_timeout_secs = 60000

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
    #[serde(rename = "model", deserialize_with = "deserialize_models")]
    pub models: Vec<ModelConfig>,
    #[serde(rename = "workflow-integration")]
    pub workflow_integration: WorkflowPlan,
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
pub struct WorkflowPlan {
    pub analyzer: WorkflowModelTarget,
    #[serde(default)]
    pub workers: Vec<WorkflowWorker>,
    pub synthesizer: WorkflowModelTarget,
}

impl WorkflowPlan {
    pub fn label(&self) -> String {
        format!("workflow:{}", self.synthesizer.model)
    }

    pub fn worker_labels(&self) -> Vec<String> {
        self.workers.iter().map(WorkflowWorker::label).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowModelTarget {
    #[serde(rename = "ref", alias = "name")]
    pub model: String,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub auto_temperature: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WorkflowWorker {
    Model(WorkflowModelTarget),
    Workflow(Box<WorkflowPlan>),
}

impl WorkflowWorker {
    pub fn label(&self) -> String {
        match self {
            WorkflowWorker::Model(target) => target.model.clone(),
            WorkflowWorker::Workflow(plan) => plan.label(),
        }
    }
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
mod tests_support {
    pub use super::*;
}

impl Config {
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
            Self::migrate_config_if_needed(&path)?;
        }
        Ok(path)
    }

    fn migrate_config_if_needed(config_path: &Path) -> Result<()> {
        let content = fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

        let mut value: Value = match toml::from_str(&content) {
            Ok(value) => value,
            Err(_) => return Ok(()),
        };

        let mut migrations: Vec<&str> = Vec::new();

        let legacy_detected = value
            .get("workflow-integration")
            .and_then(Value::as_table)
            .map(|table| {
                table.contains_key("analyzer_model")
                    || table.contains_key("worker_models")
                    || table.contains_key("synthesizer_model")
            })
            .unwrap_or(false);

        if legacy_detected {
            tracing::info!(
                "Detected legacy workflow-integration format, migrating to analyzer/workers/synthesizer map"
            );

            #[derive(Deserialize)]
            struct LegacyWorkflowIntegration {
                analyzer_model: String,
                worker_models: Vec<String>,
                synthesizer_model: String,
            }

            #[derive(Deserialize)]
            struct LegacyConfig {
                server: ServerConfig,
                #[serde(rename = "model", deserialize_with = "deserialize_models")]
                models: Vec<ModelConfig>,
                #[serde(rename = "workflow-integration")]
                workflow_integration: LegacyWorkflowIntegration,
                workflow: WorkflowConfig,
            }

            let legacy: LegacyConfig = toml::from_str(&content)
                .with_context(|| "Failed to parse legacy config format")?;

            let plan = WorkflowPlan {
                analyzer: WorkflowModelTarget {
                    model: legacy.workflow_integration.analyzer_model,
                    temperature: None,
                    auto_temperature: None,
                },
                workers: legacy
                    .workflow_integration
                    .worker_models
                    .into_iter()
                    .map(|model| {
                        WorkflowWorker::Model(WorkflowModelTarget {
                            model,
                            temperature: None,
                            auto_temperature: None,
                        })
                    })
                    .collect(),
                synthesizer: WorkflowModelTarget {
                    model: legacy.workflow_integration.synthesizer_model,
                    temperature: None,
                    auto_temperature: None,
                },
            };

            let new_config = Config {
                server: legacy.server,
                models: legacy.models,
                workflow_integration: plan,
                workflow: legacy.workflow,
            };

            value = toml::from_str(
                &toml::to_string_pretty(&new_config)
                    .with_context(|| "Failed to serialize migrated config")?,
            )
            .with_context(|| "Failed to parse migrated config back into value")?;

            migrations.push("workflow 节点结构");
        }

        if Self::migrate_worker_name_fields(&mut value) {
            tracing::info!(
                "Detected workflow workers using `ref`, migrating them to the new `name` field"
            );
            migrations.push("workers.name 字段");
        }

        if migrations.is_empty() {
            return Ok(());
        }

        let backup_path = Self::backup_config_file(config_path)?;
        tracing::info!("Old config backed up to: {}", backup_path.display());

        let mut new_content = String::new();
        new_content.push_str(&format!(
            "# Chorus 配置文件（已自动迁移：{}）\n",
            migrations.join("，")
        ));
        new_content.push_str(&format!("# 旧配置已备份到: {}\n\n", backup_path.display()));
        new_content.push_str(
            &toml::to_string_pretty(&value)
                .with_context(|| "Failed to serialize migrated config")?,
        );

        fs::write(config_path, new_content)
            .with_context(|| format!("Failed to write migrated config to {}", config_path.display()))?;

        tracing::info!(
            "Config migration completed successfully: {}",
            migrations.join(", ")
        );
        tracing::info!("New config written to: {}", config_path.display());

        Ok(())
    }

    fn migrate_worker_name_fields(value: &mut Value) -> bool {
        if let Value::Table(table) = value {
            if let Some(workflow) = table.get_mut("workflow-integration") {
                return Self::migrate_worker_name_fields_in_plan(workflow);
            }
        }
        false
    }

    fn migrate_worker_name_fields_in_plan(plan: &mut Value) -> bool {
        if let Value::Table(map) = plan {
            let mut changed = false;
            if let Some(workers) = map.get_mut("workers") {
                if let Value::Array(array) = workers {
                    for worker in array {
                        if Self::migrate_worker_name_fields_in_worker(worker) {
                            changed = true;
                        }
                    }
                }
            }
            changed
        } else {
            false
        }
    }

    fn migrate_worker_name_fields_in_worker(worker: &mut Value) -> bool {
        if let Value::Table(table) = worker {
            if table.contains_key("analyzer")
                || table.contains_key("workers")
                || table.contains_key("synthesizer")
            {
                return Self::migrate_worker_name_fields_in_plan(worker);
            }

            if table.contains_key("ref") && !table.contains_key("name") {
                if let Some(value) = table.remove("ref") {
                    table.insert("name".to_string(), value);
                    return true;
                }
            }
        }
        false
    }

    fn backup_config_file(config_path: &Path) -> Result<PathBuf> {
        let mut backup_path = config_path.with_extension("toml.bak");
        if backup_path.exists() {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            backup_path = config_path.with_extension(format!("toml.bak.{}", timestamp));
        }

        fs::copy(config_path, &backup_path)
            .with_context(|| format!("Failed to backup config to {}", backup_path.display()))?;

        Ok(backup_path)
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
