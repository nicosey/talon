# Talon

![Talon logo](logo.svg)

A lightweight cron runner written in Rust. Runs shell commands or LLM agent calls on a schedule, sends results to Telegram, stores run history locally, and exposes a web dashboard with a built-in chat agent for monitoring and Q&A.

Each job is either a shell command or a direct LLM call — with a pluggable backend supporting Anthropic (Claude), OpenAI, Ollama, LM Studio, and any other OpenAI-compatible endpoint.

---

## How it works

1. On startup, Talon runs all configured jobs immediately as a smoke test
2. It polls every minute and fires any job whose cron schedule falls within the current window
3. Each job runs either a shell command or calls an LLM backend directly
4. Output is sent to your Telegram chat on both success and failure
5. Each run is appended to `history.jsonl` for persistent local storage
6. A live web dashboard at `http://localhost:3030` shows job status, last output, and next run time
7. A **Chat** tab lets you talk to a built-in agent that knows your current job state

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

| Field                  | Required | Description                                                              |
|------------------------|----------|--------------------------------------------------------------------------|
| `telegram_token`       | yes      | Bot token from @BotFather                                                |
| `telegram_chat_id`     | yes      | Chat or user ID to receive notifications                                 |
| `timezone`             | no       | IANA timezone name, e.g. `Europe/London`                                 |
| `log_level`            | no       | Log level: `info`, `debug`, or `warn`                                    |
| `web_port`             | no       | Port for the web dashboard (default `3030`)                              |
| `anthropic.api_key`    | no       | Anthropic API key (required for Claude agent jobs)                       |
| `openai.url`           | no       | Base URL for OpenAI-compatible API                                       |
| `openai.api_key`       | no       | API key (leave blank for local models)                                   |
| `store_path`           | no       | Run history file (default `history.jsonl`), set to `""` to disable       |
| `jobs[].name`          | yes      | Label shown in logs, Telegram, and dashboard                             |
| `jobs[].schedule`      | yes      | Cron expression (7-field, see above)                                     |
| `jobs[].command`       | —        | Shell command (use this or `agent`, not both)                            |
| `jobs[].agent.backend` | —        | `anthropic`, `openai`, `ollama`, or `lmstudio`                           |
| `jobs[].agent.model`   | —        | Model name, e.g. `claude-haiku-4-5`, `qwen3:30b`                         |
| `jobs[].agent.prompt`  | —        | User prompt sent to the model                                            |
| `jobs[].agent.system`  | —        | Optional system prompt                                                   |

### 3. Run

```sh
cargo run
# or for production:
./target/release/talon
```

Open `http://localhost:3030` to see the dashboard. The JSON feed is available at `http://localhost:3030/api/jobs`.

---

## Chat agent

Talon includes a built-in chat agent accessible from the **Chat** tab in the web dashboard. It starts automatically with Talon — no separate process, no extra setup beyond adding `[chat]` to your config.

The agent is automatically given context about your jobs on every message:

```text
Current job status:
• Robotics Briefing (0 0 7 * * * *): last run succeeded ✅, 2026-03-20 07:00 UTC
• Morning Summary (0 30 7 * * * *): last run failed ❌, 2026-03-20 07:30 UTC — output: Error: connection refused
```

This means you can ask it things like:

- *"Why did Morning Summary fail?"*
- *"What did the Robotics Briefing output last time?"*
- *"Write a cron expression for every weekday at 9am"*
- *"How do I add an Ollama job?"*

### Configure in `config.toml`

```toml
[chat]
backend = "ollama"                        # or "anthropic", "openai", "lmstudio"
model   = "qwen3:8b"                      # any model your backend supports
system  = "You are Talon's assistant."   # optional — overrides the default system prompt
```

The `[chat]` block is optional. If omitted, the Chat tab shows a config hint instead of the input UI.

**Config fields:**

| Field          | Required | Description                                               |
|----------------|----------|-----------------------------------------------------------|
| `chat.backend` | yes      | `anthropic`, `openai`, `ollama`, or `lmstudio`            |
| `chat.model`   | yes      | Model name, e.g. `qwen3:8b`, `claude-haiku-4-5`           |
| `chat.system`  | no       | Custom system prompt — job context is always appended     |

### Details

- Uses the same pluggable `Backend` trait as scheduled agent jobs
- Each message sends the **full conversation history** to the model — multi-turn by default
- Job status (schedule, last run time, last output) is injected into the system prompt automatically on every turn
- Conversation history is kept in memory for the session and cleared on restart
- `GET /api/chat` returns the current history as JSON

### Test without a real backend

```sh
cargo run -- --mock
```

In mock mode, a built-in echo backend responds to every message so you can see the full chat UI working without a running model.

---

## Using Ollama (local models)

Ollama exposes an OpenAI-compatible API, so no API key is needed and nothing leaves your machine.

### 1. Install Ollama and pull a model

```sh
brew install ollama
ollama serve          # starts the local server on :11434
ollama pull qwen3:8b  # or llama3, mistral, phi3, etc.
```

### 2. Configure `config.toml`

```toml
[openai]
url     = "http://localhost:11434/v1"
api_key = ""   # leave blank — not required for local models

[[jobs]]
name     = "Daily Digest"
schedule = "0 0 8 * * * *"   # every day at 08:00

[jobs.agent]
backend = "ollama"
model   = "qwen3:8b"
prompt  = "Give me a 3-bullet summary of the most important tech news today."
system  = "Be concise. Plain text only. No markdown."
```

### 3. Test it immediately

```sh
cargo run -- --mock   # check the dashboard at http://localhost:3030
cargo run             # real run — fires all jobs once on startup
```

Talon calls `POST http://localhost:11434/v1/chat/completions`, parses `choices[0].message.content`, and sends the result to Telegram exactly as it would for any other job.

**Supported local model backends:**

| Backend name  | Default URL                       | Auth needed |
|---------------|-----------------------------------|-------------|
| `ollama`      | `http://localhost:11434/v1`       | No          |
| `lmstudio`    | `http://localhost:1234/v1`        | No          |
| `openai`      | `https://api.openai.com/v1`       | Yes         |

All three use the same `[openai]` config block — just change `url`.

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
