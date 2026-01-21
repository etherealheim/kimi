use color_eyre::Result;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const MEMORIES_FILE_NAME: &str = "Memories";
const MEMORIES_EXTENSION: &str = "md";

const ALLOWED_CONTEXT_TAGS: [&str; 7] = [
    "likes",
    "dislikes",
    "location",
    "timezone",
    "tools",
    "projects",
    "topics",
];

#[derive(Debug, Clone, Default)]
pub struct MemoryBlocks {
    pub contexts: BTreeMap<String, Vec<String>>,
}

impl MemoryBlocks {
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn to_string(&self) -> String {
        let mut output = Vec::new();
        for tag in ordered_context_tags(&self.contexts) {
            output.push(format!("[context:{}]", tag));
            if let Some(lines) = self.contexts.get(tag) {
                output.extend(lines.iter().cloned());
            }
            output.push(String::new());
        }

        output.join("\n").trim_end().to_string()
    }
}

pub fn ensure_memories() -> Result<PathBuf> {
    let dir = crate::services::personality::personality_dir()?;
    let path = dir.join(format!("{}.{}", MEMORIES_FILE_NAME, MEMORIES_EXTENSION));
    if !path.exists() {
        fs::write(&path, default_memories_template())?;
    }
    Ok(path)
}

pub fn read_memories() -> Result<String> {
    let path = ensure_memories()?;
    Ok(fs::read_to_string(path)?)
}

pub fn write_memories(blocks: &MemoryBlocks) -> Result<()> {
    let path = ensure_memories()?;
    fs::write(path, blocks.to_string())?;
    Ok(())
}

pub fn open_memories_in_new_terminal() -> Result<()> {
    let path = ensure_memories()?;
    let path_str = path.to_string_lossy().to_string();

    let mut attempts: Vec<(String, Vec<String>)> = Vec::new();

    if let Ok(terminal) = std::env::var("TERMINAL") {
        attempts.push((
            terminal,
            vec!["-e".to_string(), "micro".to_string(), path_str.clone()],
        ));
    }

    attempts.extend([
        (
            "x-terminal-emulator".to_string(),
            vec!["-e".to_string(), "micro".to_string(), path_str.clone()],
        ),
        (
            "gnome-terminal".to_string(),
            vec!["--".to_string(), "micro".to_string(), path_str.clone()],
        ),
        (
            "konsole".to_string(),
            vec!["-e".to_string(), "micro".to_string(), path_str.clone()],
        ),
        (
            "kitty".to_string(),
            vec!["-e".to_string(), "micro".to_string(), path_str.clone()],
        ),
        (
            "alacritty".to_string(),
            vec!["-e".to_string(), "micro".to_string(), path_str.clone()],
        ),
        (
            "wezterm".to_string(),
            vec![
                "start".to_string(),
                "--".to_string(),
                "micro".to_string(),
                path_str.clone(),
            ],
        ),
        (
            "xterm".to_string(),
            vec!["-e".to_string(), "micro".to_string(), path_str.clone()],
        ),
    ]);

    for (program, args) in attempts {
        if Command::new(program).args(args).spawn().is_ok() {
            return Ok(());
        }
    }

    Err(color_eyre::eyre::eyre!(
        "No supported terminal emulator found"
    ))
}

pub fn open_memories_in_place() -> Result<()> {
    let path = ensure_memories()?;
    let status = Command::new("micro").arg(path).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(color_eyre::eyre::eyre!("Micro exited with error"))
    }
}

pub fn parse_memory_blocks(content: &str) -> MemoryBlocks {
    let mut blocks = MemoryBlocks::empty();
    let mut current_kind: Option<MemoryKind> = None;
    let mut current_lines: Vec<String> = Vec::new();

    let flush = |blocks: &mut MemoryBlocks,
                 kind: &mut Option<MemoryKind>,
                 lines: &mut Vec<String>| {
        let Some(kind) = kind.take() else {
            lines.clear();
            return;
        };
        let cleaned = collect_clean_lines(lines);
        let MemoryKind::Context(tag) = kind;
        blocks.contexts.entry(tag).or_default().extend(cleaned);
        lines.clear();
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(tag) = trimmed
            .strip_prefix("[context:")
            .and_then(|value| value.strip_suffix(']'))
        {
            flush(&mut blocks, &mut current_kind, &mut current_lines);
            current_kind = Some(MemoryKind::Context(tag.trim().to_lowercase()));
            continue;
        }
        current_lines.push(line.to_string());
    }

    flush(&mut blocks, &mut current_kind, &mut current_lines);
    blocks
}

pub fn filter_extracted_blocks(blocks: MemoryBlocks) -> MemoryBlocks {
    let mut filtered = MemoryBlocks::empty();
    for (tag, lines) in blocks.contexts {
        if is_allowed_context_tag(&tag) {
            filtered.contexts.insert(tag, lines);
        }
    }
    filtered
}

pub fn merge_memory_blocks(existing: MemoryBlocks, incoming: MemoryBlocks) -> MemoryBlocks {
    let mut merged = existing;
    for (tag, lines) in incoming.contexts {
        if !is_allowed_context_tag(&tag) {
            continue;
        }
        let current = merged.contexts.entry(tag).or_default();
        let updated = merge_lines(std::mem::take(current), lines);
        *current = updated;
    }
    merged
}

#[derive(Debug, Clone)]
enum MemoryKind {
    Context(String),
}

fn ordered_context_tags(contexts: &BTreeMap<String, Vec<String>>) -> Vec<&str> {
    let mut tags = Vec::new();
    for tag in ALLOWED_CONTEXT_TAGS {
        tags.push(tag);
    }
    for tag in contexts.keys() {
        if !ALLOWED_CONTEXT_TAGS.iter().any(|known| known == tag) {
            tags.push(tag);
        }
    }
    tags
}

fn collect_clean_lines(lines: &[String]) -> Vec<String> {
    let mut cleaned = Vec::new();
    for line in lines {
        let value = clean_line(line);
        if value.is_empty() || is_placeholder_value(&value) {
            continue;
        }
        cleaned.push(value);
    }
    cleaned
}

fn clean_line(line: &str) -> String {
    let trimmed = line.trim();
    let without_prefix = trimmed
        .trim_start_matches("- ")
        .trim_start_matches('•')
        .trim_start_matches(' ')
        .trim();
    without_prefix.to_string()
}

fn merge_lines(existing: Vec<String>, incoming: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    let mut index_by_key: HashMap<String, usize> = HashMap::new();

    for line in existing {
        let normalized = normalize_line(&line);
        if normalized.is_empty() {
            continue;
        }
        if let Some(index) = index_by_key.get(&normalized).copied() {
            result[index] = line;
        } else {
            index_by_key.insert(normalized, result.len());
            result.push(line);
        }
    }

    for line in incoming {
        let normalized = normalize_line(&line);
        if normalized.is_empty() {
            continue;
        }
        if let Some(index) = index_by_key.get(&normalized).copied() {
            result[index] = line;
        } else {
            index_by_key.insert(normalized, result.len());
            result.push(line);
        }
    }

    result
}

fn normalize_line(line: &str) -> String {
    let cleaned = clean_line(line);
    if cleaned.is_empty() || is_placeholder_value(&cleaned) {
        return String::new();
    }
    let entry = parse_entry(&cleaned);
    format!("{}@{}", entry.value.to_lowercase(), entry.context.to_lowercase())
}

fn is_allowed_context_tag(tag: &str) -> bool {
    ALLOWED_CONTEXT_TAGS.iter().any(|known| known == &tag)
}

#[derive(Debug, Clone)]
struct MemoryEntry {
    value: String,
    context: String,
}

fn parse_entry(line: &str) -> MemoryEntry {
    let mut parts = line.split('|').map(str::trim).filter(|part| !part.is_empty());
    let value = parts.next().unwrap_or_default().to_string();
    let mut context = "general".to_string();
    for part in parts {
        if let Some((key, raw_value)) = split_metadata(part) {
            let normalized_key = key.to_lowercase();
            let trimmed_value = raw_value.trim().to_string();
            if trimmed_value.is_empty() {
                continue;
            }
            match normalized_key.as_str() {
                "context" => context = trimmed_value,
                _ => {}
            }
        }
    }

    MemoryEntry { value, context }
}

fn split_metadata(part: &str) -> Option<(&str, &str)> {
    if let Some((key, value)) = part.split_once('=') {
        return Some((key.trim(), value.trim()));
    }
    if let Some((key, value)) = part.split_once(':') {
        return Some((key.trim(), value.trim()));
    }
    None
}

fn is_placeholder_value(value: &str) -> bool {
    let lowered = value.trim().to_lowercase();
    if lowered.is_empty() {
        return true;
    }
    if lowered == "n/a" || lowered == "na" || lowered == "none" {
        return true;
    }
    lowered.starts_with("<value")
}

fn default_memories_template() -> String {
    [
        "[context:likes]",
        "",
        "[context:dislikes]",
        "",
        "[context:location]",
        "",
        "[context:timezone]",
        "",
        "[context:tools]",
        "",
        "[context:projects]",
        "",
        "[context:topics]",
        "",
    ]
    .join("\n")
}
