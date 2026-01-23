use crate::agents::{Agent, AgentManager, ChatMessage as AgentChatMessage};
use chrono::{DateTime, Duration, Local};
use color_eyre::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const IDENTITY_STATE_FILE: &str = "identity-state.json";
const MAX_ACTIVE_DREAMS: usize = 3;
const MAX_BACKLOG_DREAMS: usize = 5;
const MAX_TRAITS: usize = 8;
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
        Self::new(String::new(), 0.5, TraitOrigin::Inferred)
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

pub fn reflect_and_update_identity(job: IdentityReflectionJob) -> Result<()> {
    let mut state = read_identity_state()?;
    let prompt = build_reflection_prompt(&state, &job.input)?;
    let messages = vec![
        AgentChatMessage::system("You update identity state. Output only JSON."),
        AgentChatMessage::user(prompt),
    ];
    let response = job.manager.chat(&job.agent, &messages)?;
    if let Some(output) = parse_reflection_output(&response) {
        let now = Local::now();
        let mut context = IdentityUpdateContext {
            state: &mut state,
            now: &now,
        };
        apply_reflection_updates(&mut context, output);
        apply_decay(context.state, context.now);
        context.state.updated_at = Some(now.to_rfc3339());
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
            .map(|t| format!("{}: {:.1}/10", t.name.trim(), t.strength * 10.0))
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
        "Identity state (JSON):\n{}\n\n\
Rules:\n\
- Core beliefs are manual only; do NOT modify core.\n\
- Traits: adjust strength 0.0-1.0. Use small changes (0.1-0.2).\n\
- Add new traits only with strong evidence from multiple messages.\n\
- Dreams: max 3 active, 5 backlog. Prefer backlog before active.\n\
- Promote dreams only with repeated evidence or explicit confirmation.\n\
- Keep updates gentle and minimal.\n\n\
Conversation summary:\n{}\n\n\
Recent user messages:\n{}\n\n\
Return JSON only in this shape:\n\
{{\n  \"trait_updates\": [{{\"name\":\"assertiveness\",\"target_strength\":0.7,\"origin\":\"manual\",\"evidence\":\"...\"}}],\n\
  \"dream_updates\": [{{\"title\":\"explore ideas\",\"action\":\"add_backlog\",\"priority\":2,\"reason\":\"...\"}}]\n}}\n\
If no changes are needed, return {{\"trait_updates\":[],\"dream_updates\":[]}}.",
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
    cap_traits(context.state);
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
    if list.iter().any(|entry| entry.title == change.title) {
        update_dream_metadata(list, &change);
        return;
    }
    let mut entry = DreamEntry::new(change.title, change.priority, TraitOrigin::Inferred);
    entry.last_mention = Some(change.now.to_rfc3339());
    entry.progress_note = change.reason;
    list.push(entry);
}

fn update_dream_metadata(list: &mut [DreamEntry], change: &DreamChange<'_>) {
    if let Some(entry) = list.iter_mut().find(|entry| entry.title == change.title) {
        entry.priority = change.priority.max(1);
        entry.last_mention = Some(change.now.to_rfc3339());
        if let Some(note) = change.reason.as_ref()
            && !note.is_empty()
        {
            entry.progress_note = Some(note.to_string());
        }
    }
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
        let neutral = 0.5;
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

fn cap_traits(state: &mut IdentityState) {
    if state.traits.len() <= MAX_TRAITS {
        return;
    }
    state.traits.sort_by(|left, right| {
        right
            .strength
            .partial_cmp(&left.strength)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    state.traits.truncate(MAX_TRAITS);
}

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
    value.clamp(0.0, 1.0)
}

