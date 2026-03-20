# Talon

A lightweight cron runner written in Rust. Runs one or more shell commands on configurable cron schedules, sends results to Telegram, and exposes a web dashboard for monitoring.

Built as a simpler alternative to agent-based schedulers — no LLM overhead, no moving parts, just commands that fire on a schedule.

---

## How it works

1. On startup, Talon runs all configured jobs immediately as a smoke test
2. It polls every minute and fires any job whose cron schedule falls within the current window
3. stdout/stderr is captured and sent to your Telegram chat — both on success and failure
4. A live web dashboard at `http://localhost:3030` shows job status, last output, and next run time

---

## Prerequisites

- [Rust](https://rustup.rs)
- A Telegram bot token (create one via [@BotFather](https://t.me/BotFather))
- Your Telegram chat ID (send a message to your bot, then check `https://api.telegram.org/bot<TOKEN>/getUpdates`)

---

## Setup

### 1. Clone and build

```sh
git clone https://github.com/yourname/talon
cd talon
cargo build --release
```

### 2. Create `config.toml`

```toml
telegram_token   = "your_bot_token"
telegram_chat_id = "your_chat_id"
timezone         = "Europe/London"   # optional, defaults to UTC
log_level        = "info"            # optional, defaults to info
web_port         = 3030              # optional, defaults to 3030

[[jobs]]
name     = "Robotics Briefing"
command  = "cd ~/projects/briefing && python3 briefing.py"
schedule = "0 0 7 * * * *"          # daily at 07:00

[[jobs]]
name     = "Weekly Report"
command  = "cd ~/projects/reports && python3 report.py"
schedule = "0 0 9 * * Mon *"        # Mondays at 09:00
```

**Schedule format:** `sec min hour dom month dow year`

| Field | Values               | Examples       |
|-------|----------------------|----------------|
| sec   | 0–59                 | `0`            |
| min   | 0–59                 | `30`           |
| hour  | 0–23                 | `7`, `9`       |
| dom   | 1–31 or `*`          | `1`, `15`, `*` |
| month | 1–12 or name or `*`  | `*`, `Jan`     |
| dow   | 0–7 or name or `*`   | `Mon`, `*`     |
| year  | year or `*`          | `*`            |

**Config fields:**

| Field              | Required | Description                                 |
|--------------------|----------|---------------------------------------------|
| `telegram_token`   | yes      | Bot token from @BotFather                   |
| `telegram_chat_id` | yes      | Chat or user ID to receive notifications    |
| `timezone`         | no       | IANA timezone name, e.g. `Europe/London`    |
| `log_level`        | no       | Log level: `info`, `debug`, or `warn`       |
| `web_port`         | no       | Port for the web dashboard (default `3030`) |
| `jobs[].name`      | yes      | Label shown in logs, Telegram, dashboard    |
| `jobs[].command`   | yes      | Shell command to run                        |
| `jobs[].schedule`  | yes      | Cron expression (7-field, see above)        |

### 3. Run

```sh
cargo run
# or for production:
./target/release/talon
```

Open `http://localhost:3030` to see the dashboard. The JSON feed is available at `http://localhost:3030/api/jobs`.

---

## Deployment (Mac Mini)

```sh
cargo build --release
scp target/release/talon config.toml user@mac-mini:~/talon/
ssh user@mac-mini "cd ~/talon && nohup ./talon &> talon.log &"
```

For auto-restart on reboot, add a `launchd` plist or use `nohup` in your login items.

---

## Notes

- All jobs run once on startup regardless of their schedule — useful for verifying your setup before the first scheduled trigger.
- Multiple jobs run sequentially within the same minute tick, not in parallel.
- The dashboard auto-refreshes every 15 seconds without a full page reload.
