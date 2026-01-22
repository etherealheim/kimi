use crate::app::chat::agent::intent::QueryIntent;
use crate::services::{dates, obsidian};
use color_eyre::Result;
use std::path::Path;

const MAX_OBSIDIAN_CONTEXT_CHARS: usize = 8000;

pub struct ObsidianContext {
    pub content: String,
    pub count: usize,
}

pub struct ObsidianContextRequest<'a> {
    pub vault_path: &'a str,
    pub query: &'a str,
    pub intent: QueryIntent,
}

#[derive(Debug, Clone, Copy)]
enum ObsidianAction {
    WeeklyNotes {
        include_checklist: bool,
        week: dates::IsoWeek,
    },
    DailyNotesRange {
        range: dates::DateRange,
    },
    NoteSearch,
}

pub fn build_obsidian_context(
    request: ObsidianContextRequest<'_>,
) -> Result<Option<ObsidianContext>> {
    let vault_path = request.vault_path.trim();
    let lowered = request.query.to_lowercase();
    let intent = request.intent;
    if vault_path.is_empty() {
        if intent.is_note_lookup {
            return Ok(Some(ObsidianContext {
                content: "--- Obsidian notes ---\nObsidian vault path is not configured.".to_string(),
                count: 0,
            }));
        }
        return Ok(None);
    }
    if !Path::new(vault_path).is_dir() {
        if intent.is_note_lookup {
            return Ok(Some(ObsidianContext {
                content: format!(
                    "--- Obsidian notes ---\nObsidian vault path is not accessible: {}.",
                    vault_path
                ),
                count: 0,
            }));
        }
        return Ok(None);
    }
    let Some(action) = select_obsidian_action(intent, &lowered) else {
        return Ok(None);
    };
    match action {
        ObsidianAction::WeeklyNotes {
            include_checklist,
            week,
        } => {
            let notes = obsidian::week_notes_context(vault_path, week)?;
            let count = notes.len();
            let mut blocks = Vec::new();
            if let Some(content) =
                obsidian::format_obsidian_context("Obsidian weekly notes", &notes)
            {
                blocks.push(clamp_context_chars(&content, MAX_OBSIDIAN_CONTEXT_CHARS));
            } else {
                blocks.push("--- Obsidian weekly notes ---".to_string());
                blocks.push(format!(
                    "No weekly notes found for {}-W{:02}.",
                    week.year,
                    week.week
                ));
            }
            if include_checklist {
                let checklist = obsidian::week_note_checklist(vault_path, week)?;
                blocks.push("--- Weekly checklist ---".to_string());
                if checklist.is_empty() {
                    blocks.push("No checklist items found in the weekly note.".to_string());
                } else {
                    blocks.extend(checklist);
                }
            }
            Ok(Some(ObsidianContext {
                content: blocks.join("\n"),
                count,
            }))
        }
        ObsidianAction::DailyNotesRange { range } => {
            let notes = obsidian::daily_notes_context(vault_path, range)?;
            if let Some(content) = obsidian::format_obsidian_context("Obsidian daily notes", &notes)
            {
                let count = notes.len();
                let content = clamp_context_chars(&content, MAX_OBSIDIAN_CONTEXT_CHARS);
                return Ok(Some(ObsidianContext { content, count }));
            }
            if intent.is_note_lookup {
                let content = format!(
                    "--- Obsidian daily notes ---\nNo daily notes found for {} to {}.",
                    range.start.format("%Y-%m-%d"),
                    range.end.format("%Y-%m-%d")
                );
                return Ok(Some(ObsidianContext { content, count: 0 }));
            }
            Ok(None)
        }
        ObsidianAction::NoteSearch => {
            let notes = obsidian::search_notes(vault_path, request.query, 8)?;
            if let Some(content) = obsidian::format_obsidian_context("Obsidian notes", &notes) {
                let count = notes.len();
                let content = clamp_context_chars(&content, MAX_OBSIDIAN_CONTEXT_CHARS);
                return Ok(Some(ObsidianContext { content, count }));
            }
            if intent.is_note_lookup {
                let content = format!(
                    "--- Obsidian notes ---\nNo matching notes found for \"{}\".",
                    request.query.trim()
                );
                return Ok(Some(ObsidianContext { content, count: 0 }));
            }
            Ok(None)
        }
    }
}

fn is_checklist_query(lowered: &str) -> bool {
    let triggers = ["checklist", "todo", "to-do", "tasks", "task list"];
    triggers.iter().any(|term| lowered.contains(term))
}

fn select_obsidian_action(intent: QueryIntent, lowered: &str) -> Option<ObsidianAction> {
    if intent.is_external_event || intent.is_note_creation {
        return None;
    }
    let reference = dates::parse_date_reference(lowered);
    if intent.is_personal_recap || intent.is_week_note {
        if let Some(dates::DateReference::Week(week)) = reference {
            return Some(ObsidianAction::WeeklyNotes {
                include_checklist: is_checklist_query(lowered),
                week,
            });
        }
        if let Some(reference) = reference {
            if let Some(range) = reference.as_range() {
                return Some(ObsidianAction::DailyNotesRange { range });
            }
        }
        return Some(ObsidianAction::WeeklyNotes {
            include_checklist: is_checklist_query(lowered),
            week: dates::resolve_query_week(lowered),
        });
    }
    if intent.is_note_lookup {
        if let Some(dates::DateReference::Week(week)) = reference {
            return Some(ObsidianAction::WeeklyNotes {
                include_checklist: is_checklist_query(lowered),
                week,
            });
        }
        if let Some(reference) = reference {
            if let Some(range) = reference.as_range() {
                return Some(ObsidianAction::DailyNotesRange { range });
            }
        }
        return Some(ObsidianAction::NoteSearch);
    }
    if should_fallback_to_note_search(lowered) {
        return Some(ObsidianAction::NoteSearch);
    }
    None
}

fn should_fallback_to_note_search(lowered: &str) -> bool {
    let trimmed = lowered.trim();
    if trimmed.is_empty() {
        return false;
    }
    let word_count = trimmed.split_whitespace().count();
    if word_count > 6 {
        return false;
    }
    let has_time_reference = ["today", "now", "current", "latest", "happening", "news"]
        .iter()
        .any(|term| trimmed.contains(term));
    if has_time_reference {
        return false;
    }
    let has_code_indicators = ["function", "class", "variable", "import", "export", "def ", "fn "]
        .iter()
        .any(|term| trimmed.contains(term));
    if has_code_indicators {
        return false;
    }
    let has_personal_question = [
        "who are you",
        "what are you",
        "who is kimi",
        "what is kimi",
        "your name",
        "introduce yourself",
    ]
    .iter()
    .any(|term| trimmed.contains(term));
    if has_personal_question {
        return false;
    }
    if word_count <= 4 {
        return true;
    }
    false
}

pub fn should_fetch_obsidian_for_intent(
    vault_path: &str,
    query: &str,
    intent: QueryIntent,
) -> bool {
    let vault_path = vault_path.trim();
    if vault_path.is_empty() {
        return false;
    }
    let lowered = query.to_lowercase();
    select_obsidian_action(intent, &lowered).is_some()
}

fn clamp_context_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    value.chars().take(max_chars).collect()
}
