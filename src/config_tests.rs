#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, TimeoutConfig, WorkflowWorker};

    const CFG_LEGACY: &str = r#"
[server]
host = "127.0.0.1"
port = 11435

[[model]]
api_base = "https://api.example.com/v1"
api_key = "k"
name = "m1"

[workflow-integration]
json = """{
  "analyzer": {
    "ref": "m1"
  },
  "workers": [
    {
      "name": "m1"
    }
  ],
  "synthesizer": {
    "ref": "m1"
  }
}"""

[workflow.timeouts]
analyzer_timeout_secs = 3
worker_timeout_secs = 6
synthesizer_timeout_secs = 9
"#;

    const CFG_DOMAIN_ONLY: &str = r#"
[server]
host = "127.0.0.1"
port = 11435

[[model]]
api_base = "https://api.example.com/v1"
api_key = "k"
name = "m1"

[workflow-integration]
json = """{
  "analyzer": {
    "ref": "m1"
  },
  "workers": [
    {
      "name": "m1"
    }
  ],
  "synthesizer": {
    "ref": "m1"
  }
}"""

[workflow.timeouts]
analyzer_timeout_secs = 30
worker_timeout_secs = 60
synthesizer_timeout_secs = 90

[workflow.domains]
[workflow.domains."api.example.com"]
analyzer_timeout_secs = 40
worker_timeout_secs = 80
"#;

    const CFG_DOMAIN_PARTIAL: &str = r#"
[server]
host = "127.0.0.1"
port = 11435

[[model]]
api_base = "https://app.example.com/v1"
api_key = "k"
name = "m1"

[workflow-integration]
json = """{
  "analyzer": {
    "ref": "m1"
  },
  "workers": [
    {
      "name": "m1"
    }
  ],
  "synthesizer": {
    "ref": "m1"
  }
}"""

[workflow.timeouts]
analyzer_timeout_secs = 100
worker_timeout_secs = 200
synthesizer_timeout_secs = 300

[workflow.domains]
[workflow.domains."app.example.com"]
analyzer_timeout_secs = 20
synthesizer_timeout_secs = 30
"#;

    const CFG_NESTED: &str = r#"
[server]
host = "127.0.0.1"
port = 11435

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "k"
name = "glm-4.6"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "k"
name = "deepseek-r1"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "k"
name = "deepseek-v3.2"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "k"
name = "kimi-k2-0905"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "k"
name = "deepseek-v3.1"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "k"
name = "qwen3-coder"

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
analyzer_timeout_secs = 10
worker_timeout_secs = 20
synthesizer_timeout_secs = 30
"#;

    #[test]
    fn nested_workflow_config_parses() {
        let cfg: Config = toml::from_str(CFG_NESTED).unwrap();

        assert_eq!(cfg.workflow_integration.analyzer.model, "glm-4.6");
        assert_eq!(cfg.workflow_integration.workers.len(), 3);

        let nested = match &cfg.workflow_integration.workers[2] {
            WorkflowWorker::Workflow(plan) => plan.as_ref(),
            other => panic!("expected nested workflow worker, got {:?}", other),
        };

        assert_eq!(nested.analyzer.model, "glm-4.6");
        assert_eq!(nested.workers.len(), 4);
        assert_eq!(nested.synthesizer.model, "glm-4.6");

        let deeper = match &nested.workers[3] {
            WorkflowWorker::Workflow(plan) => plan.as_ref(),
            other => panic!("expected nested workflow worker, got {:?}", other),
        };

        assert_eq!(deeper.analyzer.model, "glm-4.6");
        assert_eq!(deeper.workers.len(), 3);
        assert_eq!(deeper.synthesizer.model, "glm-4.6");

        let serialized = cfg
            .workflow_integration
            .to_json_string()
            .expect("failed to serialize nested workflow");

        let value = serde_json::from_str::<serde_json::Value>(&serialized).unwrap();

        assert_eq!(
            value["workers"][2]["workers"][3]["workers"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
    }

    #[test]
    fn legacy_timeouts_used_when_no_override() {
        let cfg: Config = toml::from_str(CFG_LEGACY).unwrap();
        let eff = cfg.effective_timeouts_for_domain(Some("api.example.com"));
        assert_eq!(eff.analyzer_timeout_secs, 3);
        assert_eq!(eff.worker_timeout_secs, 6);
        assert_eq!(eff.synthesizer_timeout_secs, 9);
    }

    #[test]
    fn domain_override_applies_fields_present() {
        let cfg: Config = toml::from_str(CFG_DOMAIN_ONLY).unwrap();
        let eff = cfg.effective_timeouts_for_domain(Some("api.example.com"));
        assert_eq!(eff.analyzer_timeout_secs, 40);
        assert_eq!(eff.worker_timeout_secs, 80);
        assert_eq!(eff.synthesizer_timeout_secs, 90); // fallback to global
    }

    #[test]
    fn partial_override_falls_back_to_global() {
        let cfg: Config = toml::from_str(CFG_DOMAIN_PARTIAL).unwrap();
        let eff = cfg.effective_timeouts_for_domain(Some("app.example.com"));
        assert_eq!(eff.analyzer_timeout_secs, 20);
        assert_eq!(eff.worker_timeout_secs, 200); // fallback
        assert_eq!(eff.synthesizer_timeout_secs, 30);
    }

    #[test]
    fn user_format_with_multiple_workers_using_name() {
        const USER_CFG: &str = r#"
# Chorus 配置文件（已自动迁移：workflow json 格式）
# 旧配置已备份到: /home/user/.config/chorus/config.toml.bak.1762015072

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "sk-test"
name = "qwen3-max"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "sk-test"
name = "qwen3-vl-plus"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "sk-test"
name = "kimi-k2-0905"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "sk-test"
name = "glm-4.6"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "sk-test"
name = "deepseek-v3.2"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "sk-test"
name = "deepseek-v3.1"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "sk-test"
name = "deepseek-r1"

[[model]]
api_base = "https://api.tbox.cn/api/llm/v1"
api_key = "sk-test"
name = "ring-1t"

[server]
host = "127.0.0.1"
port = 11435

[workflow.domains]

[workflow.timeouts]
analyzer_timeout_secs = 30000
worker_timeout_secs = 60000
synthesizer_timeout_secs = 60000

[workflow-integration]
json = """
{
  "analyzer": {
    "ref": "glm-4.6"
  },
  "synthesizer": {
    "ref": "glm-4.6"
  },
  "workers": [
    {
      "name": "qwen3-max"
    },
    {
      "name": "kimi-k2-0905"
    },
    {
      "name": "glm-4.6"
    },
    {
      "name": "deepseek-v3.2"
    },
    {
      "name": "deepseek-v3.1"
    },
    {
      "name": "deepseek-r1"
    },
    {
      "name": "ring-1t"
    }
  ]
}"""
"#;

        let cfg: Config = toml::from_str(USER_CFG).unwrap();

        assert_eq!(cfg.models.len(), 8);
        assert_eq!(cfg.workflow_integration.analyzer.model, "glm-4.6");
        assert_eq!(cfg.workflow_integration.synthesizer.model, "glm-4.6");
        assert_eq!(cfg.workflow_integration.workers.len(), 7);

        let worker_names: Vec<String> = cfg
            .workflow_integration
            .workers
            .iter()
            .map(|w| match w {
                WorkflowWorker::Model(target) => target.model.clone(),
                WorkflowWorker::Workflow(_) => panic!("expected model worker"),
            })
            .collect();

        assert_eq!(
            worker_names,
            vec![
                "qwen3-max",
                "kimi-k2-0905",
                "glm-4.6",
                "deepseek-v3.2",
                "deepseek-v3.1",
                "deepseek-r1",
                "ring-1t"
            ]
        );

        assert!(cfg.workflow.domains.is_empty());
        assert_eq!(cfg.workflow.timeouts.analyzer_timeout_secs, 30000);
        assert_eq!(cfg.workflow.timeouts.worker_timeout_secs, 60000);
        assert_eq!(cfg.workflow.timeouts.synthesizer_timeout_secs, 60000);
    }

    #[test]
    fn test_json_format_with_empty_domains() {
        const CFG: &str = r#"
[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "sk-test"
name = "qwen3-max"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "sk-test"
name = "glm-4.6"

[[model]]
api_base = "https://api.tbox.cn/api/llm/v1"
api_key = "sk-test"
name = "ring-1t"

[server]
host = "127.0.0.1"
port = 11435

[workflow.domains]

[workflow.timeouts]
analyzer_timeout_secs = 30000
synthesizer_timeout_secs = 60000
worker_timeout_secs = 60000

[workflow-integration]
json = """
{
  "analyzer": {
    "ref": "glm-4.6"
  },
  "synthesizer": {
    "ref": "glm-4.6"
  },
  "workers": [
    {
      "name": "qwen3-max"
    },
    {
      "name": "glm-4.6"
    },
    {
      "name": "ring-1t"
    }
  ]
}"""
"#;

        let cfg: Config = toml::from_str(CFG).unwrap();
        assert_eq!(cfg.models.len(), 3);
        assert_eq!(cfg.workflow_integration.analyzer.model, "glm-4.6");
        assert_eq!(cfg.workflow_integration.synthesizer.model, "glm-4.6");
        assert_eq!(cfg.workflow_integration.workers.len(), 3);
        
        let worker_names: Vec<String> = cfg
            .workflow_integration
            .workers
            .iter()
            .map(|w| match w {
                WorkflowWorker::Model(target) => target.model.clone(),
                WorkflowWorker::Workflow(_) => panic!("expected model worker"),
            })
            .collect();

        assert_eq!(worker_names, vec!["qwen3-max", "glm-4.6", "ring-1t"]);
        assert!(cfg.workflow.domains.is_empty());
    }
}
