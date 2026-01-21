use color_eyre::Result;
use directories::ProjectDirs;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DEFAULT_PERSONALITY_NAME: &str = "Casca";
const MY_PERSONALITY_NAME: &str = "My personality";
const MEMORIES_ENTRY_NAME: &str = "Memories";
const PERSONALITY_EXTENSION: &str = "md";
const LEGACY_PERSONALITY_EXTENSION: &str = "txt";

pub fn default_personality_name() -> String {
    DEFAULT_PERSONALITY_NAME.to_string()
}

pub fn my_personality_name() -> String {
    MY_PERSONALITY_NAME.to_string()
}

pub fn list_personalities() -> Result<Vec<String>> {
    let personality_dir = personality_dir()?;
    let mut names = Vec::new();
    for entry in fs::read_dir(&personality_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some(PERSONALITY_EXTENSION) {
            continue;
        }
        if let Some(name) = path.file_stem().and_then(|name| name.to_str()) {
            if name != MY_PERSONALITY_NAME && name != MEMORIES_ENTRY_NAME {
                names.push(name.to_string());
            }
        }
    }
    names.sort();

    if names.is_empty() {
        let default_name = default_personality_name();
        let _ = ensure_personality(&default_name)?;
        return Ok(vec![default_name]);
    }

    Ok(names)
}

pub fn ensure_personality(name: &str) -> Result<PathBuf> {
    let personality_path = personality_path(name)?;
    if !personality_path.exists() {
        let legacy_path = legacy_personality_path(name)?;
        if legacy_path.exists() {
            fs::rename(&legacy_path, &personality_path)?;
        }
    }
    if !personality_path.exists() {
        fs::write(&personality_path, default_personality_template())?;
    }
    Ok(personality_path)
}

pub fn ensure_my_personality() -> Result<PathBuf> {
    let name = my_personality_name();
    let personality_path = personality_path(&name)?;
    if !personality_path.exists() {
        let legacy_path = legacy_personality_path(&name)?;
        if legacy_path.exists() {
            fs::rename(&legacy_path, &personality_path)?;
        }
    }
    if !personality_path.exists() {
        fs::write(&personality_path, my_personality_template())?;
    }
    Ok(personality_path)
}

pub fn create_personality(name: &str) -> Result<PathBuf> {
    let personality_path = personality_path(name)?;
    if personality_path.exists() {
        return Err(color_eyre::eyre::eyre!(
            "Personality '{}' already exists",
            name
        ));
    }
    fs::write(&personality_path, default_personality_template())?;
    Ok(personality_path)
}

pub fn delete_personality(name: &str) -> Result<()> {
    let personality_path = personality_path(name)?;
    if personality_path.exists() {
        fs::remove_file(personality_path)?;
    }
    Ok(())
}

pub fn read_personality(name: &str) -> Result<String> {
    let personality_path = ensure_personality(name)?;
    Ok(fs::read_to_string(personality_path)?)
}

pub fn read_my_personality() -> Result<String> {
    let name = my_personality_name();
    let personality_path = ensure_my_personality()?;
    let _ = name;
    Ok(fs::read_to_string(personality_path)?)
}

pub fn open_personality_in_new_terminal(name: &str) -> Result<()> {
    let personality_path = ensure_personality(name)?;
    let personality_str = personality_path.to_string_lossy().to_string();

    let mut attempts: Vec<(String, Vec<String>)> = Vec::new();

    if let Ok(terminal) = std::env::var("TERMINAL") {
        attempts.push((
            terminal,
            vec!["-e".to_string(), "micro".to_string(), personality_str.clone()],
        ));
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

pub fn open_personality_in_place(name: &str) -> Result<()> {
    let personality_path = ensure_personality(name)?;
    let status = Command::new("micro").arg(personality_path).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(color_eyre::eyre::eyre!("Micro exited with error"))
    }
}

pub fn personality_dir() -> Result<PathBuf> {
    let base_dir = project_data_dir()?;
    let personality_dir = base_dir.join("personalities");
    fs::create_dir_all(&personality_dir)?;
    migrate_legacy_personality_dir(&personality_dir)?;
    Ok(personality_dir)
}

fn personality_path(name: &str) -> Result<PathBuf> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(color_eyre::eyre::eyre!(
            "Personality name cannot be empty"
        ));
    }
    let personality_dir = personality_dir()?;
    Ok(personality_dir.join(format!("{}.{}", trimmed, PERSONALITY_EXTENSION)))
}

fn legacy_personality_path(name: &str) -> Result<PathBuf> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(color_eyre::eyre::eyre!(
            "Personality name cannot be empty"
        ));
    }
    let legacy_dir = legacy_personality_dir()?;
    Ok(legacy_dir.join(format!(
        "{}.{}",
        trimmed, LEGACY_PERSONALITY_EXTENSION
    )))
}

fn project_data_dir() -> Result<PathBuf> {
    let current_dir = std::env::current_dir()?;
    Ok(current_dir.join("data"))
}

fn legacy_personality_dir() -> Result<PathBuf> {
    let config_dir = ProjectDirs::from("", "", "kimi")
        .ok_or_else(|| color_eyre::eyre::eyre!("Could not determine config directory"))?
        .config_dir()
        .to_path_buf();
    Ok(config_dir.join("personalities"))
}

fn try_spawn_terminal(program: &str, args: &[String]) -> bool {
    Command::new(program).args(args).spawn().is_ok()
}

fn default_personality_template() -> String {
    [
        "You are Kimi, a helpful AI assistant.",
        "Be warm, thoughtful, and clear.",
        "Share enough detail to be genuinely helpful, with a short summary first.",
    ]
    .join("\n")
}

fn my_personality_template() -> String {
    [
        "[always]",
        "Name: Lukas",
        "Location: Prague",
        "Timezone: CET",
        "Tone: direct, concise",
        "",
        "[context:games]",
        "Plays: Elden Ring",
        "Likes: soulslike games",
        "",
        "[context:food]",
        "Prefers: spicy",
        "Allergy: none",
    ]
    .join("\n")
}

fn migrate_legacy_personality_files(personality_dir: &PathBuf) -> Result<()> {
    for entry in fs::read_dir(personality_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some(LEGACY_PERSONALITY_EXTENSION) {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|name| name.to_str()) else {
            continue;
        };
        let target = personality_dir.join(format!("{}.{}", stem, PERSONALITY_EXTENSION));
        if !target.exists() {
            fs::rename(&path, target)?;
        }
    }
    Ok(())
}

fn migrate_legacy_personality_dir(target_dir: &PathBuf) -> Result<()> {
    let legacy_dir = legacy_personality_dir()?;
    if !legacy_dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(&legacy_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(file_name) = path.file_name() else {
            continue;
        };
        let dest = target_dir.join(file_name);
        if dest.exists() {
            continue;
        }
        let _ = fs::copy(&path, &dest);
    }
    migrate_legacy_personality_files(target_dir)?;
    Ok(())
}
