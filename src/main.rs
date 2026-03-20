use anyhow::Result;
use std::sync::Arc;
use tracing::info;

mod agent;
mod config;
mod executor;
mod scheduler;
mod store;
mod telegram;
mod web;

#[tokio::main]
async fn main() -> Result<()> {
    let mock = std::env::args().any(|a| a == "--mock");

    let config = config::load()?;

    tracing_subscriber::fmt()
        .with_env_filter(&config.log_level)
        .init();

    let state = web::new_state();
    let web_config = Arc::new(config.clone());

    if mock {
        info!("🦅 Talon started in mock mode — web UI only, no jobs will run");
        web::seed_mock_state(&state).await;
        // Enable the chat tab in mock mode even without a real [chat] config
        let mut mock_config = config.clone();
        if mock_config.chat.is_none() {
            mock_config.chat = Some(config::ChatConfig {
                backend: "mock".to_string(),
                model: "mock".to_string(),
                system: None,
            });
        }
        web::start(state, Arc::new(mock_config)).await?;
    } else {
        info!("🦅 Talon started");
        let (sched_result, web_result) = tokio::join!(
            scheduler::start(config, Arc::clone(&state)),
            web::start(state, web_config),
        );
        sched_result?;
        web_result?;
    }

    Ok(())
}
