use crate::agents::ChatMessage as AgentChatMessage;

const VERIFY_RESPONSE_PROMPT: &str = "Verify the response using only the provided context. Correct any statements not supported by the context. If nothing needs correction, return the original response. Respond in English. Output only the final response text.";

pub fn should_verify_response(system_context: &str) -> bool {
    let has_context_usage = system_context.contains("--- Memories ---")
        || system_context.contains("--- Conversation summaries ---")
        || system_context.contains("--- Obsidian");
    let has_search_context = system_context.contains("Brave search results for");
    has_context_usage || has_search_context
}

pub fn build_verification_messages(system_context: &str, response: &str) -> Vec<AgentChatMessage> {
    let prompt = format!(
        "Context:\n{}\n\nOriginal response:\n{}\n\n{}",
        system_context, response, VERIFY_RESPONSE_PROMPT
    );
    vec![
        AgentChatMessage::system("You verify responses against provided context. Respond in English only."),
        AgentChatMessage::user(&prompt),
    ]
}
