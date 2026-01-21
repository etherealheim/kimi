use chrono::NaiveDate;
use color_eyre::Result;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_SNIPPET_LINES: usize = 6;
const MAX_DETAIL_LINES: usize = 80;
const MAX_WEEK_NOTE_LINES: usize = 12;

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

pub fn search_notes(vault_path: &str, query: &str, limit: usize) -> Result<Vec<NoteSnippet>> {
    let vault = Path::new(vault_path);
    if !vault.is_dir() {
        return Ok(Vec::new());
    }
    let tokens = tokenize_query(query);
    if tokens.is_empty() {
        return Ok(Vec::new());
    }

    let mut scored = Vec::new();
    let files = collect_markdown_files(vault)?;
    let wants_details = query_wants_details(query);
    for path in files {
        let Some(file_name) = path.file_stem().and_then(|name| name.to_str()) else {
            continue;
        };
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let score = score_content(&content, file_name, &tokens);
        if score == 0 {
            continue;
        }
        let max_lines = if wants_details || title_matches_tokens(file_name, &tokens) {
            MAX_DETAIL_LINES
        } else {
            MAX_SNIPPET_LINES
        };
        let snippet = extract_snippet(&content, &tokens, max_lines);
        scored.push((
            score,
            NoteSnippet {
                title: file_name.to_string(),
                note_type: classify_note_type(file_name),
                snippet,
            },
        ));
    }

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(scored
        .into_iter()
        .take(limit)
        .map(|(_, snippet)| snippet)
        .collect())
}

/// Fetches weekly note + daily notes for a given ISO week
pub fn week_notes_context(
    vault_path: &str,
    week: crate::services::dates::IsoWeek,
) -> Result<Vec<NoteSnippet>> {
    let vault = Path::new(vault_path);
    if !vault.is_dir() {
        return Ok(Vec::new());
    }
    let Some(range) = week.date_range() else {
        return Ok(Vec::new());
    };
    let files = collect_markdown_files(vault)?;
    let mut snippets = Vec::new();

    for path in files {
        let Some(stem) = path.file_stem().and_then(|name| name.to_str()) else {
            continue;
        };
        let note_type = classify_note_type(stem);
        match note_type {
            NoteType::Daily => {
                if let Some(date) = parse_daily_date(stem) {
                    if !crate::services::dates::date_in_range(date, range) {
                        continue;
                    }
                } else {
                    continue;
                }
            }
            NoteType::Weekly => {
                if let Some((year, parsed_week)) = parse_weekly_date(stem) {
                    if year != week.year || parsed_week != week.week {
                        continue;
                    }
                } else {
                    continue;
                }
            }
            NoteType::General => continue,
        }

        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let max_lines = match note_type {
            NoteType::Weekly => MAX_DETAIL_LINES,
            NoteType::Daily => MAX_WEEK_NOTE_LINES,
            NoteType::General => MAX_WEEK_NOTE_LINES,
        };
        let snippet = extract_first_lines(&content, max_lines);
        snippets.push(NoteSnippet {
            title: stem.to_string(),
            note_type,
            snippet,
        });
    }

    Ok(snippets)
}

pub fn week_note_checklist(
    vault_path: &str,
    week: crate::services::dates::IsoWeek,
) -> Result<Vec<String>> {
    let vault = Path::new(vault_path);
    if !vault.is_dir() {
        return Ok(Vec::new());
    }
    let files = collect_markdown_files(vault)?;
    for path in files {
        let Some(stem) = path.file_stem().and_then(|name| name.to_str()) else {
            continue;
        };
        if let Some((year, parsed_week)) = parse_weekly_date(stem) {
            if year == week.year && parsed_week == week.week {
                let Ok(content) = fs::read_to_string(&path) else {
                    return Ok(Vec::new());
                };
                return Ok(extract_checklist_items(&content));
            }
        }
    }
    Ok(Vec::new())
}

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

fn note_type_label(note_type: NoteType) -> &'static str {
    match note_type {
        NoteType::Daily => "daily note",
        NoteType::Weekly => "weekly note",
        NoteType::General => "note",
    }
}

fn collect_markdown_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("md"))
                .unwrap_or(false)
            {
                files.push(path);
            }
        }
    }
    Ok(files)
}

fn classify_note_type(stem: &str) -> NoteType {
    if parse_daily_date(stem).is_some() {
        return NoteType::Daily;
    }
    if parse_weekly_date(stem).is_some() {
        return NoteType::Weekly;
    }
    NoteType::General
}

fn parse_daily_date(stem: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(stem, "%Y-%m-%d").ok()
}

fn parse_weekly_date(stem: &str) -> Option<(i32, u32)> {
    // Support both YYYY-W4 and YYYY-W04 formats (case-insensitive)
    let lowered = stem.to_lowercase();
    let parts: Vec<&str> = lowered.split("-w").collect();
    if parts.len() != 2 {
        return None;
    }
    let year = parts[0].parse::<i32>().ok()?;
    let week_str = parts[1].trim_start_matches('0');
    let week = if week_str.is_empty() {
        0
    } else {
        week_str.parse::<u32>().ok()?
    };
    // Validate week range (1-53)
    if week < 1 || week > 53 {
        return None;
    }
    Some((year, week))
}

fn tokenize_query(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut seen = HashSet::new();
    for raw in query.split_whitespace() {
        let cleaned = raw
            .trim_matches(|character: char| !character.is_alphanumeric() && character != '-')
            .to_lowercase();
        if cleaned.len() < 2 {
            continue;
        }
        if seen.insert(cleaned.clone()) {
            tokens.push(cleaned);
        }
    }
    tokens
}

fn title_matches_tokens(title: &str, tokens: &[String]) -> bool {
    let lowered = title.to_lowercase();
    tokens.iter().any(|token| lowered.contains(token))
}

fn query_wants_details(query: &str) -> bool {
    let lowered = query.to_lowercase();
    let triggers = [
        "all",
        "everything",
        "full",
        "details",
        "show me",
        "bring that",
        "what i have",
        "what i have in notes",
        "list",
    ];
    triggers.iter().any(|term| lowered.contains(term))
}

fn score_content(content: &str, title: &str, tokens: &[String]) -> usize {
    let lowered = content.to_lowercase();
    let title_lowered = title.to_lowercase();
    let mut score = 0usize;
    for token in tokens {
        let occurrences = lowered.matches(token).count();
        if occurrences > 0 {
            score += occurrences * 2;
        }
        if title_lowered.contains(token) {
            score += 3;
        }
    }
    score
}

fn extract_snippet(content: &str, tokens: &[String], max_lines: usize) -> String {
    let lowered = content.to_lowercase();
    let lines: Vec<&str> = content.lines().collect();
    let mut best_line = None;
    for (index, line) in lines.iter().enumerate() {
        let line_lower = line.to_lowercase();
        if tokens.iter().any(|token| line_lower.contains(token)) {
            best_line = Some(index);
            break;
        }
    }
    if let Some(index) = best_line {
        let start = index.saturating_sub(1);
        let end = (index + max_lines).min(lines.len());
        return lines[start..end].join("\n");
    }
    if lowered.trim().is_empty() {
        return String::new();
    }
    extract_first_lines(content, max_lines)
}

fn extract_first_lines(content: &str, max_lines: usize) -> String {
    content
        .lines()
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_checklist_items(content: &str) -> Vec<String> {
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
