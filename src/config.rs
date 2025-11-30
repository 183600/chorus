use crate::error::AppError;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

#[derive(Parser, Debug)]
#[command(name = "chorus")]
#[command(about = "LLM API Aggregation Service", long_about = None)]
pub struct Cli {
    #[arg(long, help = "Path to configuration file")]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub model: Vec<ModelConfig>,
    #[serde(rename = "workflow-integration")]
    pub workflow_integration: WorkflowIntegrationConfig,
    pub workflow: WorkflowConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 11435,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    #[serde(rename = "api_base")]
    pub api_base: String,
    #[serde(rename = "api_key")]
    pub api_key: String,
    #[serde(rename = "auto_temperature")]
    pub auto_temperature: bool,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowIntegrationConfig {
    #[serde(rename = "nested_worker_depth")]
    pub nested_worker_depth: usize,
    pub json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConfig {
    pub timeouts: WorkflowTimeouts,
    pub domains: HashMap<String, DomainOverrides>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowTimeouts {
    #[serde(rename = "analyzer_timeout_secs")]
    pub analyzer_timeout_secs: u64,
    #[serde(rename = "worker_timeout_secs")]
    pub worker_timeout_secs: u64,
    #[serde(rename = "synthesizer_timeout_secs")]
    pub synthesizer_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainOverrides {
    #[serde(rename = "analyzer_timeout_secs")]
    pub analyzer_timeout_secs: Option<u64>,
    #[serde(rename = "worker_timeout_secs")]
    pub worker_timeout_secs: Option<u64>,
    #[serde(rename = "synthesizer_timeout_secs")]
    pub synthesizer_timeout_secs: Option<u64>,
}

impl Config {
    pub fn load() -> Result<Self, AppError> {
        let cli = Cli::parse();
        
        // Priority 1: CLI argument
        if let Some(config_path) = cli.config {
            return Self::from_file(&config_path);
        }

        // Priority 2: Environment variable
        if let Ok(env_path) = std::env::var("CHORUS_CONFIG") {
            return Self::from_file(Path::new(&env_path));
        }

        // Priority 3: Default path
        let default_path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("chorus")
            .join("config.toml");

        if default_path.exists() {
            Self::from_file(&default_path)
        } else {
            warn!("No configuration file found, using defaults with minimal setup");
            Self::default_config()
        }
    }

    fn from_file(path: &Path) -> Result<Self, AppError> {
        info!("Loading configuration from: {}", path.display());
        let content = fs::read_to_string(path)
            .map_err(|e| AppError::Config(format!("Failed to read config file: {}", e)))?;
        
        let mut config: Config = toml::from_str(&content)?;
        
        // Validate workflow references
        config.validate_workflow()?;
        
        // Apply default timeouts if missing
        config.apply_defaults();
        
        debug!("Configuration loaded successfully: {:?}", config);
        Ok(config)
    }

    fn default_config() -> Result<Self, AppError> {
        Ok(Config {
            server: ServerConfig::default(),
            model: vec![],
            workflow_integration: WorkflowIntegrationConfig {
                nested_worker_depth: 1,
                json: r#"{"analyzer": {"ref": "default"}, "workers": [], "selector": {"ref": "default"}, "synthesizer": {"ref": "default"}}"#.to_string(),
            },
            workflow: WorkflowConfig {
                timeouts: WorkflowTimeouts {
                    analyzer_timeout_secs: 30,
                    worker_timeout_secs: 60,
                    synthesizer_timeout_secs: 60,
                },
                domains: HashMap::new(),
            },
        })
    }

    fn validate_workflow(&self) -> Result<(), AppError> {
        let workflow: WorkflowJson = serde_json::from_str(&self.workflow_integration.json)?;
        
        let model_names: std::collections::HashSet<_> = self.model.iter()
            .map(|m| m.name.as_str())
            .collect();
        
        let mut missing_models = Vec::new();
        
        // Validate analyzer ref
        if let Some(ref_name) = &workflow.analyzer.ref_name {
            if !model_names.contains(ref_name.as_str()) {
                missing_models.push(format!("analyzer '{}'", ref_name));
            }
        }
        
        // Validate workers
        for (i, worker) in workflow.workers.iter().enumerate() {
            if let Some(ref_name) = &worker.ref_name {
                if !model_names.contains(ref_name.as_str()) {
                    missing_models.push(format!("worker[{}] '{}'", i, ref_name));
                }
            }
        }
        
        // Validate selector ref
        if let Some(ref_name) = &workflow.selector.ref_name {
            if !model_names.contains(ref_name.as_str()) {
                missing_models.push(format!("selector '{}'", ref_name));
            }
        }
        
        // Validate synthesizer ref
        if let Some(ref_name) = &workflow.synthesizer.ref_name {
            if !model_names.contains(ref_name.as_str()) {
                missing_models.push(format!("synthesizer '{}'", ref_name));
            }
        }
        
        if !missing_models.is_empty() {
            return Err(AppError::WorkflowValidation(format!(
                "Workflow configuration references undefined model(s): {}",
                missing_models.join(", ")
            )));
        }
        
        debug!("Workflow validation passed");
        Ok(())
    }

    fn apply_defaults(&mut self) {
        if self.model.is_empty() {
            warn!("No models configured, using empty model list");
        }
        
        // Ensure all timeouts are set
        if self.workflow.timeouts.analyzer_timeout_secs == 0 {
            self.workflow.timeouts.analyzer_timeout_secs = 30;
        }
        if self.workflow.timeouts.worker_timeout_secs == 0 {
            self.workflow.timeouts.worker_timeout_secs = 60;
        }
        if self.workflow.timeouts.synthesizer_timeout_secs == 0 {
            self.workflow.timeouts.synthesizer_timeout_secs = 60;
        }
    }

    pub fn get_model(&self, name: &str) -> Option<&ModelConfig> {
        self.model.iter().find(|m| m.name == name)
    }

    pub fn get_domain_timeouts(&self, domain: &str) -> DomainTimeouts {
        let defaults = &self.workflow.timeouts;
        if let Some(overrides) = self.workflow.domains.get(domain) {
            DomainTimeouts {
                analyzer: overrides.analyzer_timeout_secs.unwrap_or(defaults.analyzer_timeout_secs),
                worker: overrides.worker_timeout_secs.unwrap_or(defaults.worker_timeout_secs),
                synthesizer: overrides.synthesizer_timeout_secs.unwrap_or(defaults.synthesizer_timeout_secs),
            }
        } else {
            DomainTimeouts {
                analyzer: defaults.analyzer_timeout_secs,
                worker: defaults.worker_timeout_secs,
                synthesizer: defaults.synthesizer_timeout_secs,
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkflowJson {
    pub analyzer: NodeRef,
    pub workers: Vec<NodeRef>,
    pub selector: NodeRef,
    pub synthesizer: NodeRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodeRef {
    #[serde(rename = "ref")]
    pub ref_name: Option<String>,
    #[serde(rename = "temperature")]
    pub temperature: Option<f32>,
}

pub struct DomainTimeouts {
    pub analyzer: u64,
    pub worker: u64,
    pub synthesizer: u64,
}
