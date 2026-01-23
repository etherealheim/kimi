use crate::app::types::MessageRole;
use crate::app::App;
use crate::services::identity::{IdentityReflectionInput, IdentityReflectionJob};

impl App {
    pub(crate) fn maybe_spawn_identity_reflection(&self, summary: &str) {
        if !self.personality_enabled {
            return;
        }
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
