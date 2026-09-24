use crate::cli::{Cli, Format};
use crate::error::AppError;
use clap::ValueEnum;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Clone)]
struct AppState {
    jobs: Arc<Mutex<HashMap<u64, JobState>>>,
    next_job_id: Arc<AtomicU64>,
}

#[derive(Clone)]
struct JobState {
    pct: f64,
    message: String,
    done: bool,
    success: bool,
    detail: String,
}

impl JobState {
    fn pending(message: impl Into<String>) -> Self {
        Self {
            pct: 0.0,
            message: message.into(),
            done: false,
            success: false,
            detail: String::new(),
        }
    }
}

pub fn serve(host: &str, port: u16) -> Result<(), AppError> {
    let addr = format!("{host}:{port}");
    let listener = TcpListener::bind(&addr)?;
    let state = AppState {
        jobs: Arc::new(Mutex::new(HashMap::new())),
        next_job_id: Arc::new(AtomicU64::new(1)),
    };
    println!("UI running at http://{addr}");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = state.clone();
                thread::spawn(move || {
                    if let Err(e) = handle(stream, state) {
                        eprintln!("ui error: {e}");
                    }
                });
            }
            Err(e) => eprintln!("ui connection error: {e}"),
        }
    }

    Ok(())
}

fn handle(mut stream: TcpStream, state: AppState) -> Result<(), AppError> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;

    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line == "\r\n" || line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }

    let mut body = vec![0; content_length];
    std::io::Read::read_exact(&mut reader, &mut body)?;

    let request_path = request_line.split_whitespace().nth(1).unwrap_or("/");
    let (content_type, response) = if request_line.starts_with("POST /download ") {
        ("text/html; charset=utf-8", download_response(&body, state))
    } else if request_line.starts_with("GET /progress") {
        (
            "application/json; charset=utf-8",
            progress_response(request_path, state),
        )
    } else {
        ("text/html; charset=utf-8", page(None, None))
    };

    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.len(),
        response
    )?;
    Ok(())
}

fn download_response(body: &[u8], state: AppState) -> String {
    let form = parse_form(body);
    let url = form_value(&form, "url").trim().to_string();
    if url.is_empty() {
        return page(Some(ErrorStatus("Paste a URL first.".into())), None);
    }

    let bitrate = form_value(&form, "bitrate")
        .parse::<u32>()
        .unwrap_or(128)
        .clamp(32, 320);
    let format = Format::from_str(form_value(&form, "format"), true).unwrap_or(Format::Mp3);
    let output_dir = match form_value(&form, "output_dir").trim() {
        "" => None,
        value => Some(PathBuf::from(value)),
    };

    let cli = Cli {
        url: Some(url),
        output_dir,
        bitrate,
        format,
        quiet: true,
        ui: false,
        ui_host: "127.0.0.1".into(),
        ui_port: 8787,
    };

    let job_id = state.next_job_id.fetch_add(1, Ordering::SeqCst);
    state
        .jobs
        .lock()
        .unwrap()
        .insert(job_id, JobState::pending("Preparing download..."));

    let jobs = Arc::clone(&state.jobs);
    thread::spawn(move || run_download_job(job_id, cli, jobs));

    page(None, Some(job_id))
}

fn run_download_job(job_id: u64, cli: Cli, jobs: Arc<Mutex<HashMap<u64, JobState>>>) {
    let result = run_download_job_inner(job_id, &cli, &jobs);
    let mut jobs = jobs.lock().unwrap();
    let job = jobs
        .entry(job_id)
        .or_insert_with(|| JobState::pending("Finishing..."));

    match result {
        Ok(path) => {
            job.pct = 100.0;
            job.message = "Done".into();
            job.done = true;
            job.success = true;
            job.detail = format!("Saved: {}", path.display());
        }
        Err(e) => {
            job.done = true;
            job.success = false;
            job.detail = e.to_string();
            if job.message.is_empty() || job.message == "Preparing download..." {
                job.message = "Failed".into();
            }
        }
    }
}

fn run_download_job_inner(
    job_id: u64,
    cli: &Cli,
    jobs: &Arc<Mutex<HashMap<u64, JobState>>>,
) -> Result<PathBuf, AppError> {
    let out_dir = crate::output::resolve(cli.output_dir.as_deref())?;
    fs::create_dir_all(&out_dir)?;

    update_job(jobs, job_id, 0.0, "Finding filename...");
    let target = crate::ytdlp::probe_filename(cli, &out_dir)?;
    if target.exists() {
        return Ok(target);
    }

    let cancelled = Arc::new(AtomicBool::new(false));
    let bar = crate::progress::make_bar(true);
    crate::ytdlp::download_with_progress(cli, &out_dir, &bar, &cancelled, |p| {
        update_job(jobs, job_id, p.pct, &format!("{} - ETA {}", p.speed, p.eta));
    })?;
    bar.finish_and_clear();

    Ok(target)
}

fn update_job(jobs: &Arc<Mutex<HashMap<u64, JobState>>>, job_id: u64, pct: f64, message: &str) {
    if let Some(job) = jobs.lock().unwrap().get_mut(&job_id) {
        job.pct = pct.clamp(0.0, 100.0);
        job.message = message.into();
    }
}

fn progress_response(path: &str, state: AppState) -> String {
    let id = path
        .split_once('?')
        .and_then(|(_, query)| query.split('&').find_map(|part| part.strip_prefix("id=")))
        .and_then(|raw| raw.parse::<u64>().ok());

    let Some(id) = id else {
        return r#"{"done":true,"success":false,"pct":0,"message":"Missing job id","detail":""}"#
            .into();
    };

    let jobs = state.jobs.lock().unwrap();
    let Some(job) = jobs.get(&id) else {
        return r#"{"done":true,"success":false,"pct":0,"message":"Job not found","detail":""}"#
            .into();
    };

    format!(
        r#"{{"done":{},"success":{},"pct":{},"message":"{}","detail":"{}"}}"#,
        job.done,
        job.success,
        job.pct,
        escape_json(&job.message),
        escape_json(&job.detail)
    )
}

fn parse_form(body: &[u8]) -> Vec<(String, String)> {
    url::form_urlencoded::parse(body).into_owned().collect()
}

fn form_value<'a>(form: &'a [(String, String)], key: &str) -> &'a str {
    form.iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
        .unwrap_or("")
}

struct ErrorStatus(String);

fn page(status: Option<ErrorStatus>, job_id: Option<u64>) -> String {
    let status_html = match status {
        Some(ErrorStatus(message)) => {
            format!(r#"<p class="status error">{}</p>"#, escape_html(&message))
        }
        None => String::new(),
    };
    let progress_html = job_id.map(progress_panel).unwrap_or_default();
    let script = job_id.map(progress_script).unwrap_or_default();

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>ytmp3</title>
  <style>
    :root {{
      color-scheme: light dark;
      font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: #f6f4ef;
      color: #191917;
    }}
    body {{
      margin: 0;
      min-height: 100vh;
      display: grid;
      place-items: center;
      padding: 24px;
    }}
    main {{
      width: min(100%, 440px);
    }}
    h1 {{
      margin: 0 0 8px;
      font-size: 32px;
      letter-spacing: 0;
    }}
    p {{
      margin: 0 0 24px;
      color: #5f5a50;
      line-height: 1.5;
    }}
    form {{
      display: grid;
      gap: 14px;
      padding: 18px;
      border: 1px solid #ded8cb;
      border-radius: 8px;
      background: #fffdf8;
      box-shadow: 0 16px 40px rgb(25 25 23 / 0.08);
    }}
    label {{
      display: grid;
      gap: 6px;
      font-size: 13px;
      font-weight: 650;
    }}
    input, select {{
      min-height: 42px;
      border: 1px solid #cfc7b8;
      border-radius: 6px;
      padding: 0 12px;
      font: inherit;
      background: #ffffff;
      color: #191917;
    }}
    .row {{
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 12px;
    }}
    button {{
      min-height: 44px;
      border: 0;
      border-radius: 6px;
      font: inherit;
      font-weight: 750;
      background: #2364aa;
      color: #ffffff;
      cursor: pointer;
    }}
    button:hover {{
      background: #174f8b;
    }}
    .status, .progress {{
      margin: 14px 0 0;
      padding: 12px;
      border-radius: 6px;
      font-weight: 650;
    }}
    .success {{
      background: #e6f6ea;
      color: #176b33;
    }}
    .error {{
      background: #fdebea;
      color: #9e2f24;
    }}
    .progress {{
      border: 1px solid #ded8cb;
      background: #fffdf8;
    }}
    .progress-top {{
      display: flex;
      align-items: baseline;
      justify-content: space-between;
      gap: 12px;
      margin-bottom: 10px;
      font-size: 13px;
    }}
    .progress-track {{
      height: 10px;
      overflow: hidden;
      border-radius: 999px;
      background: #e5dfd3;
    }}
    .progress-fill {{
      width: 0%;
      height: 100%;
      border-radius: inherit;
      background: #2364aa;
      transition: width 160ms ease;
    }}
    .progress-detail {{
      margin: 10px 0 0;
      font-size: 13px;
      color: #5f5a50;
      overflow-wrap: anywhere;
    }}
    @media (prefers-color-scheme: dark) {{
      :root {{
        background: #171715;
        color: #f6f4ef;
      }}
      p, .progress-detail {{
        color: #bdb6aa;
      }}
      form, .progress {{
        background: #22211f;
        border-color: #3a3731;
        box-shadow: none;
      }}
      input, select {{
        background: #171715;
        border-color: #4a463f;
        color: #f6f4ef;
      }}
      .progress-track {{
        background: #3a3731;
      }}
    }}
    @media (max-width: 520px) {{
      .row {{
        grid-template-columns: 1fr;
      }}
    }}
  </style>
</head>
<body>
  <main>
    <h1>ytmp3</h1>
    <p>Download audio from a video link.</p>
    <form method="post" action="/download">
      <label>
        URL
        <input name="url" type="url" placeholder="https://youtu.be/..." required autofocus>
      </label>
      <label>
        Output folder
        <input name="output_dir" type="text" placeholder="Default: Downloads">
      </label>
      <div class="row">
        <label>
          Format
          <select name="format">
            <option value="mp3">MP3</option>
            <option value="m4a">M4A</option>
            <option value="opus">Opus</option>
          </select>
        </label>
        <label>
          Bitrate
          <input name="bitrate" type="number" min="32" max="320" value="128">
        </label>
      </div>
      <button type="submit">Download</button>
    </form>
    {status_html}
    {progress_html}
  </main>
  {script}
</body>
</html>"#
    )
}

fn progress_panel(job_id: u64) -> String {
    format!(
        r#"<section class="progress" data-job-id="{job_id}">
      <div class="progress-top">
        <span id="progress-message">Preparing download...</span>
        <span id="progress-percent">0%</span>
      </div>
      <div class="progress-track" aria-hidden="true">
        <div id="progress-fill" class="progress-fill"></div>
      </div>
      <p id="progress-detail" class="progress-detail"></p>
    </section>"#
    )
}

fn progress_script(job_id: u64) -> String {
    format!(
        r#"<script>
    const jobId = {job_id};
    const fill = document.getElementById("progress-fill");
    const percent = document.getElementById("progress-percent");
    const message = document.getElementById("progress-message");
    const detail = document.getElementById("progress-detail");

    async function pollProgress() {{
      try {{
        const response = await fetch(`/progress?id=${{jobId}}`, {{ cache: "no-store" }});
        const data = await response.json();
        const pct = Math.max(0, Math.min(100, Number(data.pct) || 0));
        fill.style.width = `${{pct}}%`;
        percent.textContent = `${{Math.round(pct)}}%`;
        message.textContent = data.message || "Working...";
        detail.textContent = data.detail || "";
        if (!data.done) {{
          setTimeout(pollProgress, 600);
        }} else if (!data.success) {{
          message.textContent = "Failed";
        }}
      }} catch (error) {{
        message.textContent = "Waiting for progress...";
        setTimeout(pollProgress, 1000);
      }}
    }}

    pollProgress();
  </script>"#
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn escape_json(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}
