use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use cron::Schedule;
use std::str::FromStr;
use std::time::Duration;
use tracing::{error, info};

use crate::{config::Config, executor, telegram, web};

fn due(schedule: &Schedule, now: DateTime<Utc>, one_minute_ago: DateTime<Utc>) -> bool {
    schedule
        .after(&one_minute_ago)
        .next()
        .map(|t| t <= now)
        .unwrap_or(false)
}

async fn update_state(
    state: &web::SharedState,
    name: &str,
    success: bool,
    output: String,
    schedule: &Schedule,
    tz: chrono_tz::Tz,
) {
    let mut statuses = state.write().await;
    if let Some(entry) = statuses.iter_mut().find(|s| s.name == name) {
        entry.last_run = Some(Utc::now());
        entry.next_run = schedule.upcoming(tz).next().map(|t| t.with_timezone(&Utc));
        entry.success = Some(success);
        entry.output = Some(output);
    }
}

pub async fn start(config: Config, state: web::SharedState) -> Result<()> {
    let tz: chrono_tz::Tz = config.timezone.parse()
        .map_err(|_| anyhow::anyhow!("Invalid timezone: {}", config.timezone))?;

    // (name, command, raw_schedule, parsed_schedule)
    let jobs: Vec<(String, String, String, Schedule)> = config.jobs.iter()
        .map(|job| {
            let schedule = Schedule::from_str(&job.schedule)
                .with_context(|| format!("Invalid schedule for '{}': {}", job.name, job.schedule))?;
            Ok((job.name.clone(), job.command.clone(), job.schedule.clone(), schedule))
        })
        .collect::<Result<_>>()?;

    // Populate initial state
    {
        let mut statuses = state.write().await;
        for (name, _, raw_schedule, schedule) in &jobs {
            statuses.push(web::JobStatus {
                name: name.clone(),
                schedule: raw_schedule.clone(),
                last_run: None,
                next_run: schedule.upcoming(tz).next().map(|t| t.with_timezone(&Utc)),
                success: None,
                output: None,
            });
        }
    }

    info!("Scheduler running with {} job(s) in timezone {}", jobs.len(), config.timezone);
    for (name, _, _, schedule) in &jobs {
        if let Some(t) = schedule.upcoming(tz).next() {
            info!("  • {} — next run at {}", name, t);
        }
    }

    // Startup test: run all jobs immediately
    for (name, command, _, schedule) in &jobs {
        info!("🧪 TEST MODE: Running '{}' now...", name);
        match executor::run_command(command) {
            Ok(output) => {
                let msg = format!("✅ <b>TEST: {}</b> completed\n\n{}", name, output.trim());
                telegram::send_message(&config.telegram_token, &config.telegram_chat_id, msg).await;
                update_state(&state, name, true, output.trim().to_string(), schedule, tz).await;
            }
            Err(e) => {
                error!("Test run of '{}' failed: {}", name, e);
                let msg = format!("❌ <b>TEST: {}</b> failed\n\n{}", name, e);
                telegram::send_message(&config.telegram_token, &config.telegram_chat_id, msg).await;
                update_state(&state, name, false, e.to_string(), schedule, tz).await;
            }
        }
    }

    let mut interval = tokio::time::interval(Duration::from_secs(60));

    loop {
        interval.tick().await;

        let now = Utc::now();
        let one_minute_ago = now - chrono::Duration::seconds(60);

        for (name, command, _, schedule) in &jobs {
            if due(schedule, now, one_minute_ago) {
                info!("▶️ Running '{}'...", name);
                match executor::run_command(command) {
                    Ok(output) => {
                        let msg = format!("✅ <b>{}</b> completed\n\n{}", name, output.trim());
                        telegram::send_message(&config.telegram_token, &config.telegram_chat_id, msg).await;
                        update_state(&state, name, true, output.trim().to_string(), schedule, tz).await;
                    }
                    Err(e) => {
                        error!("'{}' failed: {}", name, e);
                        let msg = format!("❌ <b>{}</b> failed\n\n{}", name, e);
                        telegram::send_message(&config.telegram_token, &config.telegram_chat_id, msg).await;
                        update_state(&state, name, false, e.to_string(), schedule, tz).await;
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

    // ── cron parsing ──────────────────────────────────────────────────────────

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
        assert!(Schedule::from_str("0 7 * * *").is_err()); // 5-field not supported
    }

    // ── timezone parsing ──────────────────────────────────────────────────────

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

    // ── due() logic ───────────────────────────────────────────────────────────

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
        // 2026-03-20 is a Friday — Monday-only job should not fire
        let schedule = Schedule::from_str("0 0 9 * * Mon *").unwrap();
        let now = utc(2026, 3, 20, 9, 0, 30);
        let one_minute_ago = now - chrono::Duration::seconds(60);
        assert!(!due(&schedule, now, one_minute_ago));
    }

    #[test]
    fn day_of_week_fires_on_correct_day() {
        // 2026-03-23 is a Monday
        let schedule = Schedule::from_str("0 0 9 * * Mon *").unwrap();
        let now = utc(2026, 3, 23, 9, 0, 30);
        let one_minute_ago = now - chrono::Duration::seconds(60);
        assert!(due(&schedule, now, one_minute_ago));
    }
}
