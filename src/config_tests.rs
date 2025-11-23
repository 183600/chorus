#[cfg(test)]
mod tests {
    use crate::config::{Config, WorkflowWorker};

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
name = "kimi-k2-0905"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "k"
name = "deepseek-v3.1"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "k"
name = "qwen3-coder"

[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "k"
name = "qwen3-max"

[workflow-integration]
json = """{
  "analyzer": {
    "ref": "glm-4.6",
    "auto_temperature": true
  },
  "workers": [
    {
      "name": "deepseek-v3.1",
      "temperature": 1
    },
    {
      "analyzer": {
        "ref": "glm-4.6",
        "auto_temperature": true
      },
      "workers": [
        {
          "name": "kimi-k2-0905",
          "temperature": 1
        },
        {
          "name": "deepseek-v3.1",
          "temperature": 1
        },
        {
          "name": "glm-4.6",
          "temperature": 1
        },
        {
          "analyzer": {
            "ref": "glm-4.6",
            "auto_temperature": true
          },
          "workers": [
            {
              "name": "qwen3-coder",
              "temperature": 1
            },
            {
              "name": "deepseek-v3.1",
              "temperature": 1
            },
            {
              "name": "qwen3-max",
              "temperature": 1
            }
          ],
          "synthesizer": {
            "ref": "qwen3-max"
          }
        }
      ],
      "synthesizer": {
        "ref": "qwen3-max"
      }
    }
  ],
  "synthesizer": {
    "ref": "qwen3-max"
  },
  "selector": {
    "ref": "qwen3-max"
  }
}"""

[workflow.timeouts]
analyzer_timeout_secs = 10
worker_timeout_secs = 20
synthesizer_timeout_secs = 30
"#;

    fn simple_nested_cfg(depth: usize) -> String {
        format!(
            r#"
[server]
host = "127.0.0.1"
port = 11435

[[model]]
api_base = "https://api.example.com/v1"
api_key = "k"
name = "glm-4.6"

[[model]]
api_base = "https://api.example.com/v1"
api_key = "k"
name = "kimi-k2-0905"

[[model]]
api_base = "https://api.example.com/v1"
api_key = "k"
name = "qwen3-coder"

[workflow-integration]
nested_worker_depth = {depth}
json = """{{
  "analyzer": {{
    "ref": "glm-4.6"
  }},
  "workers": [
    {{
      "name": "kimi-k2-0905"
    }},
    {{
      "name": "qwen3-coder",
      "temperature": 0.6
    }}
  ],
  "synthesizer": {{
    "ref": "qwen3-coder"
  }}
}}"""

[workflow.timeouts]
analyzer_timeout_secs = 1
worker_timeout_secs = 1
synthesizer_timeout_secs = 1
"#,
            depth = depth
        )
    }

    #[test]
    fn nested_workflow_config_parses() {
        let cfg: Config = toml::from_str(CFG_NESTED).unwrap();

        assert_eq!(cfg.workflow_integration.analyzer.model, "glm-4.6");
        assert_eq!(cfg.workflow_integration.workers.len(), 2);
        assert_eq!(
            cfg.workflow_integration
                .synthesizer
                .as_ref()
                .map(|t| t.model.as_str()),
            Some("qwen3-max")
        );
        assert_eq!(
            cfg.workflow_integration
                .selector
                .as_ref()
                .map(|t| t.model.as_str()),
            Some("qwen3-max")
        );

        let nested = match &cfg.workflow_integration.workers[1] {
            WorkflowWorker::Workflow(plan) => plan.as_ref(),
            other => panic!("expected nested workflow worker, got {:?}", other),
        };

        assert_eq!(nested.analyzer.model, "glm-4.6");
        assert_eq!(nested.workers.len(), 4);
        assert_eq!(
            nested.synthesizer.as_ref().map(|t| t.model.as_str()),
            Some("qwen3-max")
        );
        assert!(
            nested.selector.is_none(),
            "nested workflow selector should be None"
        );

        let deeper = match &nested.workers[3] {
            WorkflowWorker::Workflow(plan) => plan.as_ref(),
            other => panic!("expected nested workflow worker, got {:?}", other),
        };

        assert_eq!(deeper.analyzer.model, "glm-4.6");
        assert_eq!(deeper.workers.len(), 3);
        assert_eq!(
            deeper.synthesizer.as_ref().map(|t| t.model.as_str()),
            Some("qwen3-max")
        );
        assert!(
            deeper.selector.is_none(),
            "deepest workflow selector should be None"
        );

        let serialized = cfg
            .workflow_integration
            .to_json_string()
            .expect("failed to serialize nested workflow");

        let value = serde_json::from_str::<serde_json::Value>(&serialized).unwrap();

        assert_eq!(value["selector"]["ref"].as_str(), Some("qwen3-max"));
        assert!(
            value["workers"][1].get("selector").is_none(),
            "nested workflow should not serialize selector when absent"
        );
        assert!(
            value["workers"][1]["workers"][3].get("selector").is_none(),
            "deep workflow should not serialize selector when absent"
        );
        assert_eq!(
            value["workers"][1]["workers"][3]["workers"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
    }

    #[test]
    fn nested_worker_depth_expands_models() {
        let cfg: Config = toml::from_str(&simple_nested_cfg(2)).unwrap();

        assert_eq!(cfg.workflow_integration.workers.len(), 2);

        let first = match &cfg.workflow_integration.workers[0] {
            WorkflowWorker::Workflow(plan) => plan.as_ref(),
            other => panic!("expected nested workflow worker, got {:?}", other),
        };
        assert_eq!(first.workers.len(), 2);
        for worker in &first.workers {
            match worker {
                WorkflowWorker::Model(target) => {
                    assert_eq!(target.model, "kimi-k2-0905");
                }
                other => panic!("expected duplicated model worker, got {:?}", other),
            }
        }

        let second = match &cfg.workflow_integration.workers[1] {
            WorkflowWorker::Workflow(plan) => plan.as_ref(),
            other => panic!("expected nested workflow worker, got {:?}", other),
        };
        assert_eq!(second.workers.len(), 2);
        for worker in &second.workers {
            match worker {
                WorkflowWorker::Model(target) => {
                    assert_eq!(target.model, "qwen3-coder");
                }
                other => panic!("expected duplicated model worker, got {:?}", other),
            }
        }
    }

    #[test]
    fn nested_worker_depth_supports_multiple_layers() {
        let cfg: Config = toml::from_str(&simple_nested_cfg(3)).unwrap();

        let first = match &cfg.workflow_integration.workers[0] {
            WorkflowWorker::Workflow(plan) => plan.as_ref(),
            other => panic!("expected nested workflow worker, got {:?}", other),
        };
        assert_eq!(first.workers.len(), 2);
        for worker in &first.workers {
            match worker {
                WorkflowWorker::Workflow(plan) => {
                    assert_eq!(plan.workers.len(), 2);
                    for leaf in &plan.workers {
                        match leaf {
                            WorkflowWorker::Model(target) => {
                                assert_eq!(target.model, "kimi-k2-0905");
                            }
                            other => panic!("expected model worker, got {:?}", other),
                        }
                    }
                }
                other => panic!("expected nested workflow worker, got {:?}", other),
            }
        }

        let second = match &cfg.workflow_integration.workers[1] {
            WorkflowWorker::Workflow(plan) => plan.as_ref(),
            other => panic!("expected nested workflow worker, got {:?}", other),
        };
        assert_eq!(second.workers.len(), 2);
        for worker in &second.workers {
            match worker {
                WorkflowWorker::Workflow(plan) => {
                    assert_eq!(plan.workers.len(), 2);
                    for leaf in &plan.workers {
                        match leaf {
                            WorkflowWorker::Model(target) => {
                                assert_eq!(target.model, "qwen3-coder");
                            }
                            other => panic!("expected model worker, got {:?}", other),
                        }
                    }
                }
                other => panic!("expected nested workflow worker, got {:?}", other),
            }
        }
    }

    #[test]
    fn nested_worker_depth_rejects_zero() {
        let cfg = simple_nested_cfg(0);
        let err = toml::from_str::<Config>(&cfg).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("nested_worker_depth"),
            "unexpected error message: {}",
            message
        );
    }

    #[test]
    fn workflow_json_errors_include_underlying_message() {
        const CFG: &str = r#"
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
  ]
}"""

[workflow.timeouts]
analyzer_timeout_secs = 1
worker_timeout_secs = 1
synthesizer_timeout_secs = 1
"#;

        let err = toml::from_str::<Config>(CFG).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("must define at least one of `synthesizer` or `selector`"),
            "error message should include missing aggregator detail, got: {}",
            message
        );
    }

    #[test]
    fn workflow_selector_without_synthesizer_parses() {
        const CFG: &str = r#"
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
  "selector": {
    "ref": "m1"
  }
}"""

[workflow.timeouts]
analyzer_timeout_secs = 1
worker_timeout_secs = 1
synthesizer_timeout_secs = 1
"#;

        let cfg: Config = toml::from_str(CFG).unwrap();
        assert!(cfg.workflow_integration.synthesizer.is_none());
        assert_eq!(
            cfg.workflow_integration
                .selector
                .as_ref()
                .map(|t| t.model.as_str()),
            Some("m1")
        );

        let serialized = cfg
            .workflow_integration
            .to_json_string()
            .expect("selector-only workflow should serialize");
        assert!(
            !serialized.contains("\"synthesizer\""),
            "selector-only workflow should not include synthesizer in serialized JSON"
        );
    }

    #[test]
    fn nested_selector_does_not_inherit_parent_synthesizer() {
        const CFG: &str = r#"
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
      "analyzer": {
        "ref": "m1"
      },
      "workers": [
        {
          "analyzer": {
            "ref": "m1"
          },
          "workers": [
            {
              "name": "m1"
            }
          ]
        }
      ],
      "selector": {
        "ref": "m1"
      }
    }
  ],
  "synthesizer": {
    "ref": "m1"
  }
}"""

[workflow.timeouts]
analyzer_timeout_secs = 1
worker_timeout_secs = 1
synthesizer_timeout_secs = 1
"#;

        let cfg: Config = toml::from_str(CFG).unwrap();

        let nested = match &cfg.workflow_integration.workers[0] {
            WorkflowWorker::Workflow(plan) => plan.as_ref(),
            other => panic!("expected nested workflow worker, got {:?}", other),
        };

        assert!(
            nested.synthesizer.is_none(),
            "nested workflow with selector should not inherit synthesizer"
        );
        assert_eq!(
            nested.selector.as_ref().map(|t| t.model.as_str()),
            Some("m1")
        );

        let parent_synth = cfg
            .workflow_integration
            .synthesizer
            .as_ref()
            .expect("parent synthesizer should be present");

        let inner = match &nested.workers[0] {
            WorkflowWorker::Workflow(plan) => plan.as_ref(),
            other => panic!("expected inner workflow worker, got {:?}", other),
        };

        let inner_synth = inner
            .synthesizer
            .as_ref()
            .expect("inner workflow should inherit synthesizer when selector absent");
        assert_eq!(inner_synth.model, parent_synth.model);
    }

    #[test]
    fn nested_workflow_missing_synthesizer_inherits_parent() {
        const CFG: &str = r#"
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
      "analyzer": {
        "ref": "m1"
      },
      "workers": [
        {
          "analyzer": {
            "ref": "m1"
          },
          "workers": [
            {
              "name": "m1"
            }
          ]
        }
      ]
    }
  ],
  "synthesizer": {
    "ref": "m1"
  }
}"""

[workflow.timeouts]
analyzer_timeout_secs = 1
worker_timeout_secs = 1
synthesizer_timeout_secs = 1
"#;

        let cfg: Config = toml::from_str(CFG).unwrap();

        let nested = match &cfg.workflow_integration.workers[0] {
            WorkflowWorker::Workflow(plan) => plan.as_ref(),
            other => panic!("expected nested workflow worker, got {:?}", other),
        };

        let parent_synth = cfg
            .workflow_integration
            .synthesizer
            .as_ref()
            .expect("parent synthesizer should be present");
        let nested_synth = nested
            .synthesizer
            .as_ref()
            .expect("nested synthesizer should be inherited");
        assert_eq!(nested_synth.model, parent_synth.model);

        let deeper = match &nested.workers[0] {
            WorkflowWorker::Workflow(plan) => plan.as_ref(),
            other => panic!("expected nested workflow worker, got {:?}", other),
        };

        let deeper_synth = deeper
            .synthesizer
            .as_ref()
            .expect("deeper synthesizer should be inherited");
        assert_eq!(deeper_synth.model, parent_synth.model);

        let deepest_worker = match &deeper.workers[0] {
            WorkflowWorker::Model(target) => target,
            other => panic!("expected model worker, got {:?}", other),
        };

        assert_eq!(deepest_worker.model, "m1");
    }

    #[test]
    fn inline_workflow_plan_inherits_parent_synthesizer() {
        const CFG: &str = r#"
[server]
host = "127.0.0.1"
port = 11435

[[model]]
api_base = "https://api.example.com/v1"
api_key = "k"
name = "m1"

[workflow-integration]
analyzer = { ref = "m1" }
workers = [
  { analyzer = { ref = "m1" }, workers = [
      { name = "m1" },
      { analyzer = { ref = "m1" }, workers = [
          { name = "m1" }
      ] }
  ] }
]
synthesizer = { ref = "m1" }

[workflow.timeouts]
analyzer_timeout_secs = 5
worker_timeout_secs = 5
synthesizer_timeout_secs = 5
"#;

        let cfg: Config = toml::from_str(CFG).unwrap();

        let nested = match &cfg.workflow_integration.workers[0] {
            WorkflowWorker::Workflow(plan) => plan.as_ref(),
            other => panic!("expected nested workflow worker, got {:?}", other),
        };

        let parent_synth = cfg
            .workflow_integration
            .synthesizer
            .as_ref()
            .expect("parent synthesizer should be present");
        let nested_synth = nested
            .synthesizer
            .as_ref()
            .expect("nested workflow should inherit synthesizer");
        assert_eq!(nested_synth.model, parent_synth.model);

        let deeper = match &nested.workers[1] {
            WorkflowWorker::Workflow(plan) => plan.as_ref(),
            other => panic!("expected deeper nested workflow, got {:?}", other),
        };

        let deeper_synth = deeper
            .synthesizer
            .as_ref()
            .expect("deeper workflow should inherit synthesizer");
        assert_eq!(deeper_synth.model, parent_synth.model);
    }

    #[test]
    fn inline_nested_selector_does_not_inherit_parent_synthesizer() {
        const CFG: &str = r#"
[server]
host = "127.0.0.1"
port = 11435

[[model]]
api_base = "https://api.example.com/v1"
api_key = "k"
name = "m1"

[workflow-integration]
analyzer = { ref = "m1" }
workers = [
  { analyzer = { ref = "m1" }, workers = [
      { analyzer = { ref = "m1" }, workers = [
          { name = "m1" }
      ] }
  ], selector = { ref = "m1" } }
]
synthesizer = { ref = "m1" }

[workflow.timeouts]
analyzer_timeout_secs = 5
worker_timeout_secs = 5
synthesizer_timeout_secs = 5
"#;

        let cfg: Config = toml::from_str(CFG).unwrap();

        let nested = match &cfg.workflow_integration.workers[0] {
            WorkflowWorker::Workflow(plan) => plan.as_ref(),
            other => panic!("expected nested workflow worker, got {:?}", other),
        };

        assert!(
            nested.synthesizer.is_none(),
            "nested inline workflow with selector should not inherit synthesizer"
        );
        assert_eq!(
            nested.selector.as_ref().map(|t| t.model.as_str()),
            Some("m1")
        );

        let parent_synth = cfg
            .workflow_integration
            .synthesizer
            .as_ref()
            .expect("parent synthesizer should be present");

        let inner = match &nested.workers[0] {
            WorkflowWorker::Workflow(plan) => plan.as_ref(),
            other => panic!("expected inner workflow worker, got {:?}", other),
        };

        let inner_synth = inner
            .synthesizer
            .as_ref()
            .expect("inner inline workflow should inherit synthesizer when selector absent");
        assert_eq!(inner_synth.model, parent_synth.model);
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
# 所有超时时间单位均为秒
analyzer_timeout_secs = 30
worker_timeout_secs = 60
synthesizer_timeout_secs = 60

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

        assert_eq!(cfg.models.len(), 7);
        assert_eq!(cfg.workflow_integration.analyzer.model, "glm-4.6");
        assert_eq!(
            cfg.workflow_integration
                .synthesizer
                .as_ref()
                .map(|t| t.model.as_str()),
            Some("glm-4.6")
        );
        assert_eq!(cfg.workflow_integration.workers.len(), 6);

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
                "deepseek-v3.1",
                "deepseek-r1",
                "ring-1t"
            ]
        );

        assert!(cfg.workflow.domains.is_empty());
        assert_eq!(cfg.workflow.timeouts.analyzer_timeout_secs, 30);
        assert_eq!(cfg.workflow.timeouts.worker_timeout_secs, 60);
        assert_eq!(cfg.workflow.timeouts.synthesizer_timeout_secs, 60);
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
# 所有超时时间单位均为秒
analyzer_timeout_secs = 30
synthesizer_timeout_secs = 60
worker_timeout_secs = 60

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
        assert_eq!(
            cfg.workflow_integration
                .synthesizer
                .as_ref()
                .map(|t| t.model.as_str()),
            Some("glm-4.6")
        );
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

    #[test]
    fn migration_falls_back_when_legacy_analyzer_missing() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        struct HomeGuard {
            original: Option<String>,
        }

        impl Drop for HomeGuard {
            fn drop(&mut self) {
                if let Some(ref value) = self.original {
                    std::env::set_var("HOME", value);
                } else {
                    std::env::remove_var("HOME");
                }
            }
        }

        let temp_dir = std::env::temp_dir().join(format!(
            "chorus_partial_legacy_test_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));

        let home_guard = HomeGuard {
            original: std::env::var("HOME").ok(),
        };
        std::env::set_var("HOME", temp_dir.as_os_str());

        let config_dir = temp_dir.join(".config/chorus");
        fs::create_dir_all(&config_dir).unwrap();

        let config_path = config_dir.join("config.toml");
        let config_content = r#"
[server]
host = "127.0.0.1"
port = 11435

[[model]]
api_base = "https://api.example.com/v1"
api_key = "k"
name = "glm-4.6"

[[model]]
api_base = "https://api.example.com/v1"
api_key = "k"
name = "qwen3-max"

[workflow-integration]
worker_models = ["qwen3-max", "glm-4.6"]
synthesizer_model = "glm-4.6"
json = """
{
  "analyzer": {
    "ref": "glm-4.6"
  },
  "workers": [
    {
      "name": "qwen3-max"
    },
    {
      "name": "glm-4.6"
    }
  ],
  "synthesizer": {
    "ref": "glm-4.6"
  }
}"""

[workflow.timeouts]
# 所有超时时间单位均为秒
analyzer_timeout_secs = 30
worker_timeout_secs = 60
synthesizer_timeout_secs = 60

[workflow.domains]
"#;
        fs::write(&config_path, config_content).unwrap();

        let cfg = Config::load_from_user_config().unwrap();

        let migrated = fs::read_to_string(&config_path).unwrap();
        assert!(!migrated.contains("analyzer_model"));
        assert!(!migrated.contains("worker_models"));
        assert!(!migrated.contains("synthesizer_model"));
        assert!(migrated.contains("[workflow-integration]"));
        assert!(migrated.contains("json = \"\"\""));

        assert_eq!(cfg.workflow_integration.analyzer.model, "glm-4.6");
        assert_eq!(cfg.workflow_integration.workers.len(), 2);
        assert_eq!(
            cfg.workflow_integration
                .synthesizer
                .as_ref()
                .map(|t| t.model.as_str()),
            Some("glm-4.6")
        );

        let _ = fs::remove_dir_all(&temp_dir);
        drop(home_guard);
    }
}
