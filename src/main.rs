mod cli;
mod error;
mod output;
mod platform;
mod progress;
mod ui;
mod ytdlp;

use crate::cli::Cli;
use crate::error::AppError;
use clap::Parser;
use std::fs;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = if cli.ui {
        ui::serve(&cli.ui_host, cli.ui_port)
    } else {
        run(cli)
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(e.exit_code() as u8)
        }
    }
}

fn run(cli: Cli) -> Result<(), AppError> {
    let url = cli
        .url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing URL; pass a URL or use --ui"))?;

    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let c = Arc::clone(&cancelled);
        let _ = ctrlc::set_handler(move || {
            c.store(true, Ordering::SeqCst);
        });
    }

    let plat = platform::detect(url);
    eprintln!("Detected: {plat}");

    let out_dir = output::resolve(cli.output_dir.as_deref())?;
    fs::create_dir_all(&out_dir)?;

    let target = ytdlp::probe_filename(&cli, &out_dir)?;
    if target.exists() {
        println!("Already exists: {}", target.display());
        return Ok(());
    }

    let bar = progress::make_bar(cli.quiet);
    let result = ytdlp::download(&cli, &out_dir, &bar, &cancelled);
    bar.finish_and_clear();
    result?;

    println!("Saved: {}", target.display());
    Ok(())
}
