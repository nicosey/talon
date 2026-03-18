# Talon 🦅

A lightweight, reliable cron runner written in Rust.

Built as a simpler and more dependable alternative to OpenClaw — especially for running scripts on a schedule **without** forcing an LLM or agent turn.

---

## Features

- Direct shell command execution (no LLM overhead)
- Simple TOML-based configuration
- Built-in Telegram notifications
- Unit tests with mocking support
- Lightweight and fast
- Easy to develop on laptop and deploy to Mac Mini

---

## Quick Start

### 1. Configure

Edit `config.toml`:

```toml
telegram_token = "your_bot_token_here"
telegram_chat_id = "435026465"
command = "cd /Users/m4server/projects/briefing && python3 briefing.py --config config/robotics.json"

### 2. Run

cargo run
