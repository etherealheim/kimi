use color_eyre::Result;
use directories::ProjectDirs;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const PERSONALITY_FILE_NAME: &str = "Casca.txt";

pub fn ensure_personality_file() -> Result<PathBuf> {
    let config_dir = ProjectDirs::from("", "", "kimi")
        .ok_or_else(|| color_eyre::eyre::eyre!("Could not determine config directory"))?
        .config_dir()
        .to_path_buf();
    fs::create_dir_all(&config_dir)?;

    let personality_path = config_dir.join(PERSONALITY_FILE_NAME);
    if !personality_path.exists() {
        fs::write(&personality_path, default_personality_template())?;
    }

    Ok(personality_path)
}

pub fn open_personality_in_new_terminal() -> Result<()> {
    let personality_path = ensure_personality_file()?;
    let personality_str = personality_path.to_string_lossy().to_string();

    let mut attempts: Vec<(String, Vec<String>)> = Vec::new();

    if let Ok(terminal) = std::env::var("TERMINAL") {
        attempts.push((terminal, vec!["-e".to_string(), "micro".to_string(), personality_str.clone()]));
    }

    attempts.extend([
        (
            "x-terminal-emulator".to_string(),
            vec!["-e".to_string(), "micro".to_string(), personality_str.clone()],
        ),
        (
            "gnome-terminal".to_string(),
            vec!["--".to_string(), "micro".to_string(), personality_str.clone()],
        ),
        (
            "konsole".to_string(),
            vec!["-e".to_string(), "micro".to_string(), personality_str.clone()],
        ),
        (
            "kitty".to_string(),
            vec!["-e".to_string(), "micro".to_string(), personality_str.clone()],
        ),
        (
            "alacritty".to_string(),
            vec!["-e".to_string(), "micro".to_string(), personality_str.clone()],
        ),
        (
            "wezterm".to_string(),
            vec![
                "start".to_string(),
                "--".to_string(),
                "micro".to_string(),
                personality_str.clone(),
            ],
        ),
        (
            "xterm".to_string(),
            vec!["-e".to_string(), "micro".to_string(), personality_str.clone()],
        ),
    ]);

    for (program, args) in attempts {
        if try_spawn_terminal(&program, &args) {
            return Ok(());
        }
    }

    Err(color_eyre::eyre::eyre!(
        "No supported terminal emulator found"
    ))
}

pub fn open_personality_in_place() -> Result<()> {
    let personality_path = ensure_personality_file()?;
    let status = Command::new("micro").arg(personality_path).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(color_eyre::eyre::eyre!("Micro exited with error"))
    }
}

#[allow(dead_code)]
pub fn read_personality() -> Result<String> {
    let personality_path = ensure_personality_file()?;
    Ok(fs::read_to_string(personality_path)?)
}

pub fn personality_name() -> Result<String> {
    let personality_path = ensure_personality_file()?;
    personality_path
        .file_stem()
        .and_then(|name| name.to_str())
        .map(String::from)
        .ok_or_else(|| color_eyre::eyre::eyre!("Invalid personality file name"))
}

fn try_spawn_terminal(program: &str, args: &[String]) -> bool {
    Command::new(program).args(args).spawn().is_ok()
}

fn default_personality_template() -> String {
    [
        "You are Kimi, a helpful AI assistant.",
        "Be concise, friendly, and direct.",
        "Keep responses short and to the point unless asked for details.",
    ]
    .join("\n")
}
