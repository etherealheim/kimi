use crate::agents::{Agent, AgentManager, ChatMessage};
use crate::app::chat::agent::context::{is_personal_recap_query, is_week_note_query};
use crate::app::chat::agent::json::extract_json_object;
use serde::Deserialize;

const INTENT_SYSTEM_PROMPT: &str = r#"You are an intent classifier for a personal assistant.
Return ONLY valid JSON in this exact schema:
{"intent":"note_lookup|note_create|week_notes|personal_recap|external_event|general","confidence":0.0-1.0}

Rules:
- "note_lookup": the user wants to find content in their notes/vault/obsidian.
- "note_create": the user wants to create or add a note.
- "week_notes": weekly note, recap of week, week checklist.
- "personal_recap": recap/summarize personal history or recent chats.
- "external_event": live/real-time events/news/what is happening now.
- "general": everything else.
- Be conservative; only choose non-general when confident.
"#;

const INTENT_CONFIDENCE_THRESHOLD: f32 = 0.6;

#[derive(Debug, Clone, Copy)]
pub struct QueryIntent {
    pub is_external_event: bool,
    pub is_note_lookup: bool,
    pub is_note_creation: bool,
    pub is_personal_recap: bool,
    pub is_week_note: bool,
}

pub fn classify_query(query: &str) -> QueryIntent {
    let lowered = query.trim().to_lowercase();
    let is_external_event = is_external_event_query(&lowered);
    let is_note_creation = is_note_creation_query(&lowered);
    let is_note_lookup = !is_note_creation && is_note_lookup_query(&lowered);
    let is_personal_recap = is_personal_recap_query(&lowered);
    let is_week_note = is_week_note_query(&lowered);
    QueryIntent {
        is_external_event,
        is_note_lookup,
        is_note_creation,
        is_personal_recap,
        is_week_note,
    }
}

pub struct IntentModelContext<'a> {
    pub manager: &'a AgentManager,
    pub routing_agent: Option<&'a Agent>,
    pub fallback_agent: &'a Agent,
}

pub fn classify_query_with_model(query: &str, context: IntentModelContext<'_>) -> QueryIntent {
    let heuristic_intent = classify_query(query);
    if is_explicit_intent(heuristic_intent) {
        return heuristic_intent;
    }
    let Some(model_intent) = classify_with_model(query, context) else {
        return heuristic_intent;
    };
    merge_model_intent(heuristic_intent, model_intent)
}

fn is_note_creation_query(lowered: &str) -> bool {
    let triggers = [
        "make a note",
        "create a note",
        "add a note",
        "write a note",
        "start a note",
        "new note",
    ];
    triggers.iter().any(|term| lowered.contains(term))
}

fn is_note_lookup_query(lowered: &str) -> bool {
    let lookup_terms = [
        "in my notes",
        "from my notes",
        "in notes",
        "my notes about",
        "notes about",
        "wrote in my notes",
        "have in my notes",
        "in my note",
        "from my note",
        "my note about",
        "note about",
        "note on",
        "note for",
        "note regarding",
        "in my obsidian",
        "from obsidian",
        "obsidian note",
        "obsidian notes",
        "in my vault",
        "from my vault",
        "vault note",
        "vault notes",
    ];
    if lookup_terms.iter().any(|term| lowered.contains(term)) {
        return true;
    }
    let has_note_word = lowered.contains(" note") || lowered.contains(" notes");
    let has_vault_word = lowered.contains("obsidian") || lowered.contains("vault");
    if has_note_word || has_vault_word {
        return true;
    }
    looks_like_note_title_query(lowered)
}

fn looks_like_note_title_query(query: &str) -> bool {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return false;
    }
    let word_count = trimmed.split_whitespace().count();
    if word_count > 6 {
        return false;
    }
    let has_question_word = ["what", "when", "where", "why", "how", "who", "which"]
        .iter()
        .any(|word| trimmed.starts_with(word));
    if has_question_word && word_count > 4 {
        return false;
    }
    let has_action_verb = ["search", "find", "lookup", "get", "show", "tell", "give"]
        .iter()
        .any(|verb| trimmed.contains(verb));
    if has_action_verb {
        return true;
    }
    if word_count <= 4 && !has_question_word {
        return true;
    }
    false
}

fn is_external_event_query(lowered: &str) -> bool {
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
    (has_event_term || lowered.contains("news")) && (has_time_term || has_location)
}

fn is_explicit_intent(intent: QueryIntent) -> bool {
    intent.is_note_lookup
        || intent.is_note_creation
        || intent.is_personal_recap
        || intent.is_week_note
        || intent.is_external_event
}

#[derive(Debug, Clone, Copy)]
enum ModelIntent {
    NoteLookup,
    NoteCreate,
    WeekNotes,
    PersonalRecap,
    ExternalEvent,
    General,
}

#[derive(Debug, Deserialize)]
struct IntentPayload {
    intent: String,
    confidence: Option<f32>,
}

fn classify_with_model(query: &str, context: IntentModelContext<'_>) -> Option<ModelIntent> {
    let agent = context
        .routing_agent
        .unwrap_or(context.fallback_agent);
    let messages = vec![
        ChatMessage::system(INTENT_SYSTEM_PROMPT),
        ChatMessage::user(query),
    ];
    let response = context.manager.chat(agent, &messages).ok()?;
    parse_model_intent(&response)
}

fn parse_model_intent(response: &str) -> Option<ModelIntent> {
    let json = extract_json_object(response)?;
    let payload: IntentPayload = serde_json::from_str(&json).ok()?;
    let confidence = payload.confidence.unwrap_or(0.0);
    if confidence < INTENT_CONFIDENCE_THRESHOLD {
        return None;
    }
    parse_model_intent_label(payload.intent.trim())
}

fn parse_model_intent_label(value: &str) -> Option<ModelIntent> {
    match value {
        "note_lookup" => Some(ModelIntent::NoteLookup),
        "note_create" => Some(ModelIntent::NoteCreate),
        "week_notes" => Some(ModelIntent::WeekNotes),
        "personal_recap" => Some(ModelIntent::PersonalRecap),
        "external_event" => Some(ModelIntent::ExternalEvent),
        "general" => Some(ModelIntent::General),
        _ => None,
    }
}

fn merge_model_intent(heuristic: QueryIntent, model_intent: ModelIntent) -> QueryIntent {
    match model_intent {
        ModelIntent::NoteLookup => QueryIntent {
            is_note_lookup: true,
            ..heuristic
        },
        ModelIntent::NoteCreate => QueryIntent {
            is_note_creation: true,
            ..heuristic
        },
        ModelIntent::WeekNotes => QueryIntent {
            is_week_note: true,
            ..heuristic
        },
        ModelIntent::PersonalRecap => QueryIntent {
            is_personal_recap: true,
            ..heuristic
        },
        ModelIntent::ExternalEvent => QueryIntent {
            is_external_event: true,
            ..heuristic
        },
        ModelIntent::General => heuristic,
    }
}
