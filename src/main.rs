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
    let args: Vec<String> = std::env::args().collect();
    let mock = args.contains(&"--mock".to_string());

    let config = config::load()?;

    tracing_subscriber::fmt()
        .with_env_filter(&config.log_level)
        .init();

    // talon run <job name>  — run a single job by name and exit
    if args.len() >= 3 && args[1] == "run" {
        let target = args[2..].join(" ");
        return run_once(&target, &config).await;
    }

    let state = web::new_state();
    let web_config = Arc::new(config.clone());

    if mock {
        info!("🦅 Talon started in mock mode — web UI only, no jobs will run");
        web::seed_mock_state(&state).await;
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

async fn run_once(target: &str, config: &config::Config) -> Result<()> {
    let job = config.jobs.iter()
        .find(|j| j.name.to_lowercase() == target.to_lowercase())
        .ok_or_else(|| {
            let names = config.jobs.iter()
                .map(|j| format!("  • {}", j.name))
                .collect::<Vec<_>>()
                .join("\n");
            anyhow::anyhow!(
                "No job named '{}'. Available jobs:\n{}",
                target, names
            )
        })?;

    println!("▶  Running '{}'...\n", job.name);

    let result = if let Some(command) = &job.command {
        executor::run_command(command)
    } else if let Some(agent_cfg) = &job.agent {
        let backend = agent::build_backend(&agent_cfg.backend, &agent_cfg.model, config)?;
        agent::run(agent_cfg, backend.as_ref()).await
    } else {
        anyhow::bail!("Job '{}' has neither command nor agent", job.name)
    };

    match result {
        Ok(output) => {
            println!("{}", output.trim());
            Ok(())
        }
        Err(e) => {
            eprintln!("❌  {}", e);
            std::process::exit(1);
        }
    }
}
