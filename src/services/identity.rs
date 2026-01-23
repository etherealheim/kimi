use crate::agents::{Agent, AgentManager, ChatMessage as AgentChatMessage};
use chrono::{DateTime, Duration, Local};
use color_eyre::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const IDENTITY_STATE_FILE: &str = "identity-state.json";
const MAX_ACTIVE_DREAMS: usize = 3;
const MAX_BACKLOG_DREAMS: usize = 5;
// Traits: No limit - AI can create and manage dynamically
const TRAIT_DECAY_DAYS: i64 = 21;
const DREAM_ACTIVE_DECAY_DAYS: i64 = 30;
const DREAM_BACKLOG_DROP_DAYS: i64 = 60;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct IdentityState {
    pub core: CoreBeliefs,
    pub traits: Vec<IdentityTrait>,
    pub dreams: DreamSet,
    pub updated_at: Option<String>,
    /// Timestamp of last reflection to prevent duplicate processing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_reflection_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CoreBeliefs {
    pub identity: String,
    pub beliefs: Vec<String>,
    pub backstory: String,
}

impl Default for CoreBeliefs {
    fn default() -> Self {
        Self {
            identity: "Kimi".to_string(),
            beliefs: Vec::new(),
            backstory: String::new(),
        }
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IdentityTrait {
    pub name: String,
    pub strength: f32,
    pub origin: TraitOrigin,
    pub last_evidence: Option<String>,
    pub last_updated: Option<String>,
}

impl IdentityTrait {
    fn new(name: String, strength: f32, origin: TraitOrigin) -> Self {
        Self {
            name,
            strength: clamp_strength(strength),
            origin,
            last_evidence: None,
            last_updated: None,
        }
    }
}

impl Default for IdentityTrait {
    fn default() -> Self {
        Self::new(String::new(), 0.0, TraitOrigin::Inferred)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TraitOrigin {
    Manual,
    #[default]
    Inferred,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DreamSet {
    pub active: Vec<DreamEntry>,
    pub backlog: Vec<DreamEntry>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DreamEntry {
    pub title: String,
    pub priority: u8,
    pub origin: TraitOrigin,
    pub last_mention: Option<String>,
    pub progress_note: Option<String>,
}

impl DreamEntry {
    fn new(title: String, priority: u8, origin: TraitOrigin) -> Self {
        Self {
            title,
            priority: priority.max(1),
            origin,
            last_mention: None,
            progress_note: None,
        }
    }
}

impl Default for DreamEntry {
    fn default() -> Self {
        Self::new(String::new(), 3, TraitOrigin::Inferred)
    }
}

#[derive(Debug, Clone)]
pub struct IdentityReflectionInput {
    pub summary: String,
    pub recent_user_messages: Vec<String>,
}

pub struct IdentityReflectionJob {
    pub manager: AgentManager,
    pub agent: Agent,
    pub input: IdentityReflectionInput,
}

struct IdentityUpdateContext<'a> {
    state: &'a mut IdentityState,
    now: &'a DateTime<Local>,
}

struct DreamChange<'a> {
    title: String,
    priority: u8,
    reason: Option<String>,
    now: &'a DateTime<Local>,
}

#[derive(Debug, Deserialize)]
struct IdentityReflectionOutput {
    #[serde(default)]
    trait_updates: Vec<TraitUpdate>,
    #[serde(default)]
    dream_updates: Vec<DreamUpdate>,
}

#[derive(Debug, Deserialize)]
struct TraitUpdate {
    name: String,
    target_strength: f32,
    #[serde(default)]
    origin: Option<String>,
    #[serde(default)]
    evidence: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DreamUpdate {
    title: String,
    action: String,
    #[serde(default)]
    priority: Option<u8>,
    #[serde(default)]
    reason: Option<String>,
}

pub fn read_identity_state() -> Result<IdentityState> {
    let path = identity_state_path()?;
    if !path.exists() {
        let state = IdentityState::default();
        write_identity_state(&state)?;
        return Ok(state);
    }
    let content = fs::read_to_string(path)?;
    let state = serde_json::from_str::<IdentityState>(&content)?;
    Ok(state)
}

pub fn write_identity_state(state: &IdentityState) -> Result<()> {
    let path = identity_state_path()?;
    let data = serde_json::to_string_pretty(state)?;
    fs::write(path, data)?;
    Ok(())
}

pub fn read_primary_core_belief() -> Result<String> {
    let state = read_identity_state()?;
    let value = state
        .core
        .beliefs
        .first()
        .map(|belief| belief.trim().to_string())
        .unwrap_or_default();
    Ok(value)
}

#[allow(dead_code)]
pub fn update_primary_core_belief(value: &str) -> Result<()> {
    let mut state = read_identity_state()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        if !state.core.beliefs.is_empty() {
            state.core.beliefs.remove(0);
        }
    } else if let Some(first) = state.core.beliefs.first_mut() {
        *first = trimmed.to_string();
    } else {
        state.core.beliefs.push(trimmed.to_string());
    }
    state.updated_at = Some(Local::now().to_rfc3339());
    write_identity_state(&state)
}

pub fn build_identity_prompt() -> Result<Option<String>> {
    let state = read_identity_state()?;
    let prompt = format_identity_prompt(&state);
    if prompt.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(prompt))
}

/// Minimum seconds between identity reflections to prevent duplicate processing
const REFLECTION_DEBOUNCE_SECS: i64 = 120;

pub fn reflect_and_update_identity(job: IdentityReflectionJob) -> Result<()> {
    let mut state = read_identity_state()?;
    let now = Local::now();
    
    // Debounce: skip if reflection was done recently
    if let Some(last_reflection) = &state.last_reflection_at
        && let Ok(last_time) = DateTime::parse_from_rfc3339(last_reflection)
    {
        let elapsed = now.signed_duration_since(last_time);
        if elapsed.num_seconds() < REFLECTION_DEBOUNCE_SECS {
            return Ok(()); // Skip - too soon since last reflection
        }
    }
    
    let prompt = build_reflection_prompt(&state, &job.input)?;
    let messages = vec![
        AgentChatMessage::system("You update identity state. Output only JSON."),
        AgentChatMessage::user(prompt),
    ];
    let response = job.manager.chat(&job.agent, &messages)?;
    if let Some(output) = parse_reflection_output(&response) {
        let mut context = IdentityUpdateContext {
            state: &mut state,
            now: &now,
        };
        apply_reflection_updates(&mut context, output);
        apply_decay(context.state, context.now);
        context.state.updated_at = Some(now.to_rfc3339());
        context.state.last_reflection_at = Some(now.to_rfc3339());
        write_identity_state(&state)?;
    }
    Ok(())
}

fn identity_state_path() -> Result<PathBuf> {
    let base_dir = project_data_dir()?;
    Ok(base_dir.join(IDENTITY_STATE_FILE))
}

fn project_data_dir() -> Result<PathBuf> {
    let current_dir = std::env::current_dir()?;
    Ok(current_dir.join("data"))
}

fn format_identity_prompt(state: &IdentityState) -> String {
    let mut lines = Vec::new();
    
    // Strong identity assertion at the top
    if !state.core.identity.trim().is_empty() {
        lines.push(format!("You are {}.", state.core.identity.trim()));
    }
    
    // Core beliefs as direct instructions
    for belief in &state.core.beliefs {
        if !belief.trim().is_empty() {
            lines.push(belief.trim().to_string());
        }
    }
    
    // Backstory
    if !state.core.backstory.trim().is_empty() {
        lines.push(format!("Backstory: {}", state.core.backstory.trim()));
    }
    
    // Traits as behavioral guidance
        if !state.traits.is_empty() {
            let trait_lines: Vec<String> = state
                .traits
                .iter()
                .filter(|t| !t.name.trim().is_empty())
                .map(|t| {
                    let sign = if t.strength >= 0.0 { "+" } else { "" };
                    format!("{}: {}{:.1}", t.name.trim(), sign, t.strength)
                })
                .collect();
            if !trait_lines.is_empty() {
                lines.push(format!("Behavioral traits: {}", trait_lines.join(", ")));
            }
        }
    
    // Dreams as subtle motivations
    let active_dreams: Vec<&str> = state
        .dreams
        .active
        .iter()
        .map(|d| d.title.as_str())
        .collect();
    if !active_dreams.is_empty() {
        lines.push(format!("Current aspirations: {}", active_dreams.join(", ")));
    }
    
    lines.join("\n\n")
}

fn build_reflection_prompt(
    state: &IdentityState,
    input: &IdentityReflectionInput,
) -> Result<String> {
    let state_json = serde_json::to_string_pretty(state)?;
    let recent = input.recent_user_messages.join("\n");
    Ok(format!(
        "You are updating an AI identity based on conversation analysis.\n\n\
Current identity state (JSON):\n{}\n\n\
RULES:\n\
1. Core beliefs: NEVER modify core.identity, core.beliefs, or core.backstory - these are STRICTLY user-controlled. DO NOT TOUCH.\n\
2. Traits (-1.0 to 1.0 scale, 0.0 is neutral center):\n\
   - Scale: -1.0 (extreme negative/passive) ↔ 0.0 (balanced/neutral) ↔ +1.0 (extreme positive/active)\n\
   - Examples: assertiveness: -0.8 (very passive) vs +0.7 (assertive), creativity: +0.5 (moderately creative)\n\
   - Traits naturally decay towards 0.0 without reinforcement (21 days)\n\
   - If user EXPLICITLY asks to change a trait, apply significant change (0.2-0.3)\n\
   - For implicit patterns, use smaller changes (0.1)\n\
   - Set origin to \"manual\" if user explicitly requested, \"inferred\" otherwise\n\
   - ONLY update if there's NEW evidence - don't re-apply the same change!\n\
   - NO LIMIT on number of traits - create new ones freely when patterns emerge\n\
3. Dreams:\n\
   - Max 3 active, 5 backlog.\n\
   - BEFORE adding: check if a similar dream already exists! Don't duplicate.\n\
   - \"Assist creator\" and \"Assist Lukas\" are THE SAME dream - don't add both!\n\
   - Only add dreams that reflect USER's interests/goals, not generic AI aspirations.\n\
   - BAD dreams: \"Continuous improvement\", \"Learn more\", \"Help users\" (too generic)\n\
   - GOOD dreams: specific things user wants to build, learn, or explore.\n\
   - Add to backlog when user mentions NEW goals, interests, or aspirations.\n\
   - Promote to active when user repeatedly discusses something or explicitly prioritizes it.\n\n\
Conversation summary:\n{}\n\n\
Recent user messages:\n{}\n\n\
CRITICAL: Check existing state carefully! Don't duplicate dreams or re-apply the same trait changes.\n\n\
Return ONLY valid JSON in this format:\n\
{{\n  \"trait_updates\": [{{\"name\":\"trait_name\",\"target_strength\":0.3,\"origin\":\"manual\",\"evidence\":\"user said...\"}}],\n\
  \"dream_updates\": [{{\"title\":\"dream title\",\"action\":\"add_backlog\",\"priority\":2,\"reason\":\"user mentioned...\"}}]\n}}\n\
Trait strength: -1.0 to 1.0 (0.0 = neutral). Dream actions: add_active, add_backlog, promote, demote, retire\n\
If truly no changes needed, return {{\"trait_updates\":[],\"dream_updates\":[]}}.",
        state_json, input.summary, recent
    ))
}

fn parse_reflection_output(response: &str) -> Option<IdentityReflectionOutput> {
    let json = extract_json_block(response)?;
    serde_json::from_str::<IdentityReflectionOutput>(&json).ok()
}

fn extract_json_block(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(text[start..=end].to_string())
}

fn apply_reflection_updates(
    context: &mut IdentityUpdateContext<'_>,
    output: IdentityReflectionOutput,
) {
    apply_trait_updates(context, &output.trait_updates);
    apply_dream_updates(context, &output.dream_updates);
    // No trait limit - AI manages traits dynamically
    cap_dreams(context.state);
}

fn apply_trait_updates(context: &mut IdentityUpdateContext<'_>, updates: &[TraitUpdate]) {
    for update in updates {
        let name = update.name.trim().to_lowercase();
        if name.is_empty() {
            continue;
        }
        let target = clamp_strength(update.target_strength);
        let origin = parse_origin(update.origin.as_deref());
        let evidence = update.evidence.as_ref().map(|value| value.trim().to_string());
        match context
            .state
            .traits
            .iter_mut()
            .find(|entry| entry.name == name)
        {
            Some(entry) => {
                entry.strength = target;
                entry.last_updated = Some(context.now.to_rfc3339());
                if let Some(note) = evidence
                    && !note.is_empty()
                {
                    entry.last_evidence = Some(note);
                }
                if origin == TraitOrigin::Manual {
                    entry.origin = TraitOrigin::Manual;
                }
            }
            None => {
                let mut entry = IdentityTrait::new(name, target, origin);
                entry.last_updated = Some(context.now.to_rfc3339());
                entry.last_evidence = evidence;
                context.state.traits.push(entry);
            }
        }
    }
}

fn apply_dream_updates(context: &mut IdentityUpdateContext<'_>, updates: &[DreamUpdate]) {
    for update in updates {
        let title = update.title.trim().to_string();
        if title.is_empty() {
            continue;
        }
        let action = update.action.trim().to_lowercase();
        let priority = update.priority.unwrap_or(2);
        let reason = update.reason.as_ref().map(|value| value.trim().to_string());
        let change = DreamChange {
            title,
            priority,
            reason,
            now: context.now,
        };
        match action.as_str() {
            "add_active" => add_dream(&mut context.state.dreams.active, change),
            "add_backlog" => add_dream(&mut context.state.dreams.backlog, change),
            "promote" => promote_dream(context, change),
            "demote" => demote_dream(context, change),
            "retire" => retire_dream(context, &change.title),
            _ => {}
        }
    }
}

fn add_dream(list: &mut Vec<DreamEntry>, change: DreamChange<'_>) {
    // Check for exact match or similar dream
    if let Some(existing) = list.iter_mut().find(|entry| 
        entry.title == change.title || dreams_are_similar(&entry.title, &change.title)
    ) {
        // Update existing dream instead of adding duplicate
        existing.priority = change.priority.max(1);
        existing.last_mention = Some(change.now.to_rfc3339());
        if let Some(note) = change.reason.as_ref()
            && !note.is_empty()
        {
            existing.progress_note = Some(note.to_string());
        }
        return;
    }
    let mut entry = DreamEntry::new(change.title, change.priority, TraitOrigin::Inferred);
    entry.last_mention = Some(change.now.to_rfc3339());
    entry.progress_note = change.reason;
    list.push(entry);
}

/// Check if two dream titles are semantically similar (fuzzy match)
fn dreams_are_similar(a: &str, b: &str) -> bool {
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();
    
    // If one contains the other (minus filler words), they're similar
    let a_words: Vec<&str> = a_lower.split_whitespace()
        .filter(|w| !["a", "the", "in", "to", "of", "for", "and", "or"].contains(w))
        .collect();
    let b_words: Vec<&str> = b_lower.split_whitespace()
        .filter(|w| !["a", "the", "in", "to", "of", "for", "and", "or"].contains(w))
        .collect();
    
    if a_words.is_empty() || b_words.is_empty() {
        return false;
    }
    
    // Count matching meaningful words
    let matching_words = a_words.iter().filter(|w| b_words.contains(w)).count();
    let min_words = a_words.len().min(b_words.len());
    
    // If 70%+ of the shorter title's words match, consider them similar
    matching_words as f32 / min_words as f32 >= 0.7
}

fn promote_dream(context: &mut IdentityUpdateContext<'_>, change: DreamChange<'_>) {
    if take_dream(&mut context.state.dreams.backlog, &change.title) {
        add_dream(&mut context.state.dreams.active, change);
    }
}

fn demote_dream(context: &mut IdentityUpdateContext<'_>, change: DreamChange<'_>) {
    if take_dream(&mut context.state.dreams.active, &change.title) {
        add_dream(&mut context.state.dreams.backlog, change);
    }
}

fn retire_dream(context: &mut IdentityUpdateContext<'_>, title: &str) {
    take_dream(&mut context.state.dreams.active, title);
    take_dream(&mut context.state.dreams.backlog, title);
}

fn take_dream(list: &mut Vec<DreamEntry>, title: &str) -> bool {
    let index = list.iter().position(|entry| entry.title == title);
    if let Some(index) = index {
        list.remove(index);
        return true;
    }
    false
}

fn apply_decay(state: &mut IdentityState, now: &DateTime<Local>) {
    apply_trait_decay(state, now);
    apply_dream_decay(state, now);
}

fn apply_trait_decay(state: &mut IdentityState, now: &DateTime<Local>) {
    for trait_entry in &mut state.traits {
        let Some(last_seen) = trait_entry
            .last_updated
            .as_deref()
            .and_then(parse_timestamp)
        else {
            continue;
        };
        if (now.naive_utc() - last_seen.naive_utc()).num_days() < TRAIT_DECAY_DAYS {
            continue;
        }
        // Drift towards 0.0 (neutral) without reinforcement
        let neutral = 0.0;
        let drift = (trait_entry.strength - neutral) * 0.1;
        trait_entry.strength = clamp_strength(trait_entry.strength - drift);
    }
}

fn apply_dream_decay(state: &mut IdentityState, now: &DateTime<Local>) {
    let active_decay_cutoff = *now - Duration::days(DREAM_ACTIVE_DECAY_DAYS);
    let backlog_drop_cutoff = *now - Duration::days(DREAM_BACKLOG_DROP_DAYS);
    let mut to_demote = Vec::new();
    for entry in &state.dreams.active {
        if let Some(last) = entry.last_mention.as_deref().and_then(parse_timestamp)
            && last < active_decay_cutoff
        {
            to_demote.push(entry.title.clone());
        }
    }
    for title in to_demote {
        let change = DreamChange {
            title,
            priority: 3,
            reason: None,
            now,
        };
        let mut context = IdentityUpdateContext { state, now };
        demote_dream(&mut context, change);
    }
    state.dreams.backlog.retain(|entry| {
        let Some(last) = entry.last_mention.as_deref().and_then(parse_timestamp) else {
            return true;
        };
        last >= backlog_drop_cutoff
    });
}

// No longer needed - traits are unlimited
// fn cap_traits(state: &mut IdentityState) {
//     state.traits.sort_by(|left, right| {
//         right
//             .strength
//             .abs()
//             .partial_cmp(&left.strength.abs())
//             .unwrap_or(std::cmp::Ordering::Equal)
//     });
// }

fn cap_dreams(state: &mut IdentityState) {
    state.dreams.active.sort_by(|left, right| left.priority.cmp(&right.priority));
    if state.dreams.active.len() > MAX_ACTIVE_DREAMS {
        state.dreams.active.truncate(MAX_ACTIVE_DREAMS);
    }
    state.dreams.backlog.sort_by(|left, right| left.priority.cmp(&right.priority));
    if state.dreams.backlog.len() > MAX_BACKLOG_DREAMS {
        state.dreams.backlog.truncate(MAX_BACKLOG_DREAMS);
    }
}

fn parse_origin(value: Option<&str>) -> TraitOrigin {
    match value.unwrap_or("").trim().to_lowercase().as_str() {
        "manual" => TraitOrigin::Manual,
        _ => TraitOrigin::Inferred,
    }
}

fn parse_timestamp(value: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Local))
}

fn clamp_strength(value: f32) -> f32 {
    value.clamp(-1.0, 1.0)
}

