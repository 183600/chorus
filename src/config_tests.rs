#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, TimeoutConfig};

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
}
