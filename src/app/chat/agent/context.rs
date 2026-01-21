use crate::services::dates::DateReference;
use crate::storage::ConversationSummary;
use chrono::{Datelike, Duration, Local};
use color_eyre::Result;

#[derive(Debug, Clone)]
pub struct SummaryEntry {
    pub date: String,
    pub summary: String,
}

pub struct MemoryContext {
    pub content: String,
    pub count: usize,
}

#[derive(Debug, Clone, Copy)]
enum SummaryRange {
    Today,
    Yesterday,
    ThisWeek,
    LastWeek,
    LastDays(u32),
}

pub fn build_conversation_summary_entries(
    storage: Option<&crate::storage::StorageManager>,
    query: &str,
) -> Result<Vec<SummaryEntry>> {
    let Some(storage) = storage else {
        return Ok(Vec::new());
    };
    let Some(range) = summary_time_range(query) else {
        return Ok(Vec::new());
    };
    let conversations = storage.load_conversations()?;
    Ok(filter_summaries_by_range(&conversations, range))
}

pub fn format_summary_entries(entries: &[SummaryEntry]) -> String {
    entries
        .iter()
        .map(|entry| format!("- {}: {}", entry.date, entry.summary))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn build_memory_context(
    blocks: &crate::services::memories::MemoryBlocks,
    query: &str,
) -> Option<MemoryContext> {
    let tokens = tokenize_query(query);
    if tokens.is_empty() && !query.contains(':') {
        return None;
    }
    let lowered_query = query.to_lowercase();
    let mut sections = Vec::new();
    let mut section_count = 0usize;
    for (tag, lines) in &blocks.contexts {
        let tag_match = lowered_query.contains(tag);
        let filtered: Vec<String> = if tag_match {
            lines.clone()
        } else {
            lines
                .iter()
                .filter(|line| line_matches_tokens(line, &tokens))
                .cloned()
                .collect()
        };
        if filtered.is_empty() {
            continue;
        }
        // Count sections (e.g., likes, projects) rather than individual lines
        section_count += 1;
        sections.push(format!("[context:{}]", tag));
        sections.extend(filtered);
        sections.push(String::new());
    }
    if section_count == 0 {
        return None;
    }
    let content = sections.join("\n").trim_end().to_string();
    Some(MemoryContext {
        content,
        count: section_count,
    })
}

pub fn count_summary_matches(entries: &[SummaryEntry], tokens: &[String]) -> usize {
    let _ = tokens;
    entries.len()
}

pub fn tokenize_query(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for raw in query.split_whitespace() {
        let cleaned = raw
            .trim_matches(|character: char| !character.is_alphanumeric() && character != '-')
            .to_lowercase();
        if cleaned.len() < 2 {
            continue;
        }
        if !tokens.contains(&cleaned) {
            tokens.push(cleaned);
        }
    }
    tokens
}

fn summary_time_range(query: &str) -> Option<SummaryRange> {
    let lowered = query.to_lowercase();
    if !has_summary_intent(&lowered) {
        return None;
    }
    
    // Try comprehensive date parsing first
    if let Some(date_ref) = crate::services::dates::parse_date_reference(&lowered) {
        return match date_ref {
            DateReference::Date(date) => {
                // Single date - check if it's today/yesterday
                let today = Local::now().date_naive();
                if date == today {
                    Some(SummaryRange::Today)
                } else if date == today - Duration::days(1) {
                    Some(SummaryRange::Yesterday)
                } else {
                    // Use a 1-day range for this specific date
                    None
                }
            }
            DateReference::Week(week) => {
                // Week reference
                let current = crate::services::dates::current_week();
                let last = crate::services::dates::last_week();
                if week == current {
                    Some(SummaryRange::ThisWeek)
                } else if week == last {
                    Some(SummaryRange::LastWeek)
                } else {
                    None
                }
            }
            DateReference::Range(_range) => {
                // Range - for now, treat month/year ranges as "this week" fallback
                // TODO: Could add more granular range support
                Some(SummaryRange::ThisWeek)
            }
        };
    }
    
    // Fallback to manual parsing
    if let Some(days) = parse_last_days(&lowered) {
        return Some(SummaryRange::LastDays(days));
    }
    None
}

fn has_summary_intent(lowered: &str) -> bool {
    let triggers = [
        "remember",
        "recap",
        "summary",
        "what happened",
        "what did we",
        "what have we",
        "catch me up",
        "what were we",
        "what we talked",
        "my week",
        "this week",
    ];
    triggers.iter().any(|term| lowered.contains(term))
}

fn parse_last_days(lowered: &str) -> Option<u32> {
    let tokens: Vec<&str> = lowered.split_whitespace().collect();
    for window in tokens.windows(3) {
        if let [number, "days", "ago"] = window {
            if let Ok(value) = number.parse::<u32>() {
                return Some(value);
            }
        }
    }
    for window in tokens.windows(3) {
        if let [number, "days", "back"] = window {
            if let Ok(value) = number.parse::<u32>() {
                return Some(value);
            }
        }
    }
    None
}

fn filter_summaries_by_range(
    conversations: &[ConversationSummary],
    range: SummaryRange,
) -> Vec<SummaryEntry> {
    let today = Local::now().date_naive();
    let (start, end) = match range {
        SummaryRange::Today => (today, today),
        SummaryRange::Yesterday => (today - Duration::days(1), today - Duration::days(1)),
        SummaryRange::ThisWeek => {
            let days_from_monday = today.weekday().num_days_from_monday() as i64;
            (today - Duration::days(days_from_monday), today)
        }
        SummaryRange::LastWeek => (today - Duration::days(7), today - Duration::days(1)),
        SummaryRange::LastDays(days) => {
            let span = i64::from(days.max(1));
            (today - Duration::days(span), today)
        }
    };
    let mut entries = Vec::new();
    for convo in conversations {
        let Some(date) = parse_conversation_date(&convo.created_at) else {
            continue;
        };
        if date < start || date > end {
            continue;
        }
        let summary = convo
            .summary
            .clone()
            .or_else(|| convo.detailed_summary.clone())
            .unwrap_or_else(|| "Conversation".to_string());
        entries.push(SummaryEntry {
            date: date.format("%Y-%m-%d").to_string(),
            summary,
        });
    }
    entries
}

fn parse_conversation_date(created_at: &str) -> Option<chrono::NaiveDate> {
    chrono::DateTime::parse_from_rfc3339(created_at)
        .ok()
        .map(|value| value.date_naive())
}

pub fn is_personal_recap_query(lowered: &str) -> bool {
    let triggers = [
        "summarize my week",
        "summary of my week",
        "recap my week",
        "my week",
        "this week recap",
        "weekly summary",
    ];
    triggers.iter().any(|term| lowered.contains(term))
}

pub fn is_week_note_query(lowered: &str) -> bool {
    let triggers = [
        "weekly note",
        "week note",
        "this week note",
        "this week",
        "last week",
        "next week",
        "weekly checklist",
        "week checklist",
    ];
    triggers.iter().any(|term| lowered.contains(term)) || has_week_reference(lowered)
}

fn has_week_reference(lowered: &str) -> bool {
    for token in tokenize_query(lowered) {
        if is_week_token(&token) {
            return true;
        }
    }
    false
}

fn is_week_token(token: &str) -> bool {
    let lowered = token.to_lowercase();
    let mut parts = lowered.split("-w");
    let Some(year_part) = parts.next() else {
        return false;
    };
    let Some(week_part) = parts.next() else {
        return false;
    };
    if year_part.len() != 4 {
        return false;
    }
    let year_ok = year_part.chars().all(|character| character.is_ascii_digit());
    let week_ok = week_part
        .chars()
        .take(2)
        .all(|character| character.is_ascii_digit());
    year_ok && week_ok
}

fn line_matches_tokens(value: &str, tokens: &[String]) -> bool {
    if tokens.is_empty() {
        return false;
    }
    let lowered = value.to_lowercase();
    tokens.iter().any(|token| lowered.contains(token))
}
