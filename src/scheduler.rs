use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use cron::Schedule;
use std::str::FromStr;
use std::time::Duration;
use tracing::{error, info};

use crate::{agent, config::{AgentConfig, Config}, executor, store, telegram, web};

struct ParsedJob {
    name: String,
    command: Option<String>,
    agent: Option<AgentConfig>,
    raw_schedule: String,
    schedule: Schedule,
}

fn due(schedule: &Schedule, now: DateTime<Utc>, one_minute_ago: DateTime<Utc>) -> bool {
    schedule
        .after(&one_minute_ago)
        .next()
        .map(|t| t <= now)
        .unwrap_or(false)
}

async fn execute(job: &ParsedJob, config: &Config) -> Result<String> {
    if let Some(command) = &job.command {
        executor::run_command(command)
    } else if let Some(agent_cfg) = &job.agent {
        let backend = agent::build_backend(&agent_cfg.backend, &agent_cfg.model, config)?;
        agent::run(agent_cfg, backend.as_ref()).await
    } else {
        unreachable!("config validation ensures command or agent is set")
    }
}

async fn update_state(
    state: &web::SharedState,
    name: &str,
    success: bool,
    output: String,
    schedule: &Schedule,
    tz: chrono_tz::Tz,
) {
    let mut guard = state.write().await;
    if let Some(entry) = guard.jobs.iter_mut().find(|s| s.name == name) {
        entry.last_run = Some(Utc::now());
        entry.next_run = schedule.upcoming(tz).next().map(|t| t.with_timezone(&Utc));
        entry.success = Some(success);
        entry.output = Some(output);
    }
}

pub async fn start(config: Config, state: web::SharedState) -> Result<()> {
    let tz: chrono_tz::Tz = config.timezone.parse()
        .map_err(|_| anyhow::anyhow!("Invalid timezone: {}", config.timezone))?;

    let jobs: Vec<ParsedJob> = config.jobs.iter()
        .map(|job| {
            let schedule = Schedule::from_str(&job.schedule)
                .with_context(|| format!("Invalid schedule for '{}': {}", job.name, job.schedule))?;
            Ok(ParsedJob {
                name: job.name.clone(),
                command: job.command.clone(),
                agent: job.agent.clone(),
                raw_schedule: job.schedule.clone(),
                schedule,
            })
        })
        .collect::<Result<_>>()?;

    // Populate initial state
    {
        let mut guard = state.write().await;
        for job in &jobs {
            guard.jobs.push(web::JobStatus {
                name: job.name.clone(),
                schedule: job.raw_schedule.clone(),
                last_run: None,
                next_run: job.schedule.upcoming(tz).next().map(|t| t.with_timezone(&Utc)),
                success: None,
                output: None,
            });
        }
    }

    info!("Scheduler running with {} job(s) in timezone {}", jobs.len(), config.timezone);
    for job in &jobs {
        let kind = if job.command.is_some() { "shell" } else { "agent" };
        if let Some(t) = job.schedule.upcoming(tz).next() {
            info!("  • {} [{}] — next run at {}", job.name, kind, t);
        }
    }

    // Startup test: run all jobs immediately
    for job in &jobs {
        info!("🧪 TEST MODE: Running '{}' now...", job.name);
        match execute(job, &config).await {
            Ok(output) => {
                let msg = format!("✅ <b>TEST: {}</b> completed\n\n{}", job.name, output.trim());
                telegram::send_message(&config.telegram_token, &config.telegram_chat_id, msg).await;
                store::append(&config.store_path, &job.name, true, output.trim());
                update_state(&state, &job.name, true, output.trim().to_string(), &job.schedule, tz).await;
            }
            Err(e) => {
                error!("Test run of '{}' failed: {}", job.name, e);
                let msg = format!("❌ <b>TEST: {}</b> failed\n\n{}", job.name, e);
                telegram::send_message(&config.telegram_token, &config.telegram_chat_id, msg).await;
                store::append(&config.store_path, &job.name, false, &e.to_string());
                update_state(&state, &job.name, false, e.to_string(), &job.schedule, tz).await;
            }
        }
    }

    let mut interval = tokio::time::interval(Duration::from_secs(60));

    loop {
        interval.tick().await;

        let now = Utc::now();
        let one_minute_ago = now - chrono::Duration::seconds(60);

        for job in &jobs {
            if due(&job.schedule, now, one_minute_ago) {
                info!("▶️ Running '{}'...", job.name);
                match execute(job, &config).await {
                    Ok(output) => {
                        let msg = format!("✅ <b>{}</b> completed\n\n{}", job.name, output.trim());
                        telegram::send_message(&config.telegram_token, &config.telegram_chat_id, msg).await;
                        store::append(&config.store_path, &job.name, true, output.trim());
                        update_state(&state, &job.name, true, output.trim().to_string(), &job.schedule, tz).await;
                    }
                    Err(e) => {
                        error!("'{}' failed: {}", job.name, e);
                        let msg = format!("❌ <b>{}</b> failed\n\n{}", job.name, e);
                        telegram::send_message(&config.telegram_token, &config.telegram_chat_id, msg).await;
                        store::append(&config.store_path, &job.name, false, &e.to_string());
                        update_state(&state, &job.name, false, e.to_string(), &job.schedule, tz).await;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, s).unwrap()
    }

    #[test]
    fn valid_schedule_parses() {
        assert!(Schedule::from_str("0 0 7 * * * *").is_ok());
    }

    #[test]
    fn invalid_schedule_fails() {
        assert!(Schedule::from_str("not a cron").is_err());
    }

    #[test]
    fn too_few_fields_fails() {
        assert!(Schedule::from_str("0 7 * * *").is_err());
    }

    #[test]
    fn valid_timezone_parses() {
        let tz: Result<chrono_tz::Tz, _> = "Europe/London".parse();
        assert!(tz.is_ok());
    }

    #[test]
    fn invalid_timezone_fails() {
        let tz: Result<chrono_tz::Tz, _> = "Not/Real".parse();
        assert!(tz.is_err());
    }

    #[test]
    fn fires_when_scheduled_time_is_in_window() {
        let schedule = Schedule::from_str("0 0 7 * * * *").unwrap();
        let now = utc(2026, 3, 20, 7, 0, 30);
        let one_minute_ago = now - chrono::Duration::seconds(60);
        assert!(due(&schedule, now, one_minute_ago));
    }

    #[test]
    fn does_not_fire_outside_window() {
        let schedule = Schedule::from_str("0 0 7 * * * *").unwrap();
        let now = utc(2026, 3, 20, 8, 0, 30);
        let one_minute_ago = now - chrono::Duration::seconds(60);
        assert!(!due(&schedule, now, one_minute_ago));
    }

    #[test]
    fn does_not_fire_just_before_window() {
        let schedule = Schedule::from_str("0 0 7 * * * *").unwrap();
        let now = utc(2026, 3, 20, 6, 59, 30);
        let one_minute_ago = now - chrono::Duration::seconds(60);
        assert!(!due(&schedule, now, one_minute_ago));
    }

    #[test]
    fn every_minute_schedule_always_fires() {
        let schedule = Schedule::from_str("0 * * * * * *").unwrap();
        let now = utc(2026, 3, 20, 12, 34, 30);
        let one_minute_ago = now - chrono::Duration::seconds(60);
        assert!(due(&schedule, now, one_minute_ago));
    }

    #[test]
    fn day_of_week_filter_respected() {
        let schedule = Schedule::from_str("0 0 9 * * Mon *").unwrap();
        let now = utc(2026, 3, 20, 9, 0, 30); // Friday
        let one_minute_ago = now - chrono::Duration::seconds(60);
        assert!(!due(&schedule, now, one_minute_ago));
    }

    #[test]
    fn day_of_week_fires_on_correct_day() {
        let schedule = Schedule::from_str("0 0 9 * * Mon *").unwrap();
        let now = utc(2026, 3, 23, 9, 0, 30); // Monday
        let one_minute_ago = now - chrono::Duration::seconds(60);
        assert!(due(&schedule, now, one_minute_ago));
    }
}
