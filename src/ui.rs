use crate::cli::{Cli, Format};
use crate::error::AppError;
use clap::ValueEnum;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

pub fn serve(host: &str, port: u16) -> Result<(), AppError> {
    let addr = format!("{host}:{port}");
    let listener = TcpListener::bind(&addr)?;
    println!("UI running at http://{addr}");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(e) = handle(stream) {
                    eprintln!("ui error: {e}");
                }
            }
            Err(e) => eprintln!("ui connection error: {e}"),
        }
    }

    Ok(())
}

fn handle(mut stream: TcpStream) -> Result<(), AppError> {
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

    let response = if request_line.starts_with("POST /download ") {
        download_response(&body)
    } else {
        page(None)
    };

    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.len(),
        response
    )?;
    Ok(())
}

fn download_response(body: &[u8]) -> String {
    let form = parse_form(body);
    let url = form_value(&form, "url").trim().to_string();
    if url.is_empty() {
        return page(Some(Status::Error("Paste a URL first.".into())));
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

    match crate::run(cli) {
        Ok(()) => page(Some(Status::Success("Download complete.".into()))),
        Err(e) => page(Some(Status::Error(e.to_string()))),
    }
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

enum Status {
    Success(String),
    Error(String),
}

fn page(status: Option<Status>) -> String {
    let status_html = match status {
        Some(Status::Success(message)) => {
            format!(r#"<p class="status success">{}</p>"#, escape_html(&message))
        }
        Some(Status::Error(message)) => {
            format!(r#"<p class="status error">{}</p>"#, escape_html(&message))
        }
        None => String::new(),
    };

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
    .status {{
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
    @media (prefers-color-scheme: dark) {{
      :root {{
        background: #171715;
        color: #f6f4ef;
      }}
      p {{
        color: #bdb6aa;
      }}
      form {{
        background: #22211f;
        border-color: #3a3731;
        box-shadow: none;
      }}
      input, select {{
        background: #171715;
        border-color: #4a463f;
        color: #f6f4ef;
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
  </main>
</body>
</html>"#
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
