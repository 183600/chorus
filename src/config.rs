use anyhow::{Context, Result};
use serde::de::Error as DeError;
use serde::{de::Deserializer, Deserialize, Serialize};
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};
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

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "qwen3-coder"

[[model]]
api_base = "https://api.tbox.cn/api/llm/v1"
api_key = "your-api-key-here"
name = "ring-1t"

[workflow-integration]
json = """{
  "analyzer": {
    "ref": "glm-4.6",
    "auto_temperature": true
  },
  "workers": [
    {
      "name": "deepseek-r1",
      "temperature": 0.4
    },
    {
      "name": "deepseek-v3.2",
      "temperature": 0.4
    },
    {
      "analyzer": {
        "ref": "glm-4.6",
        "auto_temperature": true
      },
      "workers": [
        {
          "name": "kimi-k2-0905",
          "temperature": 0.4
        },
        {
          "name": "deepseek-v3.2",
          "temperature": 0.4
        },
        {
          "name": "glm-4.6",
          "temperature": 0.4
        },
        {
          "analyzer": {
            "ref": "glm-4.6",
            "auto_temperature": true
          },
          "workers": [
            {
              "name": "qwen3-coder",
              "temperature": 0.4
            },
            {
              "name": "deepseek-v3.1",
              "temperature": 0.4
            },
            {
              "name": "glm-4.6",
              "temperature": 0.4
            }
          ],
          "synthesizer": {
            "ref": "glm-4.6"
          }
        }
      ],
      "synthesizer": {
        "ref": "glm-4.6"
      }
    }
  ],
  "synthesizer": {
    "ref": "glm-4.6"
  }
}"""

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
    #[serde(
        rename = "workflow-integration",
        deserialize_with = "deserialize_workflow_plan"
    )]
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

    pub fn to_json_string(&self) -> Result<String> {
        let value = self.to_json_value()?;
        serde_json::to_string_pretty(&value)
            .with_context(|| "Failed to serialize workflow integration to JSON")
    }

    pub fn from_json_str(json: &str) -> Result<Self> {
        serde_json::from_str(json).with_context(|| "Failed to parse workflow integration JSON")
    }

    fn to_json_value(&self) -> Result<JsonValue> {
        let mut map = JsonMap::new();
        map.insert(
            "analyzer".to_string(),
            JsonValue::Object(Self::target_to_json_map(&self.analyzer, "ref")),
        );

        let mut workers = Vec::with_capacity(self.workers.len());
        for worker in &self.workers {
            workers.push(Self::worker_to_json(worker)?);
        }
        map.insert("workers".to_string(), JsonValue::Array(workers));
        map.insert(
            "synthesizer".to_string(),
            JsonValue::Object(Self::target_to_json_map(&self.synthesizer, "ref")),
        );

        Ok(JsonValue::Object(map))
    }

    fn worker_to_json(worker: &WorkflowWorker) -> Result<JsonValue> {
        match worker {
            WorkflowWorker::Model(target) => {
                Ok(JsonValue::Object(Self::target_to_json_map(target, "name")))
            }
            WorkflowWorker::Workflow(plan) => plan.to_json_value(),
        }
    }

    fn target_to_json_map(target: &WorkflowModelTarget, key: &str) -> JsonMap<String, JsonValue> {
        let mut map = JsonMap::new();
        map.insert(key.to_string(), JsonValue::String(target.model.clone()));
        if let Some(temp) = target.temperature {
            if let Some(number) = JsonNumber::from_f64(temp as f64) {
                map.insert("temperature".to_string(), JsonValue::Number(number));
            }
        }
        if let Some(auto) = target.auto_temperature {
            map.insert("auto_temperature".to_string(), JsonValue::Bool(auto));
        }
        map
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

fn deserialize_workflow_plan<'de, D>(deserializer: D) -> std::result::Result<WorkflowPlan, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct JsonWrapper {
        json: String,
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum PlanInput {
        Json(JsonWrapper),
        PlainString(String),
        Plan(WorkflowPlan),
    }

    match PlanInput::deserialize(deserializer)? {
        PlanInput::Json(wrapper) => WorkflowPlan::from_json_str(&wrapper.json)
            .map_err(|err| DeError::custom(format!("Failed to parse workflow json: {}", err))),
        PlanInput::PlainString(json) => WorkflowPlan::from_json_str(&json)
            .map_err(|err| DeError::custom(format!("Failed to parse workflow json: {}", err))),
        PlanInput::Plan(plan) => Ok(plan),
    }
}

impl Config {
    pub fn load_auto() -> Result<Self> {
        if let Ok(path) = env::var("CHORUS_CONFIG") {
            let path = PathBuf::from(path);
            if path.exists() {
                return Self::load(&path.to_string_lossy());
            } else {
                tracing::warn!(
                    "CHORUS_CONFIG points to non-existent file: {}",
                    path.display()
                );
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
        Ok(Path::new(&home)
            .join(".config")
            .join("chorus")
            .join("config.toml"))
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

        let value: Value = match toml::from_str(&content) {
            Ok(value) => value,
            Err(_) => return Ok(()),
        };

        let workflow_table = value.get("workflow-integration").and_then(Value::as_table);

        let has_json = workflow_table
            .and_then(|table| table.get("json"))
            .and_then(Value::as_str)
            .is_some();

        let legacy_fields_present = workflow_table
            .map(|table| {
                table.contains_key("analyzer_model")
                    || table.contains_key("worker_models")
                    || table.contains_key("synthesizer_model")
            })
            .unwrap_or(false);

        if has_json && !legacy_fields_present {
            return Ok(());
        }

        let mut migrations: Vec<&str> = Vec::new();

        let config = if legacy_fields_present {
            tracing::info!(
                "Detected legacy workflow-integration format, migrating to workflow plan JSON"
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

            migrations.push("workflow 节点结构");

            match toml::from_str::<LegacyConfig>(&content) {
                Ok(legacy) => Config {
                    server: legacy.server,
                    models: legacy.models,
                    workflow_integration: WorkflowPlan {
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
                    },
                    workflow: legacy.workflow,
                },
                Err(err) => {
                    tracing::warn!(
                        "Detected legacy workflow-integration fields but failed to parse legacy format: {}. Falling back to workflow plan JSON parser.",
                        err
                    );
                    toml::from_str::<Config>(&content).with_context(|| {
                        "Failed to parse config after falling back to workflow plan JSON parser"
                    })?
                }
            }
        } else {
            toml::from_str::<Config>(&content)
                .with_context(|| "Failed to parse config for migration")?
        };

        if !has_json {
            migrations.push("workflow json 格式");
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

        let new_value = Self::config_to_toml_value(&config)?;
        new_content.push_str(
            &toml::to_string_pretty(&new_value)
                .with_context(|| "Failed to serialize migrated config")?,
        );

        fs::write(config_path, new_content).with_context(|| {
            format!(
                "Failed to write migrated config to {}",
                config_path.display()
            )
        })?;

        tracing::info!(
            "Config migration completed successfully: {}",
            migrations.join("，")
        );
        tracing::info!("New config written to: {}", config_path.display());

        Ok(())
    }

    fn config_to_toml_value(config: &Config) -> Result<Value> {
        let mut root = toml::map::Map::new();

        let server_value =
            Value::try_from(&config.server).with_context(|| "Failed to serialize server config")?;
        root.insert("server".to_string(), server_value);

        let models_value =
            Value::try_from(&config.models).with_context(|| "Failed to serialize model configs")?;
        root.insert("model".to_string(), models_value);

        let workflow_json = config
            .workflow_integration
            .to_json_string()
            .with_context(|| "Failed to serialize workflow integration to JSON string")?;
        let mut workflow_integration = toml::map::Map::new();
        workflow_integration.insert("json".to_string(), Value::String(workflow_json));
        root.insert(
            "workflow-integration".to_string(),
            Value::Table(workflow_integration),
        );

        let workflow_value = Value::try_from(&config.workflow)
            .with_context(|| "Failed to serialize workflow configuration")?;
        root.insert("workflow".to_string(), workflow_value);

        Ok(Value::Table(root))
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

    pub fn effective_timeouts_for_domain(&self, domain: Option<&str>) -> TimeoutConfig {
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
