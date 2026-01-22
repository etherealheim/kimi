use color_eyre::Result;
use directories::UserDirs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub fn download_video_with_progress(url: &str, mut on_progress: impl FnMut(u8)) -> Result<()> {
    let desktop_dir = resolve_desktop_dir()?;
    let output_template = desktop_dir.join("%(title)s.%(ext)s");
    let output = output_template
        .to_str()
        .ok_or_else(|| color_eyre::eyre::eyre!("Invalid desktop path"))?;

    let mut child = Command::new("yt-dlp")
        .arg("--newline")
        .arg("--no-playlist")
        .arg("-o")
        .arg(output)
        .arg(url)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

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
        Ok(())
    } else {
        Err(color_eyre::eyre::eyre!("yt-dlp failed"))
    }
}

fn resolve_desktop_dir() -> Result<PathBuf> {
    if let Some(user_dirs) = UserDirs::new() {
        if let Some(desktop_dir) = user_dirs.desktop_dir() {
            return Ok(desktop_dir.to_path_buf());
        }
        let home_dir = user_dirs.home_dir().to_path_buf();
        return Ok(home_dir.join("Desktop"));
    }
    Err(color_eyre::eyre::eyre!("Could not locate desktop directory"))
}

fn parse_progress_percent(line: &str) -> Option<u8> {
    let percent_pos = line.find('%')?;
    let prefix = &line[..percent_pos];
    let number_part = prefix.split_whitespace().last()?;
    let value: f32 = number_part.parse().ok()?;
    let clamped = value.clamp(0.0, 100.0) as u8;
    Some(clamped)
}
