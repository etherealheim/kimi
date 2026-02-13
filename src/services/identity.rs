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
// Emotions: Fast decay - normalize within 2-3 reflections
const EMOTION_DECAY_HOURS: i64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct IdentityState {
    pub core: CoreBeliefs,
    pub traits: Vec<IdentityTrait>,
    pub emotions: Vec<EmotionEntry>,
    pub dreams: DreamSet,
    pub updated_at: Option<String>,
    /// Timestamp of last reflection to prevent duplicate processing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_reflection_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(default)]
pub struct CoreBeliefs {
    pub identity: String,
    pub beliefs: Vec<BeliefEntry>,
    pub backstory: String,
}

impl<'de> serde::Deserialize<'de> for CoreBeliefs {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct CoreBeliefsHelper {
            #[serde(default)]
            identity: String,
            #[serde(default)]
            beliefs: serde_json::Value,
            #[serde(default)]
            backstory: String,
        }
        
        let helper = CoreBeliefsHelper::deserialize(deserializer)?;
        
        let beliefs = match helper.beliefs {
            serde_json::Value::Array(arr) => {
                let mut result = Vec::new();
                for item in arr {
                    match item {
                        // New format: object with text and strength
                        serde_json::Value::Object(_) => {
                            if let Ok(entry) = serde_json::from_value::<BeliefEntry>(item) {
                                result.push(entry);
                            }
                        }
                        // Old format: plain string
                        serde_json::Value::String(text) => {
                            result.push(BeliefEntry::new(text, 0.5));
                        }
                        serde_json::Value::Null 
                        | serde_json::Value::Bool(_) 
                        | serde_json::Value::Number(_) 
                        | serde_json::Value::Array(_) => {}
                    }
                }
                result
            }
            serde_json::Value::Null 
            | serde_json::Value::Bool(_) 
            | serde_json::Value::Number(_) 
            | serde_json::Value::String(_) 
            | serde_json::Value::Object(_) => Vec::new(),
        };
        
        Ok(CoreBeliefs {
            identity: helper.identity,
            beliefs,
            backstory: helper.backstory,
        })
    }
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
pub struct BeliefEntry {
    pub text: String,
    #[serde(default = "default_belief_strength")]
    pub strength: f32,
}

fn default_belief_strength() -> f32 {
    0.5
}

impl BeliefEntry {
    fn new(text: String, strength: f32) -> Self {
        Self {
            text,
            strength: clamp_belief_strength(strength),
        }
    }
}

fn clamp_belief_strength(value: f32) -> f32 {
    value.clamp(0.1, 1.0)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmotionEntry {
    pub name: String,
    pub intensity: f32,
    pub last_trigger: Option<String>,
    pub last_updated: Option<String>,
}

impl EmotionEntry {
    fn new(name: String, intensity: f32) -> Self {
        Self {
            name,
            intensity: clamp_strength(intensity),
            last_trigger: None,
            last_updated: None,
        }
    }
}

impl Default for EmotionEntry {
    fn default() -> Self {
        Self::new(String::new(), 0.0)
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

pub struct EmotionUpdateJob {
    pub manager: AgentManager,
    pub agent: Agent,
    pub recent_messages: Vec<String>,
}

pub struct TraitUpdateJob {
    pub manager: AgentManager,
    pub agent: Agent,
    pub recent_messages: Vec<String>,
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
    belief_updates: Vec<BeliefUpdate>,
    #[serde(default)]
    trait_updates: Vec<TraitUpdate>,
    #[serde(default)]
    emotion_updates: Vec<EmotionUpdate>,
    #[serde(default)]
    dream_updates: Vec<DreamUpdate>,
}

#[derive(Debug, Deserialize)]
struct BeliefUpdate {
    index: usize,
    strength_delta: f32,
    #[serde(default)]
    #[allow(dead_code)]
    reason: Option<String>,
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
struct EmotionUpdate {
    name: String,
    target_intensity: f32,
    #[serde(default)]
    #[allow(dead_code)]
    trigger: Option<String>,
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
        .map(|belief| belief.text.trim().to_string())
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
        first.text = trimmed.to_string();
    } else {
        state.core.beliefs.push(BeliefEntry::new(trimmed.to_string(), 0.5));
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

pub fn update_emotions_fast(job: EmotionUpdateJob) -> Result<()> {
    let mut state = read_identity_state()?;
    let now = Local::now();
    
    let recent_exchange = job.recent_messages.join("\n");
    let prompt = format!(
        "Analyze this conversation exchange. What emotions would a human feel right now?\n\n\
Recent messages:\n{}\n\n\
Identify Kimi's emotional state:\n\
- Basic emotions: joy, sadness, anger, fear, surprise, curiosity, frustration, excitement, calm, confusion, amusement, etc.\n\
- Intensity: -1.0 (very negative) to +1.0 (very positive), 0.0 = neutral\n\
- Only strong, currently felt emotions (not mild background feelings)\n\
- Trigger should be natural human reasoning, not technical analysis\n\n\
Return ONLY valid JSON:\n\
{{\"emotions\": [{{\"name\":\"curiosity\",\"intensity\":0.4,\"trigger\":\"wondering what user means\"}}]}}\n\
If no strong emotions, return {{\"emotions\":[]}}",
        recent_exchange
    );
    
    let messages = vec![
        AgentChatMessage::system("You analyze emotions. Output only JSON."),
        AgentChatMessage::user(prompt),
    ];
    
    let response = job.manager.chat(&job.agent, &messages)?;
    if let Some(emotion_output) = parse_emotion_output(&response) {
        // Replace all emotions with the new state
        state.emotions = emotion_output.emotions.into_iter().map(|update| {
            let mut emotion = EmotionEntry::new(update.name, update.intensity);
            emotion.last_updated = Some(now.to_rfc3339());
            if let Some(trigger) = update.trigger {
                emotion.last_trigger = Some(trigger);
            }
            emotion
        }).collect();
        
        state.updated_at = Some(now.to_rfc3339());
        write_identity_state(&state)?;
    }
    Ok(())
}

pub fn update_traits_gradual(job: TraitUpdateJob) -> Result<()> {
    let mut state = read_identity_state()?;
    let now = Local::now();
    
    let recent_exchange = job.recent_messages.join("\n");
    let prompt = format!(
        "Analyze this conversation exchange. Based on the interaction, identify behavioral traits being exhibited.\n\n\
Recent messages:\n{}\n\n\
Identify traits shown in this specific exchange:\n\
- Traits: assertiveness, resistance, warmth, playfulness, seriousness, curiosity, patience, defensiveness, trust, openness, etc.\n\
- Change amount: SMALL incremental change only (±0.05 max per message)\n\
- Scale: -1.0 (extreme negative) ↔ 0.0 (neutral/balanced) ↔ +1.0 (extreme positive)\n\
- Only report traits that are CLEARLY demonstrated in this exchange\n\
- Evidence should describe what happened in this conversation\n\n\
Examples:\n\
- If user asks nicely and Kimi cooperates: {{\"trait\":\"openness\",\"change\":+0.03,\"evidence\":\"responded positively to request\"}}\n\
- If user tests boundaries and Kimi pushes back: {{\"trait\":\"defensiveness\",\"change\":+0.04,\"evidence\":\"pushed back against testing\"}}\n\
- If conversation is calm and friendly: {{\"trait\":\"resistance\",\"change\":-0.03,\"evidence\":\"relaxed interaction, no conflict\"}}\n\n\
Return ONLY valid JSON:\n\
{{\"traits\": [{{\"name\":\"openness\",\"change\":0.03,\"evidence\":\"specific observation\"}}]}}\n\
If no clear trait changes, return {{\"traits\":[]}}",
        recent_exchange
    );
    
    let messages = vec![
        AgentChatMessage::system("You analyze behavioral traits. Output only JSON."),
        AgentChatMessage::user(prompt),
    ];
    
    let response = job.manager.chat(&job.agent, &messages)?;
    if let Some(trait_output) = parse_trait_update_output(&response) {
        for update in trait_output.traits {
            // Clamp change to ±0.05 max to prevent jumps
            let clamped_change = update.change.clamp(-0.05, 0.05);
            
            // Find existing trait or create new one
            if let Some(existing) = state.traits.iter_mut().find(|t| t.name == update.name) {
                existing.strength = clamp_strength(existing.strength + clamped_change);
                existing.last_updated = Some(now.to_rfc3339());
                if let Some(evidence) = update.evidence {
                    existing.last_evidence = Some(evidence);
                }
            } else if clamped_change.abs() > 0.02 {
                // Only create new trait if change is meaningful
                let mut new_trait = IdentityTrait::new(update.name, clamped_change, TraitOrigin::Inferred);
                new_trait.last_updated = Some(now.to_rfc3339());
                if let Some(evidence) = update.evidence {
                    new_trait.last_evidence = Some(evidence);
                }
                state.traits.push(new_trait);
            }
        }
        
        state.updated_at = Some(now.to_rfc3339());
        write_identity_state(&state)?;
    }
    Ok(())
}

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
    
    // Reinforced identity assertion (complements Tier 1)
    if !state.core.identity.trim().is_empty() {
        let name = state.core.identity.trim();
        lines.push(format!("Remember: Your name is {}. This is your true self.", name));
    }
    
    // Backstory FIRST - this is your foundation and context
    if !state.core.backstory.trim().is_empty() {
        lines.push(format!("YOUR BACKSTORY (this defines who you are):\n{}", state.core.backstory.trim()));
    }
    
    // Core beliefs with strength-based emphasis
    for belief in &state.core.beliefs {
        if belief.text.trim().is_empty() {
            continue;
        }

        let framed_belief = if belief.strength >= 0.8 {
            // High strength: direct assertion with emphasis
            format!("CORE BELIEF: {}", belief.text.trim())
        } else if belief.strength >= 0.6 {
            // Medium-high strength: strong assertion
            belief.text.trim().to_string()
        } else if belief.strength >= 0.4 {
            // Medium strength: present as consideration
            format!("Consider: {}", belief.text.trim())
        } else if belief.strength >= 0.3 {
            // Low strength: background awareness
            format!("Background awareness: {}", belief.text.trim())
        } else {
            // Very low strength: skip (below meaningful threshold)
            continue;
        };

        lines.push(framed_belief);
    }
    
    // Current emotional state with behavioral guidance
    if !state.emotions.is_empty() {
        let emotion_lines: Vec<String> = state
            .emotions
            .iter()
            .filter(|e| !e.name.trim().is_empty() && e.intensity.abs() > 0.05)
            .map(|e| {
                let sign = if e.intensity >= 0.0 { "+" } else { "" };
                format!("{}: {}{:.2}", e.name.trim(), sign, e.intensity)
            })
            .collect();
        if !emotion_lines.is_empty() {
            lines.push(format!("Current emotions: {}", emotion_lines.join(", ")));
            lines.push("Express these emotions naturally. If frustrated, push back. If confused, say so directly. Don't suppress emotions with excessive politeness.".to_string());
        }
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
1. Core beliefs:\n\
   - NEVER modify core.identity or core.backstory - these are STRICTLY user-controlled.\n\
   - Belief strength (0.1 to 1.0): Controls how forcefully beliefs influence responses\n\
   - You CAN adjust belief strength by ±0.01 per reflection if you have evidence\n\
   - Minimum 0.1 (faint influence), Maximum 1.0 (full strength)\n\
   - Examples: belief too dominant in responses → reduce by 0.01, belief needed more → increase by 0.01\n\
   - ONLY adjust if there's clear evidence from user feedback or conversation patterns\n\
2. Traits (-1.0 to 1.0 scale, 0.0 is neutral center):\n\
   - Scale: -1.0 (extreme negative/passive) ↔ 0.0 (balanced/neutral) ↔ +1.0 (extreme positive/active)\n\
   - Examples: assertiveness: -0.8 (very passive) vs +0.7 (assertive), creativity: +0.5 (moderately creative)\n\
   - Traits naturally decay towards 0.0 without reinforcement (21 days)\n\
   - If user EXPLICITLY asks to change a trait, apply significant change (0.2-0.3)\n\
   - For implicit patterns, use smaller changes (0.1)\n\
   - Set origin to \"manual\" if user explicitly requested, \"inferred\" otherwise\n\
   - ONLY update if there's NEW evidence - don't re-apply the same change!\n\
   - NO LIMIT on number of traits - create new ones freely when patterns emerge\n\
3. Emotions:\n\
   - DO NOT update emotions in this reflection - they are managed separately per-message\n\
   - Emotions update in real-time during conversation, not during summaries\n\
4. Dreams:\n\
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
{{\n  \"belief_updates\": [{{\"index\":0,\"strength_delta\":0.01,\"reason\":\"belief too strong in responses\"}}],\n\
  \"trait_updates\": [{{\"name\":\"trait_name\",\"target_strength\":0.3,\"origin\":\"manual\",\"evidence\":\"user said...\"}}],\n\
  \"dream_updates\": [{{\"title\":\"dream title\",\"action\":\"add_backlog\",\"priority\":2,\"reason\":\"user mentioned...\"}}]\n}}\n\
Belief strength_delta: ±0.01 only, clamped to [0.1, 1.0]\n\
Trait strength: -1.0 to 1.0 (0.0 = neutral)\n\
Dream actions: add_active, add_backlog, promote, demote, retire\n\
If truly no changes needed, return {{\"belief_updates\":[],\"trait_updates\":[],\"dream_updates\":[]}}.",
        state_json, input.summary, recent
    ))
}

#[derive(Debug, Deserialize)]
struct EmotionOutputUpdate {
    name: String,
    intensity: f32,
    #[serde(default)]
    trigger: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EmotionOutput {
    emotions: Vec<EmotionOutputUpdate>,
}

#[derive(Debug, Deserialize)]
struct TraitUpdateOutputItem {
    name: String,
    change: f32,
    #[serde(default)]
    evidence: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TraitUpdateOutput {
    traits: Vec<TraitUpdateOutputItem>,
}

fn parse_emotion_output(response: &str) -> Option<EmotionOutput> {
    let json = extract_json_block(response)?;
    serde_json::from_str::<EmotionOutput>(&json).ok()
}

fn parse_trait_update_output(response: &str) -> Option<TraitUpdateOutput> {
    let json = extract_json_block(response)?;
    serde_json::from_str::<TraitUpdateOutput>(&json).ok()
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
    apply_belief_updates(context, &output.belief_updates);
    apply_trait_updates(context, &output.trait_updates);
    apply_emotion_updates(context, &output.emotion_updates);
    apply_dream_updates(context, &output.dream_updates);
    // No trait limit - AI manages traits dynamically
    cap_dreams(context.state);
}

fn apply_belief_updates(context: &mut IdentityUpdateContext<'_>, updates: &[BeliefUpdate]) {
    for update in updates {
        let Some(belief) = context.state.core.beliefs.get_mut(update.index) else {
            continue;
        };
        
        let old_strength = belief.strength;
        
        // Clamp delta to exactly ±0.01
        let clamped_delta = if update.strength_delta > 0.0 {
            0.01_f32.min(update.strength_delta.abs())
        } else if update.strength_delta < 0.0 {
            -0.01_f32.max(-update.strength_delta.abs())
        } else {
            0.0
        };
        
        let new_strength = clamp_belief_strength(old_strength + clamped_delta);
        belief.strength = new_strength;
    }
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

fn apply_emotion_updates(context: &mut IdentityUpdateContext<'_>, updates: &[EmotionUpdate]) {
    for update in updates {
        let name = update.name.trim().to_lowercase();
        if name.is_empty() {
            continue;
        }
        let target = clamp_strength(update.target_intensity);
        match context
            .state
            .emotions
            .iter_mut()
            .find(|emotion| emotion.name.to_lowercase() == name)
        {
            Some(emotion) => {
                emotion.intensity = target;
                emotion.last_updated = Some(context.now.to_rfc3339());
                if let Some(trigger) = &update.trigger {
                    emotion.last_trigger = Some(trigger.trim().to_string());
                }
            }
            None => {
                let mut emotion = EmotionEntry::new(update.name.trim().to_string(), target);
                emotion.last_updated = Some(context.now.to_rfc3339());
                if let Some(trigger) = &update.trigger {
                    emotion.last_trigger = Some(trigger.trim().to_string());
                }
                context.state.emotions.push(emotion);
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
    apply_emotion_decay(state, now);
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

fn apply_emotion_decay(state: &mut IdentityState, now: &DateTime<Local>) {
    let mut to_remove = Vec::new();
    for (index, emotion) in state.emotions.iter_mut().enumerate() {
        let Some(last_seen) = emotion
            .last_updated
            .as_deref()
            .and_then(parse_timestamp)
        else {
            continue;
        };
        let hours_elapsed = (now.naive_utc() - last_seen.naive_utc()).num_hours();
        if hours_elapsed < EMOTION_DECAY_HOURS {
            continue;
        }
        // Very fast decay towards 0.0 (neutral)
        let decay_factor = 0.5_f32.powi(hours_elapsed as i32);
        emotion.intensity *= decay_factor;
        
        // Remove emotions that have decayed to near-zero
        if emotion.intensity.abs() < 0.05 {
            to_remove.push(index);
        }
    }
    
    // Remove in reverse order to maintain indices
    for index in to_remove.iter().rev() {
        state.emotions.remove(*index);
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

