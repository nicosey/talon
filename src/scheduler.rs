use anyhow::Result;
use chrono::{Timelike, Utc};
use std::time::Duration;
use tracing::{error, info};

use crate::{config::Config, executor, telegram};

pub async fn start(config: Config) -> Result<()> {
    info!("Scheduler running. Checking every minute for 07:00 Europe/London...");

    // === TEST TRIGGER - Runs immediately when you start Talon ===
    info!("🧪 TEST MODE: Running briefing command now...");
    match executor::run_command(&config.command) {
        Ok(output) => {
            let msg = format!("✅ <b>TEST Robotics Briefing</b> completed\n\n{}", output.trim());
            telegram::send_message(&config.telegram_token, &config.telegram_chat_id, msg).await;
        }
        Err(e) => {
            error!("Test briefing failed: {}", e);
            let msg = format!("❌ <b>TEST Robotics Briefing</b> failed\n\n{}", e);
            telegram::send_message(&config.telegram_token, &config.telegram_chat_id, msg).await;
        }
    }
    // ========================================================

    let mut interval = tokio::time::interval(Duration::from_secs(60));

    loop {
        interval.tick().await;

        let now = Utc::now();
        let london = now.with_timezone(&chrono_tz::Europe::London);

        if london.hour() == 7 && london.minute() == 0 {
            info!("▶️ Running daily robotics briefing...");

            match executor::run_command(&config.command) {
                Ok(output) => {
                    let msg = format!("✅ <b>Robotics Briefing</b> completed\n\n{}", output.trim());
                    telegram::send_message(&config.telegram_token, &config.telegram_chat_id, msg).await;
                }
                Err(e) => {
                    error!("Job failed: {}", e);
                    let msg = format!("❌ <b>Robotics Briefing</b> failed\n\n{}", e);
                    telegram::send_message(&config.telegram_token, &config.telegram_chat_id, msg).await;
                }
            }

            tokio::time::sleep(Duration::from_secs(70)).await;
        }
    }
}
