use crate::app::chat::agent::intent::QueryIntent;
use crate::services::{dates, obsidian};
use color_eyre::Result;

const MAX_OBSIDIAN_CONTEXT_CHARS: usize = 32000;

pub struct ObsidianContext {
    pub content: String,
    pub count: usize,
    pub raw_notes: Vec<crate::services::obsidian::NoteSnippet>,
}

pub struct ObsidianContextRequest<'a> {
    pub vault_name: &'a str,
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
    let vault_name = request.vault_name.trim();
    let lowered = request.query.to_lowercase();
    let intent = request.intent;
    if vault_name.is_empty() {
        if intent.is_note_lookup {
            return Ok(Some(ObsidianContext {
                content: "--- Obsidian notes ---\nObsidian vault is not configured.".to_string(),
                count: 0,
                raw_notes: vec![],
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
            let week_query = format!("{}-W{:02}", week.year, week.week);
            let notes = obsidian::search_notes(vault_name, &week_query, 10)?;
            let count = notes.len();
            let raw_notes = notes.clone();
            let mut blocks = Vec::new();
            if let Some(content) =
                obsidian::format_obsidian_context("Obsidian weekly notes", &notes)
            {
                blocks.push(clamp_context_chars(&content, MAX_OBSIDIAN_CONTEXT_CHARS));
            } else {
                blocks.push("--- Obsidian weekly notes ---".to_string());
                blocks.push(format!("No weekly notes found for {}.", week_query));
            }
            if include_checklist {
                blocks.push("--- Weekly checklist ---".to_string());
                match obsidian::read_note(vault_name, &week_query) {
                    Ok(content) => {
                        let checklist = obsidian::extract_checklist_items(&content);
                        if checklist.is_empty() {
                            blocks.push(
                                "No checklist items found in the weekly note.".to_string(),
                            );
                        } else {
                            blocks.extend(checklist);
                        }
                    }
                    Err(_) => {
                        blocks.push("No checklist items found in the weekly note.".to_string());
                    }
                }
            }
            Ok(Some(ObsidianContext {
                content: blocks.join("\n"),
                count,
                raw_notes,
            }))
        }
        ObsidianAction::DailyNotesRange { range } => {
            let mut notes = Vec::new();
            let mut current = range.start;
            while current <= range.end {
                let date_str = current.format("%Y-%m-%d").to_string();
                if let Ok(content) = obsidian::read_note(vault_name, &date_str) {
                    let trimmed = content.trim();
                    if !trimmed.is_empty() {
                        notes.push(obsidian::NoteSnippet {
                            title: date_str,
                            note_type: obsidian::NoteType::Daily,
                            snippet: trimmed.to_string(),
                        });
                    }
                }
                current += chrono::Duration::days(1);
            }
            if let Some(content) =
                obsidian::format_obsidian_context("Obsidian daily notes", &notes)
            {
                let count = notes.len();
                let raw_notes = notes.clone();
                let content = clamp_context_chars(&content, MAX_OBSIDIAN_CONTEXT_CHARS);
                return Ok(Some(ObsidianContext {
                    content,
                    count,
                    raw_notes,
                }));
            }
            if intent.is_note_lookup {
                let content = format!(
                    "--- Obsidian daily notes ---\nNo daily notes found for {} to {}.",
                    range.start.format("%Y-%m-%d"),
                    range.end.format("%Y-%m-%d")
                );
                return Ok(Some(ObsidianContext {
                    content,
                    count: 0,
                    raw_notes: vec![],
                }));
            }
            Ok(None)
        }
        ObsidianAction::NoteSearch => {
            let notes = obsidian::search_notes(vault_name, request.query, 8)?;
            if let Some(content) = obsidian::format_obsidian_context("Obsidian notes", &notes) {
                let count = notes.len();
                let raw_notes = notes.clone();
                let content = clamp_context_chars(&content, MAX_OBSIDIAN_CONTEXT_CHARS);
                return Ok(Some(ObsidianContext {
                    content,
                    count,
                    raw_notes,
                }));
            }
            if intent.is_note_lookup {
                let content = format!(
                    "--- Obsidian notes ---\nNo matching notes found for \"{}\".",
                    request.query.trim()
                );
                return Ok(Some(ObsidianContext {
                    content,
                    count: 0,
                    raw_notes: vec![],
                }));
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
        if let Some(reference) = reference
            && let Some(range) = reference.as_range()
        {
            return Some(ObsidianAction::DailyNotesRange { range });
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
        if let Some(reference) = reference
            && let Some(range) = reference.as_range()
        {
            return Some(ObsidianAction::DailyNotesRange { range });
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
    if !(2..=6).contains(&word_count) {
        return false;
    }
    // Conversational phrases should never trigger note search
    let is_conversational = [
        "hi", "hello", "hey", "yo", "sup", "thanks", "thank you", "bye", "goodbye",
        "good morning", "good night", "good evening", "good afternoon",
        "nice to meet", "how are you", "what's up", "whats up",
        "i am", "i'm", "my name", "that's", "thats", "okay", "ok",
        "yes", "no", "sure", "yeah", "nah", "please", "sorry",
        "cool", "great", "awesome", "nice", "wow", "lol", "haha",
        "help me", "can you", "could you", "would you", "tell me",
        "i think", "i feel", "i want", "i need", "i like", "i love",
        "do you", "are you", "is it", "is that", "was it",
        "let's", "lets", "why", "how", "when", "where", "what if",
    ]
    .iter()
    .any(|term| trimmed.starts_with(term) || trimmed == *term);
    if is_conversational {
        return false;
    }
    let has_time_reference = ["today", "now", "current", "latest", "happening", "news"]
        .iter()
        .any(|term| trimmed.contains(term));
    if has_time_reference {
        return false;
    }
    let has_code_indicators =
        ["function", "class", "variable", "import", "export", "def ", "fn "]
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
    // Only fallback for very short queries that look like topic lookups (e.g. "rust", "swift")
    if word_count <= 4 {
        return true;
    }
    false
}

pub fn should_fetch_obsidian_for_intent(
    vault_name: &str,
    query: &str,
    intent: QueryIntent,
) -> bool {
    let vault_name = vault_name.trim();
    if vault_name.is_empty() {
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
