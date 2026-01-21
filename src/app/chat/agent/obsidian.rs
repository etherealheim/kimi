use crate::services::{dates, obsidian};
use color_eyre::Result;

const MAX_OBSIDIAN_CONTEXT_CHARS: usize = 4000;

pub struct ObsidianContext {
    pub content: String,
    pub count: usize,
}

pub fn build_obsidian_context(
    vault_path: &str,
    query: &str,
    _query_tokens: &[String],
) -> Result<Option<ObsidianContext>> {
    let vault_path = vault_path.trim();
    if vault_path.is_empty() {
        return Ok(None);
    }
    let lowered = query.to_lowercase();
    // External events (news, happening, etc.) skip Obsidian entirely
    if is_external_event_query(&lowered) {
        return Ok(None);
    }
    // Week-related queries: personal recap or explicit week reference
    if crate::app::chat::agent::context::is_personal_recap_query(&lowered)
        || crate::app::chat::agent::context::is_week_note_query(&lowered)
    {
        // Try comprehensive date parsing first
        let target_week = if let Some(dates::DateReference::Week(week)) =
            dates::parse_date_reference(&lowered)
        {
            week
        } else {
            // Fallback to old logic
            dates::resolve_query_week(&lowered)
        };
        let notes = obsidian::week_notes_context(vault_path, target_week)?;
        let count = notes.len();
        let mut blocks = Vec::new();
        if let Some(content) = obsidian::format_obsidian_context("Obsidian weekly notes", &notes) {
            blocks.push(clamp_context_chars(&content, MAX_OBSIDIAN_CONTEXT_CHARS));
        } else {
            blocks.push("--- Obsidian weekly notes ---".to_string());
            blocks.push(format!(
                "No weekly notes found for {}-W{:02}.",
                target_week.year,
                target_week.week
            ));
        }
        if is_checklist_query(&lowered) {
            let checklist = obsidian::week_note_checklist(vault_path, target_week)?;
            blocks.push("--- Weekly checklist ---".to_string());
            if checklist.is_empty() {
                blocks.push("No checklist items found in the weekly note.".to_string());
            } else {
                blocks.extend(checklist);
            }
        }
        return Ok(Some(ObsidianContext {
            content: blocks.join("\n"),
            count,
        }));
    }
    // General Obsidian search
    let notes = obsidian::search_notes(vault_path, query, 5)?;
    if let Some(content) = obsidian::format_obsidian_context("Obsidian notes", &notes) {
        let count = notes.len();
        let content = clamp_context_chars(&content, MAX_OBSIDIAN_CONTEXT_CHARS);
        return Ok(Some(ObsidianContext { content, count }));
    }
    Ok(None)
}


pub fn is_external_event_query(lowered: &str) -> bool {
    let event_terms = [
        "happening",
        "happened",
        "news",
        "events",
        "what is going on",
        "what's going on",
    ];
    let time_terms = ["today", "current", "latest", "now"];
    let has_event_term = event_terms.iter().any(|term| lowered.contains(term));
    let has_time_term = time_terms.iter().any(|term| lowered.contains(term));
    let has_location = [" in ", " near ", " at "]
        .iter()
        .any(|token| lowered.contains(token));
    (has_event_term && (has_time_term || has_location))
        || (lowered.contains("news") && (has_time_term || has_location))
}

fn is_checklist_query(lowered: &str) -> bool {
    let triggers = ["checklist", "todo", "to-do", "tasks", "task list"];
    triggers.iter().any(|term| lowered.contains(term))
}

fn clamp_context_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    value.chars().take(max_chars).collect()
}
