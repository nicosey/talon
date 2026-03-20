# Talon

![Talon logo](logo.svg)

A lightweight cron runner written in Rust. Runs shell commands or LLM agent calls on configurable cron schedules, sends results to Telegram, and exposes a web dashboard for monitoring.

Each job is either a shell command or a direct LLM call — with a pluggable backend supporting Anthropic (Claude), OpenAI, Ollama, LM Studio, and any other OpenAI-compatible endpoint.

---

## How it works

1. On startup, Talon runs all configured jobs immediately as a smoke test
2. It polls every minute and fires any job whose cron schedule falls within the current window
3. Each job runs either a shell command or calls an LLM backend directly
4. Output is sent to your Telegram chat on both success and failure
5. A live web dashboard at `http://localhost:3030` shows job status, last output, and next run time

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

# Required if any job uses backend = "anthropic"
[anthropic]
api_key = "sk-ant-..."

# Required if any job uses backend = "openai" or "ollama"
[openai]
url     = "http://localhost:11434/v1"  # Ollama, LM Studio, OpenAI, etc.
api_key = ""                           # leave blank for local models

# Shell job — runs a command
[[jobs]]
name     = "Robotics Briefing"
command  = "cd ~/projects/briefing && python3 briefing.py robotics"
schedule = "0 0 7 * * * *"            # daily at 07:00

# Agent job — calls an LLM directly
[[jobs]]
name     = "Morning Summary"
schedule = "0 30 7 * * * *"           # daily at 07:30
[jobs.agent]
backend  = "ollama"                    # or "anthropic", "openai", "lmstudio"
model    = "qwen3:30b"
prompt   = "Give me a 3-bullet summary of the most important tech news today."
system   = "Be concise. Plain text only."
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

| Field                    | Required | Description                                          |
|--------------------------|----------|------------------------------------------------------|
| `telegram_token`         | yes      | Bot token from @BotFather                            |
| `telegram_chat_id`       | yes      | Chat or user ID to receive notifications             |
| `timezone`               | no       | IANA timezone name, e.g. `Europe/London`             |
| `log_level`              | no       | Log level: `info`, `debug`, or `warn`                |
| `web_port`               | no       | Port for the web dashboard (default `3030`)          |
| `anthropic.api_key`      | no       | Anthropic API key (required for Claude agent jobs)   |
| `openai.url`             | no       | Base URL for OpenAI-compatible API                   |
| `openai.api_key`         | no       | API key (leave blank for local models)               |
| `jobs[].name`            | yes      | Label shown in logs, Telegram, and dashboard         |
| `jobs[].schedule`        | yes      | Cron expression (7-field, see above)                 |
| `jobs[].command`         | —        | Shell command (use this or `agent`, not both)        |
| `jobs[].agent.backend`   | —        | `anthropic`, `openai`, `ollama`, or `lmstudio`       |
| `jobs[].agent.model`     | —        | Model name, e.g. `claude-haiku-4-5`, `qwen3:30b`     |
| `jobs[].agent.prompt`    | —        | User prompt sent to the model                        |
| `jobs[].agent.system`    | —        | Optional system prompt                               |

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
