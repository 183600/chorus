mod config;
mod llm;
mod server;
mod workflow;

#[cfg(test)]
mod config_tests;

use anyhow::Result;
use std::sync::Arc;
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

    // 自动加载配置（env > ~/.config/chorus/config.toml）
    let config = config::Config::load_auto()?;
    let host = config.server.host.clone();
    let port = config.server.port;

    tracing::info!("Starting Chorus server on {}:{}", host, port);
    tracing::info!("Analyzer model: {}", config.workflow_integration.analyzer_model);
    tracing::info!("Worker models: {:?}", config.workflow_integration.worker_models);
    tracing::info!("Synthesizer model: {}", config.workflow_integration.synthesizer_model);

    // 启动服务器
    server::start_server(Arc::new(config)).await?;

    Ok(())
}