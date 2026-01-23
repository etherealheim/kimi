use crate::app::types::MessageRole;
use crate::app::App;
use crate::services::identity::{EmotionUpdateJob, TraitUpdateJob, IdentityReflectionInput, IdentityReflectionJob};

impl App {
    /// Spawns a background reflection job to update identity traits/dreams based on conversation.
    /// This runs after each conversation summary, independent of personality toggle.
    pub(crate) fn maybe_spawn_identity_reflection(&self, summary: &str) {
        let Some(manager) = self.agent_manager.clone() else {
            return;
        };
        let Some(agent) = self.current_agent.clone() else {
            return;
        };
        let input = IdentityReflectionInput {
            summary: summary.to_string(),
            recent_user_messages: self.recent_user_messages(),
        };
        let job = IdentityReflectionJob {
            manager,
            agent,
            input,
        };
        std::thread::spawn(move || {
            let _ = crate::services::identity::reflect_and_update_identity(job);
        });
    }
    
    /// Updates emotions and traits after each message exchange (user + assistant).
    /// Fast, lightweight updates that run per message without debounce.
    pub(crate) fn maybe_update_emotions(&self, assistant_response: &str) {
        let Some(manager) = self.agent_manager.clone() else {
            return;
        };
        let Some(agent) = self.current_agent.clone() else {
            return;
        };
        
        // Get last 2-3 exchanges (last user message + assistant response)
        let mut recent_messages = Vec::new();
        for message in self.chat_history.iter().rev().take(4) {
            recent_messages.push(format!("{}: {}", 
                match message.role {
                    MessageRole::User => "User",
                    MessageRole::Assistant => "Kimi",
                    MessageRole::System => "System",
                },
                message.content
            ));
        }
        recent_messages.push(format!("Kimi: {}", assistant_response));
        recent_messages.reverse();
        
        // Update emotions
        let emotion_job = EmotionUpdateJob {
            manager: manager.clone(),
            agent: agent.clone(),
            recent_messages: recent_messages.clone(),
        };
        
        std::thread::spawn(move || {
            let _ = crate::services::identity::update_emotions_fast(emotion_job);
        });
        
        // Update traits gradually (small changes per message)
        let trait_job = TraitUpdateJob {
            manager,
            agent,
            recent_messages,
        };
        
        std::thread::spawn(move || {
            let _ = crate::services::identity::update_traits_gradual(trait_job);
        });
    }

    fn recent_user_messages(&self) -> Vec<String> {
        let mut messages = self
            .chat_history
            .iter()
            .rev()
            .filter(|message| message.role == MessageRole::User)
            .take(8)
            .map(|message| message.content.clone())
            .collect::<Vec<_>>();
        messages.reverse();
        messages
    }
}

impl App {
    pub fn open_identity_view(&mut self) {
        self.mode = crate::app::AppMode::IdentityView;
    }

    pub fn close_identity_view(&mut self) {
        self.mode = crate::app::AppMode::PersonalitySelection;
    }
}
