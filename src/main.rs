use anyhow::Result;
use std::sync::Arc;
use tracing::info;

mod agent;
mod config;
mod executor;
mod scheduler;
mod telegram;
mod web;

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::load()?;

    tracing_subscriber::fmt()
        .with_env_filter(&config.log_level)
        .init();

    info!("🦅 Talon started");

    let state = web::new_state();
    let web_port = config.web_port;

    let (sched_result, web_result) = tokio::join!(
        scheduler::start(config, Arc::clone(&state)),
        web::start(state, web_port),
    );

    sched_result?;
    web_result?;
    Ok(())
}
