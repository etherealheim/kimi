use crate::agents::{AgentManager, ChatMessage as AgentChatMessage};
use color_eyre::{Result, eyre::eyre};
use std::fs;
use std::path::{Path, PathBuf};

// ── Data types ──────────────────────────────────────────────────────────────

/// Summary of a project (name + entry count) for list views
#[derive(Debug, Clone)]
pub struct ProjectSummary {
    pub name: String,
    pub description: String,
    pub entry_count: usize,
}

/// Full parsed project file
#[derive(Debug, Clone)]
pub struct ProjectFile {
    pub name: String,
    pub description: String,
    pub entries: Vec<String>,
}

/// Result of LLM entry extraction for one project
#[derive(Debug, Clone)]
pub struct ProjectExtractionResult {
    pub project_name: String,
    pub entries: Vec<String>,
}

// ── Path safety ─────────────────────────────────────────────────────────────

/// Sanitizes a project name for use as a filename (removes path separators, etc.)
fn sanitize_project_name(name: &str) -> String {
    name.chars()
        .filter(|character| !matches!(character, '/' | '\\' | '\0'))
        .collect::<String>()
        .trim()
        .to_string()
}

/// Returns the safe path to a project file, rejecting path traversal attempts
fn project_file_path(vault_path: &str, name: &str) -> Result<PathBuf> {
    let sanitized = sanitize_project_name(name);
    if sanitized.is_empty() {
        return Err(eyre!("Project name cannot be empty"));
    }
    let projects_dir = PathBuf::from(vault_path).join("projects");
    let path = projects_dir.join(format!("{}.md", sanitized));

    // For new files that don't exist yet, verify the parent resolves inside projects/
    if let Ok(canonical_parent) = projects_dir.canonicalize() {
        let resolved = if path.exists() {
            path.canonicalize()?
        } else {
            canonical_parent.join(format!("{}.md", sanitized))
        };
        if !resolved.starts_with(&canonical_parent) {
            return Err(eyre!("Invalid project path: attempted path traversal"));
        }
    }
    Ok(path)
}

fn archived_file_path(vault_path: &str, name: &str) -> Result<PathBuf> {
    let sanitized = sanitize_project_name(name);
    if sanitized.is_empty() {
        return Err(eyre!("Project name cannot be empty"));
    }
    Ok(PathBuf::from(vault_path)
        .join("projects")
        .join("archived")
        .join(format!("{}.md", sanitized)))
}

fn ensure_projects_dir(vault_path: &str) -> Result<()> {
    let dir = PathBuf::from(vault_path).join("projects");
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(())
}

fn ensure_archived_dir(vault_path: &str) -> Result<()> {
    let dir = PathBuf::from(vault_path).join("projects").join("archived");
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(())
}

// ── CRUD operations ─────────────────────────────────────────────────────────

/// Creates a new project markdown file. Fails if the file already exists (never overwrites).
pub fn create_project_file(vault_path: &str, name: &str, description: &str) -> Result<()> {
    if vault_path.is_empty() {
        return Err(eyre!("Obsidian vault path not configured"));
    }
    ensure_projects_dir(vault_path)?;
    let path = project_file_path(vault_path, name)?;

    if path.exists() {
        return Err(eyre!("Project '{}' already exists", name));
    }

    let content = format!("# {}\n\n> {}\n\n## Entries\n", name.trim(), description.trim());
    fs::write(&path, content)?;
    Ok(())
}

/// Appends new entries to an existing project file (append-only, never overwrites).
pub fn append_project_entries(vault_path: &str, name: &str, entries: &[String]) -> Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    let path = project_file_path(vault_path, name)?;
    if !path.exists() {
        return Err(eyre!("Project '{}' does not exist", name));
    }

    // Read current content
    let current = fs::read_to_string(&path)?;

    // Build new entry lines
    let new_lines: Vec<String> = entries
        .iter()
        .map(|entry| format!("- {}", entry.trim()))
        .collect();

    // Append after the ## Entries section (or at the end if not found)
    let updated = if current.contains("## Entries") {
        format!("{}\n{}\n", current.trim_end(), new_lines.join("\n"))
    } else {
        format!("{}\n\n## Entries\n{}\n", current.trim_end(), new_lines.join("\n"))
    };

    fs::write(&path, updated)?;
    Ok(())
}

/// Reads and parses a project file into structured data
pub fn read_project_file(vault_path: &str, name: &str) -> Result<ProjectFile> {
    let path = project_file_path(vault_path, name)?;
    if !path.exists() {
        return Err(eyre!("Project '{}' does not exist", name));
    }
    let content = fs::read_to_string(&path)?;
    parse_project_markdown(&content, name)
}

/// Lists all projects in the vault's projects/ directory
pub fn list_projects(vault_path: &str) -> Result<Vec<ProjectSummary>> {
    if vault_path.is_empty() {
        return Ok(Vec::new());
    }
    let projects_dir = PathBuf::from(vault_path).join("projects");
    if !projects_dir.exists() {
        return Ok(Vec::new());
    }

    let mut summaries = Vec::new();
    let entries = fs::read_dir(&projects_dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !is_markdown_file(&path) {
            continue;
        }
        // Skip the archived subdirectory
        if path.is_dir() {
            continue;
        }
        let file_name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Unknown")
            .to_string();

        if let Ok(content) = fs::read_to_string(&path) {
            let parsed = parse_project_markdown(&content, &file_name)
                .unwrap_or_else(|_| ProjectFile {
                    name: file_name.clone(),
                    description: String::new(),
                    entries: Vec::new(),
                });
            summaries.push(ProjectSummary {
                name: parsed.name,
                description: parsed.description,
                entry_count: parsed.entries.len(),
            });
        }
    }

    summaries.sort_by(|first, second| first.name.cmp(&second.name));
    Ok(summaries)
}

/// Archives a project by moving it to projects/archived/ (never truly deletes)
pub fn archive_project(vault_path: &str, name: &str) -> Result<()> {
    let source = project_file_path(vault_path, name)?;
    if !source.exists() {
        return Err(eyre!("Project '{}' does not exist", name));
    }

    ensure_archived_dir(vault_path)?;
    let destination = archived_file_path(vault_path, name)?;

    // If there's already an archived version, add a timestamp suffix
    let final_destination = if destination.exists() {
        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let sanitized = sanitize_project_name(name);
        PathBuf::from(vault_path)
            .join("projects")
            .join("archived")
            .join(format!("{}-{}.md", sanitized, timestamp))
    } else {
        destination
    };

    fs::rename(&source, &final_destination)?;
    Ok(())
}

/// Searches all project entries for matching text (case-insensitive substring match)
pub fn search_project_entries(
    vault_path: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<(String, String)>> {
    let projects = list_projects(vault_path)?;
    let query_lower = query.to_lowercase();
    let query_words: Vec<&str> = query_lower.split_whitespace().collect();

    let mut results = Vec::new();
    for summary in &projects {
        let project_file = read_project_file(vault_path, &summary.name)?;

        // Check if the project name matches
        let name_matches = query_words.iter().any(|word| {
            summary.name.to_lowercase().contains(word)
        });

        if name_matches {
            // Return all entries for matching projects
            for entry in &project_file.entries {
                results.push((summary.name.clone(), entry.clone()));
                if results.len() >= limit {
                    return Ok(results);
                }
            }
        } else {
            // Search individual entries
            for entry in &project_file.entries {
                let entry_lower = entry.to_lowercase();
                let matches = query_words.iter().any(|word| entry_lower.contains(word));
                if matches {
                    results.push((summary.name.clone(), entry.clone()));
                    if results.len() >= limit {
                        return Ok(results);
                    }
                }
            }
        }
    }

    Ok(results)
}

/// Returns a list of project names (lightweight, for LLM prompts)
pub fn list_project_names(vault_path: &str) -> Result<Vec<String>> {
    let summaries = list_projects(vault_path)?;
    Ok(summaries.into_iter().map(|summary| summary.name).collect())
}

// ── Markdown parsing ────────────────────────────────────────────────────────

fn parse_project_markdown(content: &str, fallback_name: &str) -> Result<ProjectFile> {
    let mut name = fallback_name.to_string();
    let mut description = String::new();
    let mut entries = Vec::new();
    let mut in_entries_section = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Parse title: # Name
        if let Some(title) = trimmed.strip_prefix("# ") {
            name = title.trim().to_string();
            continue;
        }

        // Parse description: > blockquote
        if let Some(desc) = trimmed.strip_prefix("> ") {
            if description.is_empty() {
                description = desc.trim().to_string();
            }
            continue;
        }

        // Detect ## Entries section
        if trimmed == "## Entries" {
            in_entries_section = true;
            continue;
        }

        // New section ends entries
        if trimmed.starts_with("## ") && in_entries_section {
            in_entries_section = false;
            continue;
        }

        // Parse entry lines
        if in_entries_section
            && let Some(entry_text) = trimmed.strip_prefix("- ")
        {
            let entry = entry_text.trim().to_string();
            if !entry.is_empty() {
                entries.push(entry);
            }
        }
    }

    Ok(ProjectFile {
        name,
        description,
        entries,
    })
}

fn is_markdown_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
}

// ── LLM extraction logic ────────────────────────────────────────────────────

/// Extracts 1-3 topic keywords from a conversation (lightweight LLM call)
pub fn extract_topics(
    conversation_content: &str,
    agent: &crate::agents::Agent,
    manager: &AgentManager,
) -> Vec<String> {
    let truncated: String = conversation_content.chars().take(1000).collect();
    let prompt = format!(
        "What are the main topics discussed? Return a JSON array of 1-3 topic names, lowercase. \
Only include substantive topics (not greetings, emotions, small talk, or meta-conversation). \
Example: [\"gardening\", \"rust programming\"]. If no substantive topics, return [].\n\n\
Conversation:\n{}",
        truncated
    );

    let messages = vec![
        AgentChatMessage::system(
            "You extract topic keywords from conversations. Return only a JSON array of strings.",
        ),
        AgentChatMessage::user(&prompt),
    ];

    let response = match manager.chat(agent, &messages) {
        Ok(text) => text,
        Err(_) => return Vec::new(),
    };

    parse_topic_json(&response)
}

/// Extracts factual entries from a conversation for matching projects
pub fn extract_entries_for_projects(
    conversation_content: &str,
    project_names: &[String],
    agent: &crate::agents::Agent,
    manager: &AgentManager,
) -> Vec<ProjectExtractionResult> {
    if project_names.is_empty() {
        return Vec::new();
    }

    let truncated: String = conversation_content.chars().take(2000).collect();
    let names_list = project_names.join(", ");
    let prompt = format!(
        "Extract factual information from this conversation that belongs to these projects: {}\n\
Return JSON in this format:\n\
{{\"projects\":[{{\"name\":\"ProjectName\",\"entries\":[\"fact 1\",\"fact 2\"]}}]}}\n\
Only extract concrete facts, preferences, or knowledge — not opinions, greetings, or questions.\n\
If no relevant facts, return {{\"projects\":[]}}.\n\n\
Conversation:\n{}",
        names_list, truncated
    );

    let messages = vec![
        AgentChatMessage::system(
            "You extract factual entries from conversations for knowledge projects. Return only JSON.",
        ),
        AgentChatMessage::user(&prompt),
    ];

    let response = match manager.chat(agent, &messages) {
        Ok(text) => text,
        Err(_) => return Vec::new(),
    };

    parse_extraction_json(&response)
}

fn parse_topic_json(response: &str) -> Vec<String> {
    let trimmed = response.trim();
    let start = trimmed.find('[');
    let end = trimmed.rfind(']');

    if let (Some(start_idx), Some(end_idx)) = (start, end)
        && start_idx < end_idx
    {
        let json_slice = &trimmed[start_idx..=end_idx];
        if let Ok(topics) = serde_json::from_str::<Vec<String>>(json_slice) {
            return topics
                .into_iter()
                .map(|topic| topic.to_lowercase().trim().to_string())
                .filter(|topic| !topic.is_empty())
                .collect();
        }
    }
    Vec::new()
}

fn parse_extraction_json(response: &str) -> Vec<ProjectExtractionResult> {
    let trimmed = response.trim();
    let start = trimmed.find('{');
    let end = trimmed.rfind('}');

    if let (Some(start_idx), Some(end_idx)) = (start, end)
        && start_idx < end_idx
    {
        let json_slice = &trimmed[start_idx..=end_idx];
        if let Ok(parsed) = serde_json::from_str::<ExtractionResponse>(json_slice) {
            return parsed
                .projects
                .into_iter()
                .filter(|project| !project.entries.is_empty())
                .map(|project| ProjectExtractionResult {
                    project_name: project.name,
                    entries: project.entries,
                })
                .collect();
        }
    }
    Vec::new()
}

#[derive(serde::Deserialize)]
struct ExtractionResponse {
    projects: Vec<ExtractionProject>,
}

#[derive(serde::Deserialize)]
struct ExtractionProject {
    name: String,
    entries: Vec<String>,
}

/// Formats search results for tool output
pub fn format_search_results(results: &[(String, String)], project_names: &[String]) -> String {
    if results.is_empty() && project_names.is_empty() {
        return "No projects found.".to_string();
    }

    let mut output = String::new();

    if !project_names.is_empty() {
        output.push_str(&format!("Active projects: {}\n", project_names.join(", ")));
    }

    if results.is_empty() {
        output.push_str("No matching entries found.");
        return output;
    }

    // Group results by project
    let mut current_project = String::new();
    for (project_name, entry) in results {
        if *project_name != current_project {
            output.push_str(&format!("\n--- {} ---\n", project_name));
            current_project = project_name.clone();
        }
        output.push_str(&format!("- {}\n", entry));
    }

    output
}
