use crate::agents::ChatMessage as AgentChatMessage;
use crate::app::types::MessageRole;
use crate::app::{AgentEvent, App};
use color_eyre::Result;

impl App {
    pub(crate) fn parse_summary_pair(summary: &str) -> (String, String) {
        let mut short = String::new();
        let mut detailed = String::new();
        for line in summary.lines() {
            let trimmed = line.trim();
            if let Some(value) = trimmed.strip_prefix("Short:") {
                short = value.trim().to_string();
            } else if let Some(value) = trimmed.strip_prefix("Detailed:") {
                detailed = value.trim().to_string();
            }
        }

        if short.is_empty() {
            if let Some(first_line) = summary
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
            {
                short = first_line.to_string();
            } else {
                short = "Conversation".to_string();
            }
        }
        if detailed.is_empty() {
            detailed = short.clone();
        }

        if short.trim().eq_ignore_ascii_case("conversation") && detailed.len() > 20 {
            short = detailed.clone();
        }

        short = Self::clamp_summary_words(&short, 12);

        (short, detailed)
    }

    fn clamp_summary_words(summary: &str, max_words: usize) -> String {
        let words: Vec<&str> = summary.split_whitespace().collect();
        if words.len() <= max_words {
            return summary.to_string();
        }
        words[..max_words].join(" ")
    }

    /// Builds conversation context from recent messages for summary generation
    fn build_summary_context(&self) -> String {
        self.chat_history
            .iter()
            .filter(|message| message.role != MessageRole::System)
            .rev()
            .take(10)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|message| message.content.clone())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Spawns a background thread to generate conversation summary
    fn spawn_summary_generation_thread(
        agent: crate::agents::Agent,
        manager: crate::agents::AgentManager,
        context: String,
        agent_tx: std::sync::mpsc::Sender<AgentEvent>,
    ) {
        let summary_prompt = format!(
            "Generate two summaries for this conversation.\n\
Short: 7-12 words.\n\
Detailed: 2-3 sentences.\n\
Return only two lines in this exact format:\n\
Short: <summary>\n\
Detailed: <summary>\n\n\
Conversation: {}",
            context.chars().take(400).collect::<String>()
        );

        std::thread::spawn(move || {
            let messages = vec![
                AgentChatMessage::system(
                    "You create short and detailed conversation summaries. Follow the requested format exactly.",
                ),
                AgentChatMessage::user(&summary_prompt),
            ];
            let response = match manager.chat(&agent, &messages) {
                Ok(text) => text,
                Err(_) => "Short: Conversation\nDetailed: Conversation".to_string(),
            };
            let (short, detailed) = Self::parse_summary_pair(&response);
            let payload = format!("{}\n{}", short, detailed);
            let _ = agent_tx.send(AgentEvent::SummaryGenerated(payload));
        });
    }

    /// Spawns a background thread to extract persistent memories
    fn spawn_memory_extraction_thread(
        agent: crate::agents::Agent,
        manager: crate::agents::AgentManager,
        context: String,
        agent_tx: std::sync::mpsc::Sender<AgentEvent>,
    ) {
        let extraction_prompt = format!(
            "Extract persistent user memories from this conversation.\n\
Prefer specific entities over generic categories. If a title or proper noun is mentioned, include it.\n\
Examples: use \"finished Elden Ring\" instead of \"finishing games\".\n\
Return only blocks in the exact format below and nothing else.\n\
Use empty output if nothing is relevant.\n\
[context:likes]\n\
<value | context=general | source=explicit | confidence=high>\n\
[context:dislikes]\n\
<value | context=general | source=explicit | confidence=high>\n\
[context:location]\n\
<value | context=general | source=explicit | confidence=high>\n\
[context:timezone]\n\
<value | context=general | source=explicit | confidence=high>\n\
[context:tools]\n\
<value | context=general | source=explicit | confidence=high>\n\
[context:projects]\n\
<value | context=general | source=explicit | confidence=high>\n\
[context:topics]\n\
<value | context=general | source=explicit | confidence=high>\n\n\
Conversation: {}",
            context.chars().take(600).collect::<String>()
        );

        std::thread::spawn(move || {
            let messages = vec![
                AgentChatMessage::system(
                    "You extract structured user memory. Follow the format exactly.",
                ),
                AgentChatMessage::user(&extraction_prompt),
            ];
            let response = manager.chat(&agent, &messages).unwrap_or_default();
            let _ = agent_tx.send(AgentEvent::MemoryExtracted(response));
        });
    }

    pub(crate) fn queue_realtime_memory_extraction(
        &self,
        assistant_response: &str,
    ) -> Result<()> {
        let Some(context) = self.build_realtime_memory_context(assistant_response) else {
            return Ok(());
        };
        let (agent, manager, agent_tx) = self.get_agent_chat_dependencies()?;
        let job = MemoryExtractionJob {
            agent,
            manager,
            context,
            agent_tx,
        };
        Self::spawn_realtime_memory_extraction_thread(job);
        Ok(())
    }

    fn build_realtime_memory_context(&self, assistant_response: &str) -> Option<String> {
        let user_message = self.last_user_message_content()?;
        let trimmed_user = user_message.trim();
        if trimmed_user.is_empty() {
            return None;
        }
        let trimmed_assistant = assistant_response.trim();
        let context = format!(
            "User: {}\nAssistant: {}",
            trimmed_user,
            trimmed_assistant
        );
        Some(context)
    }

    fn spawn_realtime_memory_extraction_thread(job: MemoryExtractionJob) {
        let extraction_prompt = format!(
            "Decide if the user's latest message contains durable personal facts worth saving.\n\
Only capture stable, user-specific details (preferences, tools, projects, topics, location, timezone).\n\
Prefer specific entities over generic categories. If a title or proper noun is mentioned, include it.\n\
Examples: use \"finished Elden Ring\" instead of \"finishing games\".\n\
If nothing is worth saving, return empty output.\n\
Return only blocks in the exact format below and nothing else.\n\
[context:likes]\n\
<value | context=general | source=explicit | confidence=high>\n\
[context:dislikes]\n\
<value | context=general | source=explicit | confidence=high>\n\
[context:location]\n\
<value | context=general | source=explicit | confidence=high>\n\
[context:timezone]\n\
<value | context=general | source=explicit | confidence=high>\n\
[context:tools]\n\
<value | context=general | source=explicit | confidence=high>\n\
[context:projects]\n\
<value | context=general | source=explicit | confidence=high>\n\
[context:topics]\n\
<value | context=general | source=explicit | confidence=high>\n\n\
Conversation: {}",
            job.context.chars().take(400).collect::<String>()
        );

        std::thread::spawn(move || {
            let messages = vec![
                AgentChatMessage::system(
                    "You extract structured user memory. Follow the format exactly.",
                ),
                AgentChatMessage::user(&extraction_prompt),
            ];
            let response = job.manager.chat(&job.agent, &messages).unwrap_or_default();
            let _ = job
                .agent_tx
                .send(AgentEvent::MemoryExtracted(response));
        });
    }

    pub fn exit_chat_to_history(&mut self) -> Result<()> {
        if self.chat_history.is_empty() {
            self.open_history();
            return Ok(());
        }

        self.is_generating_summary = true;
        self.summary_active = true;

        let context = self.build_summary_context();
        let (agent, manager, agent_tx) = self.get_agent_chat_dependencies()?;

        Self::spawn_summary_generation_thread(
            agent.clone(),
            manager.clone(),
            context.clone(),
            agent_tx.clone(),
        );
        Self::spawn_memory_extraction_thread(agent, manager, context, agent_tx);

        Ok(())
    }
}

struct MemoryExtractionJob {
    agent: crate::agents::Agent,
    manager: crate::agents::AgentManager,
    context: String,
    agent_tx: std::sync::mpsc::Sender<AgentEvent>,
}
