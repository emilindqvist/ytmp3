use std::path::PathBuf;

#[derive(clap::Parser, Debug)]
#[command(
    version,
    about = "Download video as audio (YouTube, Instagram, TikTok, X/Twitter, and more) via yt-dlp"
)]
pub struct Cli {
    /// Video URL (YouTube, Instagram, TikTok, X/Twitter, ...)
    pub url: Option<String>,

    /// Output directory (overrides $YTMP3_DIR and platform defaults)
    #[arg(short = 'o', long = "output-dir", value_name = "DIR")]
    pub output_dir: Option<PathBuf>,

    /// Audio bitrate in kbps (32–320)
    #[arg(
        short = 'b',
        long,
        default_value_t = 128,
        value_parser = clap::value_parser!(u32).range(32..=320),
    )]
    pub bitrate: u32,

    /// Audio format
    #[arg(short = 'f', long, value_enum, default_value_t = Format::Mp3)]
    pub format: Format,

    /// Suppress progress bar (still prints result path)
    #[arg(short = 'q', long)]
    pub quiet: bool,

    /// Start a tiny local web UI instead of downloading from the terminal
    #[arg(long)]
    pub ui: bool,

    /// Host for the local web UI
    #[arg(long = "ui-host", default_value = "127.0.0.1")]
    pub ui_host: String,

    /// Port for the local web UI
    #[arg(long = "ui-port", default_value_t = 8787)]
    pub ui_port: u16,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub enum Format {
    Mp3,
    M4a,
    Opus,
}

impl Format {
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Mp3 => "mp3",
            Format::M4a => "m4a",
            Format::Opus => "opus",
        }
    }
}
