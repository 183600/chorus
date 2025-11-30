mod config;
mod error;
mod llm;
mod server;
mod workflow;

use crate::config::Config;
use crate::llm::LLMClient;
use crate::server::{create_router, SharedState, AppState};
use crate::workflow::WorkflowEngine;
use clap::Parser;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{error, info, level_filters::LevelFilter};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy()
        .add_directive("chorus=debug".parse()?)
        .add_directive("axum::rejection=trace".parse()?);

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(env_filter)
        .init();

    info!("Chorus LLM API Aggregation Service starting");

    // Load configuration
    let config = Arc::new(Config::load()?);
    
    let listener_addr = format!("{}:{}", config.server.host, config.server.port);
    info!("Binding to {}", listener_addr);

    // Initialize components
    let llm_client = Arc::new(LLMClient::new()?);
    let workflow_engine = Arc::new(WorkflowEngine::new(config.clone(), llm_client.clone())?);

    // Create application state
    let state: SharedState = Arc::new(AppState {
        config: config.clone(),
        llm_client: llm_client.clone(),
        workflow_engine: workflow_engine.clone(),
    });

    // Create router
    let app = create_router(state);

    // Start server
    let listener = TcpListener::bind(&listener_addr).await?;
    info!("Server listening on http://{}", listener_addr);

    axum::serve(listener, app)
        .await
        .map_err(|e| {
            error!("Server error: {}", e);
            e.into()
        })
}
