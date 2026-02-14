use crate::services::dates::DateReference;
use crate::storage::{ConversationSummary, ConversationWithMessages};
use chrono::{Datelike, Duration, Local, NaiveDate};
use color_eyre::Result;

/// Maximum messages per conversation when loading full content
const MAX_MESSAGES_PER_CONVERSATION: usize = 8;
/// Maximum total characters across all conversations to stay within context budget
const MAX_TOTAL_CHARS: usize = 6000;

// ── Public result types ─────────────────────────────────────────────────────

/// Recall result injected into the system prompt
#[derive(Debug, Clone)]
pub struct RecallResult {
    /// Number of conversations recalled
    pub conversation_count: usize,
    /// Formatted text block for the system prompt
    pub prompt_text: String,
}

// ── Main entry point ────────────────────────────────────────────────────────

/// Builds conversation recall context for the system prompt.
/// For short ranges (today, yesterday) loads actual message content.
/// For wider ranges (this week, etc.) falls back to summaries.
pub fn build_conversation_recall(
    storage: Option<&crate::storage::StorageManager>,
    query: &str,
) -> Result<Option<RecallResult>> {
    let Some(storage) = storage else {
        return Ok(None);
    };
    let Some(range) = recall_time_range(query) else {
        return Ok(None);
    };

    let runtime = tokio::runtime::Runtime::new()?;
    let today = Local::now().date_naive();
    let (start_date, end_date) = range_to_dates(range, today);

    // For short ranges (1-2 days), load actual messages
    let day_span = (end_date - start_date).num_days() + 1;
    if day_span <= 2 {
        let start_rfc = format!("{}T00:00:00+00:00", start_date);
        let end_rfc = format!("{}T00:00:00+00:00", end_date + Duration::days(1));

        let conversations = runtime.block_on(async {
            storage
                .load_conversations_in_date_range(
                    &start_rfc,
                    &end_rfc,
                    MAX_MESSAGES_PER_CONVERSATION,
                )
                .await
        })?;

        if conversations.is_empty() {
            return Ok(None);
        }

        let prompt_text = format_conversation_content(&conversations);
        return Ok(Some(RecallResult {
            conversation_count: conversations.len(),
            prompt_text,
        }));
    }

    // For wider ranges, fall back to summaries
    let conversations = runtime.block_on(async { storage.load_conversations().await })?;
    let entries = filter_summaries_by_range(&conversations, start_date, end_date);
    if entries.is_empty() {
        return Ok(None);
    }

    let prompt_text = format_summary_recall(&entries);
    Ok(Some(RecallResult {
        conversation_count: entries.len(),
        prompt_text,
    }))
}

// ── Formatting ──────────────────────────────────────────────────────────────

/// Formats actual conversation messages grouped by conversation
fn format_conversation_content(conversations: &[ConversationWithMessages]) -> String {
    let mut lines = Vec::new();
    lines.push("--- Past conversations ---".to_string());
    lines.push(
        "Below are actual messages from your past conversations with this user. \
         Use them to answer naturally — never say you can't remember."
            .to_string(),
    );

    let mut total_chars = 0;
    for (index, conversation) in conversations.iter().enumerate() {
        let time_label = parse_conversation_time(&conversation.created_at);
        lines.push(format!("\n[Conversation {}, {}]", index + 1, time_label));

        for message in &conversation.messages {
            let role_label = match message.role.as_str() {
                "User" => "User",
                "Assistant" => "Kimi",
                _ => continue,
            };
            let line = format!("{}: {}", role_label, message.content);
            total_chars += line.len();
            if total_chars > MAX_TOTAL_CHARS {
                lines.push("(... earlier messages trimmed ...)".to_string());
                return lines.join("\n");
            }
            lines.push(line);
        }
    }

    lines.join("\n")
}

struct SummaryLine {
    date: String,
    summary: String,
}

fn format_summary_recall(entries: &[SummaryLine]) -> String {
    let mut lines = Vec::new();
    lines.push("--- Conversation summaries ---".to_string());
    lines.push(
        "Use the summaries below to answer recap questions. \
         If they are insufficient, ask a clarifying question."
            .to_string(),
    );
    for entry in entries {
        lines.push(format!("- {}: {}", entry.date, entry.summary));
    }
    lines.join("\n")
}

fn parse_conversation_time(created_at: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(created_at)
        .ok()
        .map_or_else(|| "unknown time".to_string(), |dt| dt.format("%H:%M").to_string())
}

// ── Time range detection ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
enum RecallRange {
    Today,
    Yesterday,
    ThisWeek,
    LastWeek,
    LastDays(u32),
}

fn recall_time_range(query: &str) -> Option<RecallRange> {
    let lowered = query.to_lowercase();
    if !has_recall_intent(&lowered) {
        return None;
    }

    // Try comprehensive date parsing first
    if let Some(date_ref) = crate::services::dates::parse_date_reference(&lowered) {
        return match date_ref {
            DateReference::Date(date) => {
                let today = Local::now().date_naive();
                if date == today {
                    Some(RecallRange::Today)
                } else if date == today - Duration::days(1) {
                    Some(RecallRange::Yesterday)
                } else {
                    None
                }
            }
            DateReference::Week(week) => {
                let current = crate::services::dates::current_week();
                let last = crate::services::dates::last_week();
                if week == current {
                    Some(RecallRange::ThisWeek)
                } else if week == last {
                    Some(RecallRange::LastWeek)
                } else {
                    None
                }
            }
            DateReference::Range(_) => Some(RecallRange::ThisWeek),
        };
    }

    // Fallback to manual parsing
    if let Some(days) = parse_last_days(&lowered) {
        return Some(RecallRange::LastDays(days));
    }
    None
}

fn has_recall_intent(lowered: &str) -> bool {
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

fn range_to_dates(range: RecallRange, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    match range {
        RecallRange::Today => (today, today),
        RecallRange::Yesterday => {
            let yesterday = today - Duration::days(1);
            (yesterday, yesterday)
        }
        RecallRange::ThisWeek => {
            let days_from_monday = today.weekday().num_days_from_monday() as i64;
            (today - Duration::days(days_from_monday), today)
        }
        RecallRange::LastWeek => (today - Duration::days(7), today - Duration::days(1)),
        RecallRange::LastDays(days) => {
            let span = i64::from(days.max(1));
            (today - Duration::days(span), today)
        }
    }
}

fn parse_last_days(lowered: &str) -> Option<u32> {
    let tokens: Vec<&str> = lowered.split_whitespace().collect();
    for window in tokens.windows(3) {
        if let [number, "days", "ago"] = window
            && let Ok(value) = number.parse::<u32>()
        {
            return Some(value);
        }
    }
    for window in tokens.windows(3) {
        if let [number, "days", "back"] = window
            && let Ok(value) = number.parse::<u32>()
        {
            return Some(value);
        }
    }
    None
}

// ── Summary fallback for wider ranges ───────────────────────────────────────

fn filter_summaries_by_range(
    conversations: &[ConversationSummary],
    start: NaiveDate,
    end: NaiveDate,
) -> Vec<SummaryLine> {
    let mut entries = Vec::new();
    for convo in conversations {
        let Some(date) = parse_conversation_date(&convo.created_at) else {
            continue;
        };
        if date < start || date > end {
            continue;
        }
        let summary_text = convo
            .detailed_summary
            .as_deref()
            .filter(|value| is_real_summary(value))
            .or_else(|| convo.summary.as_deref().filter(|value| is_real_summary(value)));
        let Some(summary) = summary_text else {
            continue;
        };
        entries.push(SummaryLine {
            date: date.format("%Y-%m-%d").to_string(),
            summary: summary.to_string(),
        });
    }
    entries
}

fn is_real_summary(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && !trimmed.eq_ignore_ascii_case("conversation")
        && !trimmed.eq_ignore_ascii_case(crate::app::chat::summary::PENDING_SUMMARY_LABEL)
}

fn parse_conversation_date(created_at: &str) -> Option<NaiveDate> {
    chrono::DateTime::parse_from_rfc3339(created_at)
        .ok()
        .map(|value| value.date_naive())
}

// ── Re-exports used by other modules ────────────────────────────────────────

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
        "past week",
        "previous week",
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
