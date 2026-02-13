use color_eyre::eyre::{eyre, WrapErr};
use color_eyre::Result;
use directories::UserDirs;
use std::io::{BufRead, BufReader, Read as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Downloads a video using yt-dlp with real-time progress updates.
pub fn download_video_with_progress(url: &str, mut on_progress: impl FnMut(u8)) -> Result<()> {
    let download_dir = resolve_download_dir()?;
    let output_template = download_dir.join("%(title)s.%(ext)s");
    let output_path = output_template
        .to_str()
        .ok_or_else(|| eyre!("Download path contains invalid characters"))?;

    let mut child = Command::new("yt-dlp")
        .args(["--newline", "--progress", "--no-playlist"])
        .args(["-o", output_path])
        .arg(url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .wrap_err("Failed to start yt-dlp — is it installed and in your PATH?")?;

    // Drain stderr in a background thread to prevent pipe buffer deadlocks
    let stderr_thread = child.stderr.take().map(|stderr| {
        std::thread::spawn(move || {
            let mut buffer = String::new();
            let _ = BufReader::new(stderr).read_to_string(&mut buffer);
            buffer
        })
    });

    // Read stdout line-by-line for progress updates
    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if let Some(progress) = parse_progress_percent(&line) {
                on_progress(progress);
            }
        }
    }

    let status = child.wait()?;
    if status.success() {
        on_progress(100);
        Ok(())
    } else {
        let stderr_output = stderr_thread
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default();
        let detail = extract_ytdlp_error(&stderr_output);
        Err(eyre!("Download failed: {}", detail))
    }
}

/// Resolves download directory: Downloads > Desktop > Home
fn resolve_download_dir() -> Result<PathBuf> {
    let user_dirs =
        UserDirs::new().ok_or_else(|| eyre!("Could not determine home directory"))?;

    let dir = user_dirs
        .download_dir()
        .or_else(|| user_dirs.desktop_dir())
        .map_or_else(|| user_dirs.home_dir().to_path_buf(), Path::to_path_buf);

    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .wrap_err_with(|| format!("Could not create download directory: {}", dir.display()))?;
    }

    Ok(dir)
}

/// Extracts meaningful error messages from yt-dlp stderr output.
fn extract_ytdlp_error(stderr: &str) -> String {
    // yt-dlp prefixes errors with "ERROR:"
    let error_lines: Vec<&str> = stderr
        .lines()
        .filter(|line| line.starts_with("ERROR:"))
        .collect();

    if error_lines.is_empty() {
        // Fall back to last non-empty line of stderr
        stderr
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("unknown error (no output from yt-dlp)")
            .to_string()
    } else {
        error_lines.join("; ")
    }
}

/// Parses progress percentage from yt-dlp output lines.
/// Lines look like: `[download]  45.3% of ~100.00MiB ...`
fn parse_progress_percent(line: &str) -> Option<u8> {
    if !line.contains('%') {
        return None;
    }
    let percent_pos = line.find('%')?;
    let prefix = line.get(..percent_pos)?;
    let number_part = prefix.split_whitespace().last()?;
    let value: f32 = number_part.parse().ok()?;
    let clamped = value.clamp(0.0, 100.0) as u8;
    Some(clamped)
}
