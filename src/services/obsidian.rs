use color_eyre::Result;
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Clone, Copy)]
pub enum NoteType {
    Daily,
    Weekly,
    General,
}

#[derive(Debug, Clone)]
pub struct NoteSnippet {
    pub title: String,
    pub note_type: NoteType,
    pub snippet: String,
}

/// JSON shape returned by `obsidian search ... matches format=json`
#[derive(Debug, Deserialize)]
struct SearchResult {
    file: String,
    matches: Vec<SearchMatch>,
}

#[derive(Debug, Deserialize)]
struct SearchMatch {
    #[allow(dead_code)]
    line: u64,
    text: String,
}

/// Execute an Obsidian CLI command and return stdout
fn run_cli(vault_name: &str, args: &[&str]) -> Result<String> {
    let mut command = Command::new("obsidian");
    if !vault_name.is_empty() {
        command.arg(format!("vault={}", vault_name));
    }
    for arg in args {
        command.arg(arg);
    }
    let output = command.output().map_err(|error| {
        color_eyre::eyre::eyre!("Failed to run obsidian CLI: {}", error)
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // CLI returns exit 0 even for errors, check for "Error:" prefix in stdout
    if stdout.starts_with("Error:") {
        return Err(color_eyre::eyre::eyre!(
            "Obsidian CLI error: {}",
            stdout.trim()
        ));
    }
    if !stderr.is_empty() && !output.status.success() {
        return Err(color_eyre::eyre::eyre!(
            "Obsidian CLI failed: {}",
            stderr.trim()
        ));
    }
    Ok(stdout)
}

/// Search notes using Obsidian's built-in search engine
pub fn search_notes(vault_name: &str, query: &str, limit: usize) -> Result<Vec<NoteSnippet>> {
    let limit_str = limit.to_string();
    let query_arg = format!("query={}", query);
    let limit_arg = format!("limit={}", limit_str);
    let output = run_cli(
        vault_name,
        &["search", &query_arg, &limit_arg, "matches", "format=json"],
    )?;

    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let results: Vec<SearchResult> = serde_json::from_str(trimmed).map_err(|error| {
        color_eyre::eyre::eyre!("Failed to parse search results: {}", error)
    })?;

    let snippets = results
        .into_iter()
        .map(|result| {
            let title = derive_title(&result.file);
            let note_type = classify_note_type(&title);
            let snippet = result
                .matches
                .iter()
                .map(|matched| matched.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            NoteSnippet {
                title,
                note_type,
                snippet,
            }
        })
        .collect();

    Ok(snippets)
}

/// Read a specific note by file name (fuzzy) or path (exact)
pub fn read_note(vault_name: &str, file_or_path: &str) -> Result<String> {
    let arg = format!("file={}", file_or_path);
    let output = run_cli(vault_name, &["read", &arg])?;
    Ok(output)
}

/// Read today's daily note
#[allow(dead_code)]
pub fn read_daily_note(vault_name: &str) -> Result<String> {
    let output = run_cli(vault_name, &["daily:read"])?;
    Ok(output)
}

/// Format note snippets into a context block for the LLM
pub fn format_obsidian_context(label: &str, notes: &[NoteSnippet]) -> Option<String> {
    if notes.is_empty() {
        return None;
    }
    let mut blocks = Vec::new();
    blocks.push(format!("--- {} ---", label));
    for note in notes {
        blocks.push(format!("{} ({})", note.title, note_type_label(note.note_type)));
        if !note.snippet.is_empty() {
            blocks.push(note.snippet.clone());
        }
    }
    Some(blocks.join("\n"))
}

/// Extract checklist items (- [ ] / - [x]) from raw note content
pub fn extract_checklist_items(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with("- [") {
                Some(trimmed.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn note_type_label(note_type: NoteType) -> &'static str {
    match note_type {
        NoteType::Daily => "daily note",
        NoteType::Weekly => "weekly note",
        NoteType::General => "note",
    }
}

/// Derive a clean title from a file path like "Journal/Daily/2026-02-10.md"
fn derive_title(file_path: &str) -> String {
    std::path::Path::new(file_path)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(file_path)
        .to_string()
}

/// Classify a note title as Daily, Weekly, or General based on naming patterns
fn classify_note_type(title: &str) -> NoteType {
    // Daily: YYYY-MM-DD pattern
    if title.len() == 10
        && title.chars().nth(4) == Some('-')
        && title.chars().nth(7) == Some('-')
        && title.chars().take(4).all(|character| character.is_ascii_digit())
        && title.chars().skip(5).take(2).all(|character| character.is_ascii_digit())
        && title.chars().skip(8).take(2).all(|character| character.is_ascii_digit())
    {
        return NoteType::Daily;
    }
    // Weekly: YYYY-W## pattern (case insensitive)
    let lowered = title.to_lowercase();
    if lowered.contains("-w") {
        let parts: Vec<&str> = lowered.split("-w").collect();
        if parts.len() == 2
            && parts.first().is_some_and(|year| year.len() == 4 && year.chars().all(|character| character.is_ascii_digit()))
            && parts.get(1).is_some_and(|week| !week.is_empty() && week.chars().all(|character| character.is_ascii_digit()))
        {
            return NoteType::Weekly;
        }
    }
    NoteType::General
}
