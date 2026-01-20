use color_eyre::Result;
use directories::UserDirs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub fn convert_file(input_path: &str, target_format: &str) -> Result<PathBuf> {
    let input = Path::new(input_path);
    let file_stem = input
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| color_eyre::eyre::eyre!("Invalid input file name"))?;

    let format = normalize_format(target_format)?;
    let desktop_dir = resolve_desktop_dir()?;
    let output_path = desktop_dir.join(format!("{}.{}", file_stem, format));

    let status = Command::new("ffmpeg")
        .arg("-y")
        .arg("-i")
        .arg(input)
        .args(build_format_args(&format))
        .arg(&output_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if status.success() {
        Ok(output_path)
    } else {
        Err(color_eyre::eyre::eyre!("ffmpeg failed"))
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

fn normalize_format(format: &str) -> Result<String> {
    let normalized = format.trim().to_lowercase();
    match normalized.as_str() {
        "mp4" | "webm" | "mov" | "gif" | "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tiff" => {
            Ok(normalized)
        }
        _ => Err(color_eyre::eyre::eyre!("Unsupported format: {}", format)),
    }
}

fn build_format_args(format: &str) -> Vec<&'static str> {
    match format {
        "gif" => vec![
            "-vf",
            "fps=12,scale=640:-1:flags=lanczos",
            "-loop",
            "0",
        ],
        _ => Vec::new(),
    }
}
