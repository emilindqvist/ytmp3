use crate::cli::Cli;
use crate::error::AppError;
use crate::progress;
use indicatif::ProgressBar;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const YTDLP: &str = "yt-dlp";

fn common_args(cli: &Cli, out_dir: &Path) -> Vec<String> {
    let template = format!(
        "{}/%(title)s.%(ext)s",
        out_dir.to_string_lossy().trim_end_matches('/')
    );
    vec![
        "--no-playlist".into(),
        "--restrict-filenames".into(),
        "--extract-audio".into(),
        "--audio-format".into(),
        cli.format.as_str().into(),
        "-o".into(),
        template,
    ]
}

pub fn probe_filename(cli: &Cli, out_dir: &Path) -> Result<PathBuf, AppError> {
    let url = required_url(cli)?;
    let mut args = common_args(cli, out_dir);
    args.extend([
        "--simulate".into(),
        "--print".into(),
        "filename".into(),
        url.to_string(),
    ]);

    let output = Command::new(YTDLP)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(map_spawn_err)?;

    if !output.status.success() {
        return Err(classify_exit(
            output.status.code().unwrap_or(-1),
            &output.stderr,
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw = stdout
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty() && !l.starts_with("WARNING:"))
        .unwrap_or("");
    if raw.is_empty() {
        return Err(AppError::YtDlp {
            code: 0,
            stderr: "yt-dlp produced no filename in dry-run".into(),
        });
    }
    let mut path = PathBuf::from(raw);
    path.set_extension(cli.format.as_str());
    Ok(path)
}

pub fn download(
    cli: &Cli,
    out_dir: &Path,
    bar: &ProgressBar,
    cancelled: &Arc<AtomicBool>,
) -> Result<(), AppError> {
    download_with_progress(cli, out_dir, bar, cancelled, |_| {})
}

pub fn download_with_progress<F>(
    cli: &Cli,
    out_dir: &Path,
    bar: &ProgressBar,
    cancelled: &Arc<AtomicBool>,
    mut on_progress: F,
) -> Result<(), AppError>
where
    F: FnMut(&progress::Progress),
{
    let url = required_url(cli)?;
    let mut args = common_args(cli, out_dir);
    args.extend([
        "--newline".into(),
        "--audio-quality".into(),
        format!("{}K", cli.bitrate),
        "--progress-template".into(),
        "download:PROGRESS %(progress._percent_str)s %(progress._speed_str)s %(progress._eta_str)s"
            .into(),
        url.to_string(),
    ]);

    let mut child = Command::new(YTDLP)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(map_spawn_err)?;

    stream_stdout(&mut child, bar, cancelled, &mut on_progress)?;

    let status = child.wait().map_err(AppError::Io)?;

    if cancelled.load(Ordering::SeqCst) {
        return Err(AppError::Cancelled);
    }
    match status.code() {
        Some(0) => Ok(()),
        Some(code) => {
            let mut stderr = Vec::new();
            if let Some(mut se) = child.stderr.take() {
                let _ = se.read_to_end(&mut stderr);
            }
            Err(classify_exit(code, &stderr))
        }
        None => Err(AppError::Cancelled),
    }
}

fn required_url(cli: &Cli) -> Result<&str, AppError> {
    cli.url
        .as_deref()
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("missing URL; pass a URL or use --ui")))
}

fn stream_stdout(
    child: &mut Child,
    bar: &ProgressBar,
    cancelled: &Arc<AtomicBool>,
    on_progress: &mut dyn FnMut(&progress::Progress),
) -> Result<(), AppError> {
    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => return Ok(()),
    };
    let reader = BufReader::new(stdout);
    for line in reader.lines() {
        if cancelled.load(Ordering::SeqCst) {
            let _ = child.kill();
            break;
        }
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if let Some(p) = progress::parse(&line) {
            progress::apply(bar, &p);
            on_progress(&p);
        }
    }
    Ok(())
}

fn map_spawn_err(e: io::Error) -> AppError {
    if e.kind() == io::ErrorKind::NotFound {
        AppError::YtDlpMissing
    } else {
        AppError::Spawn(e)
    }
}

fn classify_exit(code: i32, stderr: &[u8]) -> AppError {
    let msg = String::from_utf8_lossy(stderr).to_string();
    let lower = msg.to_ascii_lowercase();
    if lower.contains("ffmpeg")
        && (lower.contains("not installed")
            || lower.contains("not found")
            || lower.contains("could not find")
            || lower.contains("ffprobe"))
    {
        return AppError::FfmpegMissing;
    }
    AppError::YtDlp {
        code,
        stderr: msg.trim().to_string(),
    }
}
