# ytmp3

Download audio from YouTube, Instagram, TikTok, X/Twitter, and [~1800 other sites](https://github.com/yt-dlp/yt-dlp/blob/master/supportedsites.md) as MP3, M4A, or Opus. Single binary, no venv, no `python3` prefix.

Thin Rust wrapper around [yt-dlp](https://github.com/yt-dlp/yt-dlp) and [ffmpeg](https://ffmpeg.org/).

## Install

### From crates.io

Requires the Rust toolchain (`cargo`). Install via [rustup](https://rustup.rs/) if you don't have it:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then:

```bash
cargo install ytmp3
```

### Pre-built binaries

Grab the latest build for your platform from [Releases](https://github.com/emilindqvist/YT-mp3-Converter/releases) and drop it somewhere on your `PATH`.

### Prerequisites

`ytmp3` shells out to two external tools — install them first:

| Tool | macOS | Linux/WSL | Windows |
|---|---|---|---|
| yt-dlp | `brew install yt-dlp` | `pip install yt-dlp` | `pip install yt-dlp` or [binary](https://github.com/yt-dlp/yt-dlp/releases) |
| ffmpeg | `brew install ffmpeg` | `apt install ffmpeg` | `winget install ffmpeg` or [binary](https://www.gyan.dev/ffmpeg/builds/) |

## Usage

```bash
ytmp3 <url>
```

Or start the tiny local UI:

```bash
ytmp3 --ui
```

Then open `http://127.0.0.1:8787`.

Examples:

```bash
ytmp3 "https://youtu.be/dQw4w9WgXcQ"
ytmp3 "https://www.instagram.com/reel/<id>/"
ytmp3 "https://www.tiktok.com/@user/video/<id>"
ytmp3 "https://x.com/<user>/status/<id>"
```

### Flags

| Flag | Default | Description |
|---|---|---|
| `-o, --output-dir <DIR>` | (see below) | Where to save the file |
| `-b, --bitrate <KBPS>` | `128` | Audio bitrate, 32–320 |
| `-f, --format <FMT>` | `mp3` | `mp3`, `m4a`, or `opus` |
| `-q, --quiet` | off | Suppress progress bar |

### Save-path priority

1. `-o / --output-dir` flag
2. `$YTMP3_DIR` environment variable
3. Windows `Downloads/` (auto-detected under WSL for your Windows user)
4. `~/Downloads`

If the MP3/M4A/Opus already exists at the resolved path, `ytmp3` prints `Already exists: <path>` and exits 0 without re-downloading.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success (or already exists) |
| 1 | Download or generic error |
| 2 | `yt-dlp` not found on PATH |
| 3 | `ffmpeg` not found on PATH |
| 130 | Cancelled with Ctrl-C |

## Development

```bash
cargo build --release
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Releases are cut by pushing a `v*` tag — CI builds four platform binaries and publishes to crates.io.

## License

MIT — see [LICENSE](LICENSE).
