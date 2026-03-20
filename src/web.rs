use anyhow::Result;
use axum::{Router, extract::State, response::{Html, Json}, routing::{get, post}};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::{agent, config::Config};

// ── Job state ─────────────────────────────────────────────────────────────────

#[derive(Clone, Serialize)]
pub struct JobStatus {
    pub name: String,
    pub schedule: String,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
    pub success: Option<bool>,
    pub output: Option<String>,
}

// ── App state ─────────────────────────────────────────────────────────────────

pub struct AppState {
    pub jobs: Vec<JobStatus>,
    pub chat: Vec<agent::ChatMessage>,
}

pub type SharedState = Arc<RwLock<AppState>>;

pub fn new_state() -> SharedState {
    Arc::new(RwLock::new(AppState { jobs: Vec::new(), chat: Vec::new() }))
}

pub async fn seed_mock_state(state: &SharedState) {
    let mut s = state.write().await;
    let now = Utc::now();
    s.jobs.push(JobStatus {
        name: "Robotics Briefing".to_string(),
        schedule: "0 0 7 * * * *".to_string(),
        last_run: Some(now - chrono::Duration::hours(2)),
        next_run: Some(now + chrono::Duration::hours(22)),
        success: Some(true),
        output: Some("• Boston Dynamics releases Atlas v4\n• NASA Perseverance finds organic compounds\n• EU proposes new robotics safety framework".to_string()),
    });
    s.jobs.push(JobStatus {
        name: "Morning Summary".to_string(),
        schedule: "0 30 7 * * * *".to_string(),
        last_run: Some(now - chrono::Duration::hours(1)),
        next_run: Some(now + chrono::Duration::hours(23)),
        success: Some(false),
        output: Some("Error: connection refused at http://localhost:11434 — is Ollama running?".to_string()),
    });
    s.jobs.push(JobStatus {
        name: "Weekly Report".to_string(),
        schedule: "0 0 9 * * Mon *".to_string(),
        last_run: None,
        next_run: Some(now + chrono::Duration::days(3)),
        success: None,
        output: None,
    });
    s.chat.push(agent::ChatMessage {
        role: "assistant".to_string(),
        content: "Hi! I'm Talon's assistant. Ask me about your jobs, schedules, or anything else.".to_string(),
    });
}

// ── Router state ──────────────────────────────────────────────────────────────

#[derive(Clone)]
struct RouterState {
    shared: SharedState,
    config: Arc<Config>,
}

// ── API handlers ──────────────────────────────────────────────────────────────

async fn api_jobs(State(rs): State<RouterState>) -> Json<Vec<JobStatus>> {
    Json(rs.shared.read().await.jobs.clone())
}

async fn api_chat(State(rs): State<RouterState>) -> Json<Vec<agent::ChatMessage>> {
    Json(rs.shared.read().await.chat.clone())
}

#[derive(Deserialize)]
struct ChatRequest {
    message: String,
}

#[derive(Serialize)]
struct ChatResponse {
    response: String,
}

async fn handle_chat(
    State(rs): State<RouterState>,
    Json(body): Json<ChatRequest>,
) -> Json<ChatResponse> {
    let response = match do_chat(&rs, body.message).await {
        Ok(r) => r,
        Err(e) => format!("Error: {}", e),
    };
    Json(ChatResponse { response })
}

async fn do_chat(rs: &RouterState, message: String) -> anyhow::Result<String> {
    let chat_cfg = rs.config.chat.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Chat not configured — add [chat] to config.toml"))?;

    let backend = agent::build_backend(&chat_cfg.backend, &chat_cfg.model, &rs.config)?;

    let job_context = {
        let state = rs.shared.read().await;
        build_job_context(&state.jobs)
    };
    let system = build_system_prompt(chat_cfg.system.as_deref(), &job_context);

    {
        let mut state = rs.shared.write().await;
        state.chat.push(agent::ChatMessage { role: "user".to_string(), content: message });
    }

    let messages = rs.shared.read().await.chat.clone();
    let response = backend.chat(Some(&system), &messages).await?;

    {
        let mut state = rs.shared.write().await;
        state.chat.push(agent::ChatMessage { role: "assistant".to_string(), content: response.clone() });
    }

    Ok(response)
}

fn build_job_context(jobs: &[JobStatus]) -> String {
    if jobs.is_empty() {
        return "No jobs configured.".to_string();
    }
    jobs.iter().map(|j| {
        let status = match j.success {
            None          => "not yet run".to_string(),
            Some(true)    => "last run succeeded ✅".to_string(),
            Some(false)   => "last run failed ❌".to_string(),
        };
        let last = j.last_run
            .map(|t| format!(", {}", t.format("%Y-%m-%d %H:%M UTC")))
            .unwrap_or_default();
        let output_hint = j.output.as_deref()
            .map(|o| format!(" — output: {}", o.lines().next().unwrap_or("").chars().take(80).collect::<String>()))
            .unwrap_or_default();
        format!("• {} ({}): {}{}{}", j.name, j.schedule, status, last, output_hint)
    }).collect::<Vec<_>>().join("\n")
}

fn build_system_prompt(custom: Option<&str>, job_context: &str) -> String {
    let base = custom.unwrap_or(
        "You are Talon's built-in assistant. Talon is a lightweight cron runner that \
         executes shell commands and LLM agent calls on a schedule and reports results to Telegram."
    );
    format!(
        "{}\n\nCurrent job status:\n{}\n\n\
         Help the user understand job outputs, debug failures, write cron expressions, \
         or answer general questions about Talon.",
        base, job_context
    )
}

// ── Dashboard ─────────────────────────────────────────────────────────────────

async fn dashboard(State(rs): State<RouterState>) -> Html<String> {
    let jobs = rs.shared.read().await.jobs.clone();
    let chat_enabled = rs.config.chat.is_some();

    let rows: String = jobs.iter().map(|j| {
        let last_run = j.last_run
            .map(|t| t.format("%Y-%m-%d %H:%M UTC").to_string())
            .unwrap_or_else(|| "Never".to_string());
        let next_run = j.next_run
            .map(|t| t.format("%Y-%m-%d %H:%M UTC").to_string())
            .unwrap_or_else(|| "—".to_string());
        let (badge, badge_class) = match j.success {
            None        => ("Pending", "pending"),
            Some(true)  => ("OK", "ok"),
            Some(false) => ("FAIL", "fail"),
        };
        let output = j.output.as_deref().unwrap_or("").replace('<', "&lt;").replace('>', "&gt;");
        let preview = if output.len() > 600 { &output[..600] } else { &output };
        format!(r#"<tr>
          <td class="name">{}</td>
          <td><code>{}</code></td>
          <td>{}</td>
          <td>{}</td>
          <td><span class="badge {}">{}</span></td>
          <td><pre class="output">{}</pre></td>
        </tr>"#, j.name, j.schedule, last_run, next_run, badge_class, badge, preview)
    }).collect();

    let chat_tab_content = if chat_enabled {
        r#"<div id="messages"></div>
        <div class="chat-row">
          <input id="chat-input" placeholder="Ask anything…" onkeydown="if(event.key==='Enter')sendMsg()">
          <button onclick="sendMsg()">Send</button>
        </div>"#.to_string()
    } else {
        r#"<p class="hint">Chat agent not configured. Add <code>[chat]</code> to <code>config.toml</code>:</p>
        <pre class="hint-pre">[chat]
backend = "ollama"   # or "anthropic", "openai", "lmstudio"
model   = "qwen3:8b"
system  = "You are Talon's assistant."  # optional</pre>"#.to_string()
    };

    let chat_enabled_js = if chat_enabled { "true" } else { "false" };

    Html(format!(r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Talon</title>
  <style>
    *, *::before, *::after {{ box-sizing: border-box; }}
    body {{ margin: 0; font-family: system-ui, sans-serif; background: #0d1117; color: #c9d1d9;
            height: 100vh; display: flex; flex-direction: column; overflow: hidden; }}
    header {{ padding: 1.2rem 2rem; border-bottom: 1px solid #21262d; display: flex; align-items: center; gap: 0.6rem; flex-shrink: 0; }}
    header h1 {{ margin: 0; font-size: 1.5rem; color: #f0f6fc; }}
    .subtitle {{ color: #8b949e; font-size: 1rem; margin-left: auto; }}
    .logo {{ display: flex; align-items: center; gap: 0.5rem; }}
    .logo svg {{ flex-shrink: 0; }}
    /* tabs */
    .tabs {{ display: flex; border-bottom: 1px solid #21262d; padding: 0 2rem; flex-shrink: 0; }}
    .tab {{ background: none; border: none; border-bottom: 2px solid transparent; padding: 0.75rem 1.25rem;
             color: #8b949e; cursor: pointer; font-size: 1rem; margin-bottom: -1px; }}
    .tab.active {{ color: #3fb950; border-bottom-color: #3fb950; }}
    /* jobs tab */
    #jobs-tab {{ padding: 2rem; flex: 1; overflow-y: auto; }}
    table {{ width: 100%; border-collapse: collapse; font-size: 1rem; }}
    th {{ text-align: left; padding: 0.6rem 0.75rem; font-size: 0.85rem; text-transform: uppercase;
          letter-spacing: 0.05em; color: #8b949e; border-bottom: 1px solid #21262d; }}
    td {{ padding: 0.75rem 0.75rem; border-bottom: 1px solid #161b22; vertical-align: top; }}
    td.name {{ font-weight: 600; color: #f0f6fc; white-space: nowrap; }}
    code {{ background: #161b22; padding: 0.15em 0.4em; border-radius: 4px; font-size: 0.9rem; white-space: nowrap; }}
    pre.output {{ margin: 0; font-size: 0.9rem; color: #8b949e; white-space: pre-wrap; word-break: break-all;
                  max-height: 6rem; overflow-y: auto; }}
    .badge {{ display: inline-block; padding: 0.25em 0.65em; border-radius: 999px; font-size: 0.85rem; font-weight: 600; }}
    .badge.ok      {{ background: #1a4731; color: #3fb950; }}
    .badge.fail    {{ background: #4a1e1e; color: #f85149; }}
    .badge.pending {{ background: #21262d; color: #8b949e; }}
    #last-refresh {{ color: #8b949e; font-size: 0.9rem; }}
    /* chat tab */
    #chat-tab {{ display: none; flex-direction: column; flex: 1; min-height: 0; padding: 1.5rem 2rem; gap: 1rem; }}
    #messages {{ flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 0.75rem; padding-bottom: 0.5rem; }}
    .msg {{ max-width: 72%; padding: 0.75rem 1rem; border-radius: 12px; line-height: 1.6; font-size: 1rem; white-space: pre-wrap; word-break: break-word; }}
    .msg.user      {{ align-self: flex-end; background: #1a4731; color: #e6f4ea; border-radius: 12px 12px 2px 12px; }}
    .msg.assistant {{ align-self: flex-start; background: #161b22; color: #c9d1d9; border: 1px solid #21262d; border-radius: 12px 12px 12px 2px; }}
    .msg.thinking  {{ align-self: flex-start; color: #8b949e; font-style: italic; }}
    .chat-row {{ display: flex; gap: 0.5rem; flex-shrink: 0; }}
    .chat-row input {{ flex: 1; background: #161b22; border: 1px solid #30363d; color: #c9d1d9;
                       padding: 0.75rem 1rem; border-radius: 8px; font-size: 1rem; outline: none; }}
    .chat-row input:focus {{ border-color: #3fb950; }}
    .chat-row button {{ background: #238636; border: none; color: white; padding: 0.75rem 1.5rem;
                        border-radius: 8px; cursor: pointer; font-size: 1rem; font-weight: 600; }}
    .chat-row button:disabled {{ background: #21262d; color: #8b949e; cursor: not-allowed; }}
    .hint {{ color: #8b949e; margin: 2rem 0 1rem; }}
    .hint-pre {{ background: #161b22; border: 1px solid #21262d; padding: 1rem; border-radius: 8px;
                 font-size: 0.9rem; color: #c9d1d9; }}
  </style>
</head>
<body>
  <header>
    <div class="logo">
      <svg width="30" height="30" viewBox="0 0 120 120" xmlns="http://www.w3.org/2000/svg">
        <g fill="#3fb950">
          <path d="M53 10 C53 8 67 8 67 10 L65 28 C65 31 62 33 60 33 C58 33 55 31 55 28 Z"/>
          <ellipse cx="60" cy="40" rx="27" ry="13"/>
          <path d="M40 46 C34 54 24 67 16 83 C13 90 14 98 20 100 C25 102 30 98 32 92 L35 82 C39 70 45 58 50 47 C48 43 43 43 40 46 Z"/>
          <path d="M18 99 C13 105 14 113 20 114 C26 115 31 109 28 102 C24 104 19 103 18 99 Z"/>
          <path d="M54 51 C52 65 50 79 50 91 C50 98 54 103 59 104 C64 105 68 100 68 93 C68 81 66 67 64 53 C62 48 56 47 54 51 Z"/>
          <path d="M50 93 C46 100 48 109 54 110 C60 111 64 104 61 97 C57 100 52 98 50 93 Z"/>
          <path d="M80 46 C77 43 72 43 70 47 C75 58 81 70 85 82 L88 92 C90 98 95 102 100 100 C106 98 107 90 104 83 C96 67 86 54 80 46 Z"/>
          <path d="M102 99 C101 103 96 104 92 102 C89 109 94 115 100 114 C106 113 107 105 102 99 Z"/>
        </g>
      </svg>
      <h1>Talon</h1>
    </div>
    <span class="subtitle" id="last-refresh">Loading…</span>
  </header>

  <nav class="tabs">
    <button class="tab active" onclick="showTab('jobs')">Jobs</button>
    <button class="tab" onclick="showTab('chat')">Chat</button>
  </nav>

  <div id="jobs-tab">
    <table>
      <thead>
        <tr>
          <th>Job</th><th>Schedule</th><th>Last Run</th><th>Next Run</th><th>Status</th><th>Output</th>
        </tr>
      </thead>
      <tbody id="tbody">{rows}</tbody>
    </table>
  </div>

  <div id="chat-tab">
    {chat_tab}
  </div>

  <script>
    const CHAT_ENABLED = {chat_enabled};

    // ── Tabs ────────────────────────────────────────────────────────────────
    function showTab(name) {{
      const jobs = document.getElementById('jobs-tab');
      const chat = document.getElementById('chat-tab');
      document.querySelectorAll('.tab').forEach((t, i) => {{
        t.classList.toggle('active', (i === 0) === (name === 'jobs'));
      }});
      if (name === 'jobs') {{
        jobs.style.display = '';
        chat.style.display = 'none';
      }} else {{
        jobs.style.display = 'none';
        chat.style.display = 'flex';
        if (CHAT_ENABLED) document.getElementById('chat-input').focus();
      }}
    }}

    // ── Jobs refresh ────────────────────────────────────────────────────────
    function fmt(iso) {{
      if (!iso) return '—';
      return new Date(iso).toISOString().replace('T', ' ').slice(0, 16) + ' UTC';
    }}
    async function refresh() {{
      try {{
        const jobs = await fetch('/api/jobs').then(r => r.json());
        document.getElementById('tbody').innerHTML = jobs.map(j => `
          <tr>
            <td class="name">${{j.name}}</td>
            <td><code>${{j.schedule}}</code></td>
            <td>${{fmt(j.last_run)}}</td>
            <td>${{fmt(j.next_run)}}</td>
            <td><span class="badge ${{j.success === null ? 'pending' : j.success ? 'ok' : 'fail'}}">
              ${{j.success === null ? 'Pending' : j.success ? 'OK' : 'FAIL'}}
            </span></td>
            <td><pre class="output">${{(j.output || '').slice(0, 600)}}</pre></td>
          </tr>`).join('');
        document.getElementById('last-refresh').textContent =
          'Refreshed ' + new Date().toISOString().replace('T', ' ').slice(0, 19) + ' UTC';
      }} catch(e) {{ console.error(e); }}
    }}
    setInterval(refresh, 15000);
    refresh();

    // ── Chat ────────────────────────────────────────────────────────────────
    function appendMsg(role, content) {{
      const el = document.createElement('div');
      el.className = 'msg ' + role;
      el.textContent = content;
      document.getElementById('messages').appendChild(el);
      el.scrollIntoView({{ behavior: 'smooth' }});
      return el;
    }}

    async function sendMsg() {{
      if (!CHAT_ENABLED) return;
      const input = document.getElementById('chat-input');
      const btn   = document.querySelector('.chat-row button');
      const msg   = input.value.trim();
      if (!msg) return;
      input.value = '';
      input.disabled = true;
      btn.disabled = true;
      appendMsg('user', msg);
      const thinking = appendMsg('thinking', 'Thinking…');
      try {{
        const resp = await fetch('/chat', {{
          method: 'POST',
          headers: {{ 'Content-Type': 'application/json' }},
          body: JSON.stringify({{ message: msg }})
        }});
        const data = await resp.json();
        thinking.remove();
        appendMsg('assistant', data.response);
      }} catch(e) {{
        thinking.remove();
        appendMsg('assistant', 'Error: ' + e.message);
      }} finally {{
        input.disabled = false;
        btn.disabled = false;
        input.focus();
      }}
    }}

    async function loadHistory() {{
      if (!CHAT_ENABLED) return;
      try {{
        const msgs = await fetch('/api/chat').then(r => r.json());
        msgs.forEach(m => appendMsg(m.role, m.content));
      }} catch(e) {{}}
    }}
    loadHistory();
  </script>
</body>
</html>"##,
        rows = rows,
        chat_tab = chat_tab_content,
        chat_enabled = chat_enabled_js,
    ))
}

// ── Server ────────────────────────────────────────────────────────────────────

pub async fn start(state: SharedState, config: Arc<Config>) -> Result<()> {
    let port = config.web_port;
    let rs = RouterState { shared: state, config };

    let app = Router::new()
        .route("/", get(dashboard))
        .route("/api/jobs", get(api_jobs))
        .route("/api/chat", get(api_chat))
        .route("/chat", post(handle_chat))
        .with_state(rs);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    tracing::info!("Web UI listening on http://localhost:{}", port);
    axum::serve(listener, app).await?;
    Ok(())
}
