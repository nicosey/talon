use anyhow::Result;
use axum::{Router, extract::State, response::{Html, Json}, routing::get};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Serialize)]
pub struct JobStatus {
    pub name: String,
    pub schedule: String,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
    pub success: Option<bool>,
    pub output: Option<String>,
}

pub type SharedState = Arc<RwLock<Vec<JobStatus>>>;

pub fn new_state() -> SharedState {
    Arc::new(RwLock::new(Vec::new()))
}

async fn api_jobs(State(state): State<SharedState>) -> Json<Vec<JobStatus>> {
    Json(state.read().await.clone())
}

async fn dashboard(State(state): State<SharedState>) -> Html<String> {
    let jobs = state.read().await.clone();

    let rows: String = jobs.iter().map(|j| {
        let last_run = j.last_run
            .map(|t| t.format("%Y-%m-%d %H:%M UTC").to_string())
            .unwrap_or_else(|| "Never".to_string());
        let next_run = j.next_run
            .map(|t| t.format("%Y-%m-%d %H:%M UTC").to_string())
            .unwrap_or_else(|| "—".to_string());
        let (badge, badge_class) = match j.success {
            None    => ("Pending", "pending"),
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
        </tr>"#,
            j.name, j.schedule, last_run, next_run, badge_class, badge, preview)
    }).collect();

    Html(format!(r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Talon</title>
  <style>
    *, *::before, *::after {{ box-sizing: border-box; }}
    body {{ margin: 0; font-family: system-ui, sans-serif; background: #0d1117; color: #c9d1d9; }}
    header {{ padding: 1.2rem 2rem; border-bottom: 1px solid #21262d; display: flex; align-items: center; gap: 0.6rem; }}
    header h1 {{ margin: 0; font-size: 1.2rem; color: #f0f6fc; }}
    .subtitle {{ color: #8b949e; font-size: 0.85rem; margin-left: auto; }}
    main {{ padding: 2rem; }}
    table {{ width: 100%; border-collapse: collapse; }}
    th {{ text-align: left; padding: 0.5rem 0.75rem; font-size: 0.75rem; text-transform: uppercase;
          letter-spacing: 0.05em; color: #8b949e; border-bottom: 1px solid #21262d; }}
    td {{ padding: 0.65rem 0.75rem; border-bottom: 1px solid #161b22; vertical-align: top; }}
    td.name {{ font-weight: 600; color: #f0f6fc; white-space: nowrap; }}
    code {{ background: #161b22; padding: 0.15em 0.4em; border-radius: 4px; font-size: 0.8rem; white-space: nowrap; }}
    pre.output {{ margin: 0; font-size: 0.78rem; color: #8b949e; white-space: pre-wrap; word-break: break-all;
                  max-height: 6rem; overflow-y: auto; }}
    .badge {{ display: inline-block; padding: 0.2em 0.55em; border-radius: 999px; font-size: 0.75rem; font-weight: 600; }}
    .badge.ok      {{ background: #1a4731; color: #3fb950; }}
    .badge.fail    {{ background: #4a1e1e; color: #f85149; }}
    .badge.pending {{ background: #21262d; color: #8b949e; }}
    #last-refresh {{ color: #8b949e; font-size: 0.78rem; }}
    .logo {{ display: flex; align-items: center; gap: 0.5rem; }}
    .logo svg {{ flex-shrink: 0; }}
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
  <main>
    <table>
      <thead>
        <tr>
          <th>Job</th><th>Schedule</th><th>Last Run</th><th>Next Run</th><th>Status</th><th>Output</th>
        </tr>
      </thead>
      <tbody id="tbody">{}</tbody>
    </table>
  </main>
  <script>
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
  </script>
</body>
</html>"##, rows))
}

pub async fn start(state: SharedState, port: u16) -> Result<()> {
    let app = Router::new()
        .route("/", get(dashboard))
        .route("/api/jobs", get(api_jobs))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    tracing::info!("Web UI listening on http://localhost:{}", port);
    axum::serve(listener, app).await?;
    Ok(())
}
