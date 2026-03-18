use anyhow::Result;
use tracing::info;

mod config;
mod scheduler;
mod executor;
mod telegram;

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::load()?;

    tracing_subscriber::fmt()
        .with_env_filter(&config.log_level)
        .init();

    info!("🦅 Talon started - Robotics briefing scheduled for 07:00 Europe/London");

    scheduler::start(config).await
}
