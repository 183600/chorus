mod config;
mod llm;
mod server;
mod workflow;

#[cfg(test)]
mod config_tests;

use anyhow::{anyhow, Result};
use std::{env, sync::Arc};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "chorus=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut raw_args = env::args();
    raw_args.next(); // 跳过可执行文件名
    let cli_args: Vec<String> = raw_args.collect();
    let config_override = parse_config_override_from_args(&cli_args)?;

    // 自动加载配置（CLI > 环境变量 CHORUS_CONFIG > ~/.config/chorus/config.toml）
    let config = if let Some(path) = config_override {
        tracing::info!("Loading config from CLI override: {}", path);
        config::Config::load(&path)?
    } else {
        config::Config::load_auto()?
    };
    let host = config.server.host.clone();
    let port = config.server.port;
    let worker_labels = config.workflow_integration.worker_labels();

    tracing::info!("Starting Chorus server on {}:{}", host, port);
    tracing::info!(
        "Analyzer model: {}",
        config.workflow_integration.analyzer.model
    );
    tracing::info!("Worker nodes: {:?}", worker_labels);
    if let Some(synth) = &config.workflow_integration.synthesizer {
        tracing::info!("Synthesizer model: {}", synth.model);
    } else if let Some(selector) = &config.workflow_integration.selector {
        tracing::info!(
            "No synthesizer configured; using selector: {}",
            selector.model
        );
    } else {
        tracing::warn!("No synthesizer or selector configured for the workflow");
    }

    // 启动服务器
    server::start_server(Arc::new(config)).await?;

    Ok(())
}

fn parse_config_override_from_args(args: &[String]) -> Result<Option<String>> {
    let mut override_path: Option<String> = None;
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];

        if arg == "--" {
            break;
        }

        if arg == "--config" || arg == "-c" {
            let value = args
                .get(index + 1)
                .ok_or_else(|| anyhow!("Expected a file path after the `{}` flag", arg))?
                .clone();
            set_config_override(&mut override_path, &value, arg)?;
            index += 2;
            continue;
        }

        if let Some(rest) = arg.strip_prefix("--config=") {
            set_config_override(&mut override_path, rest, "--config")?;
            index += 1;
            continue;
        }

        if let Some(rest) = arg.strip_prefix("-c=") {
            set_config_override(&mut override_path, rest, "-c")?;
            index += 1;
            continue;
        }

        index += 1;
    }

    Ok(override_path)
}

fn set_config_override(target: &mut Option<String>, candidate: &str, flag: &str) -> Result<()> {
    if candidate.trim().is_empty() {
        return Err(anyhow!("{} flag requires a non-empty file path", flag));
    }

    if target.is_some() {
        return Err(anyhow!(
            "`--config`/`-c` flags can only be provided once per invocation"
        ));
    }

    *target = Some(candidate.to_string());
    Ok(())
}

#[cfg(test)]
mod cli_arg_tests {
    use super::parse_config_override_from_args;

    fn build_args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn returns_none_when_flag_missing() {
        let args = build_args(&["--foo", "bar"]);
        let parsed = parse_config_override_from_args(&args).unwrap();
        assert!(parsed.is_none());
    }

    #[test]
    fn parses_long_form_with_separate_value() {
        let args = build_args(&["--config", "/tmp/test.toml"]);
        let parsed = parse_config_override_from_args(&args).unwrap();
        assert_eq!(parsed, Some("/tmp/test.toml".to_string()));
    }

    #[test]
    fn parses_long_form_with_equals() {
        let args = build_args(&["--config=/opt/chorus/config.toml"]);
        let parsed = parse_config_override_from_args(&args).unwrap();
        assert_eq!(parsed, Some("/opt/chorus/config.toml".to_string()));
    }

    #[test]
    fn parses_short_flag_with_value() {
        let args = build_args(&["-c", "relative.toml"]);
        let parsed = parse_config_override_from_args(&args).unwrap();
        assert_eq!(parsed, Some("relative.toml".to_string()));
    }

    #[test]
    fn parses_short_flag_with_equals() {
        let args = build_args(&["-c=relative.toml"]);
        let parsed = parse_config_override_from_args(&args).unwrap();
        assert_eq!(parsed, Some("relative.toml".to_string()));
    }

    #[test]
    fn stops_parsing_after_double_dash() {
        let args = build_args(&["--", "--config", "ignored.toml"]);
        let parsed = parse_config_override_from_args(&args).unwrap();
        assert!(parsed.is_none());
    }

    #[test]
    fn errors_when_flag_missing_value() {
        let args = build_args(&["--config"]);
        assert!(parse_config_override_from_args(&args).is_err());
    }

    #[test]
    fn errors_when_flag_repeated() {
        let args = build_args(&["--config", "a.toml", "-c", "b.toml"]);
        assert!(parse_config_override_from_args(&args).is_err());
    }
}
