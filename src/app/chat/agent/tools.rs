use crate::agents::openai_compat::{
    FunctionDefinition, ToolCallResponse, ToolDefinition,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Tool definitions that the LLM can use
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tool", rename_all = "snake_case")]
pub enum ToolCall {
    SearchNotes { query: String },
    SearchWeb { query: String },
    RetrieveMemories { query: String },
    CreateProject { name: String, description: String },
    SearchProjects { query: String },
    DeleteProject { name: String },
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool: String,
    pub result: String,
}

// -- Native tool calling (OpenAI-compatible API) --

/// Returns structured tool definitions for the OpenAI-compatible tools API
pub fn get_tool_definitions() -> Vec<ToolDefinition> {
    let query_params = json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "The search query"
            }
        },
        "required": ["query"]
    });

    let create_project_params = json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "The project name (e.g. Gardening)"
            },
            "description": {
                "type": "string",
                "description": "A brief description of what the project tracks"
            }
        },
        "required": ["name", "description"]
    });

    let name_params = json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "The project name to delete/archive"
            }
        },
        "required": ["name"]
    });

    vec![
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "search_notes".to_string(),
                description: "Search the user's Obsidian notes and vault for documents and written content".to_string(),
                parameters: query_params.clone(),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "search_web".to_string(),
                description: "Search the web for current events, recent news, or real-time information".to_string(),
                parameters: query_params.clone(),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "retrieve_memories".to_string(),
                description: "Search past conversation history and memory database. Use this when the user asks what you know or remember about them, or references something from a previous conversation. Pass the user's question as the query.".to_string(),
                parameters: query_params.clone(),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "create_project".to_string(),
                description: "Create a new project to track knowledge about a topic. Use when the user agrees to create a project. Requires name and description.".to_string(),
                parameters: create_project_params,
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "search_projects".to_string(),
                description: "Search your accumulated project knowledge base stored in Obsidian. Use when the user asks about a topic you've been tracking across conversations.".to_string(),
                parameters: query_params,
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "delete_project".to_string(),
                description: "Archive a project (moves to projects/archived/, nothing is deleted). Use when the user asks to remove or delete a project.".to_string(),
                parameters: name_params,
            },
        },
    ]
}

/// Converts native API tool call responses into internal ToolCall enums
pub fn parse_native_tool_calls(calls: &[ToolCallResponse]) -> Vec<ToolCall> {
    let mut tools = Vec::new();
    for call in calls {
        let name = call.function.name.as_str();
        match name {
            "search_notes" | "search_web" | "retrieve_memories" | "search_projects" => {
                if let Some(query) = extract_query_from_arguments(&call.function.arguments) {
                    match name {
                        "search_notes" => tools.push(ToolCall::SearchNotes { query }),
                        "search_web" => tools.push(ToolCall::SearchWeb { query }),
                        "retrieve_memories" => tools.push(ToolCall::RetrieveMemories { query }),
                        "search_projects" => tools.push(ToolCall::SearchProjects { query }),
                        _ => {}
                    }
                }
            }
            "create_project" => {
                if let Some((name_val, desc)) = extract_create_project_args(&call.function.arguments) {
                    tools.push(ToolCall::CreateProject { name: name_val, description: desc });
                }
            }
            "delete_project" => {
                if let Some(name_val) = extract_name_from_arguments(&call.function.arguments) {
                    tools.push(ToolCall::DeleteProject { name: name_val });
                }
            }
            _ => {} // Unknown tool, skip
        }
    }
    tools
}

/// Extracts the "query" field from a JSON arguments string
fn extract_query_from_arguments(arguments: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(arguments).ok()?;
    parsed
        .get("query")
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

/// Extracts "name" and "description" fields for create_project
fn extract_create_project_args(arguments: &str) -> Option<(String, String)> {
    let parsed: serde_json::Value = serde_json::from_str(arguments).ok()?;
    let name = parsed.get("name")?.as_str()?.to_string();
    let description = parsed.get("description")?.as_str()?.to_string();
    Some((name, description))
}

/// Extracts the "name" field from a JSON arguments string
fn extract_name_from_arguments(arguments: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(arguments).ok()?;
    parsed
        .get("name")
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

// -- Text-based tool calling (fallback for non-native models) --

/// Returns the tool schema to include in system prompts (fallback for non-native models)
pub fn get_tool_schema() -> String {
    r#"
AVAILABLE TOOLS (use when you need information you don't have):

1. search_notes: Search user's Obsidian notes/vault
   Format: {"tool":"search_notes","query":"what to search"}
   When to use: User asks about their notes, documents, or written content

2. search_web: Search the web for current/live information
   Format: {"tool":"search_web","query":"what to search"}
   When to use: User asks about current events, recent news, or real-time info

3. retrieve_memories: Search past conversation history
   Format: {"tool":"retrieve_memories","query":"what to recall"}
   When to use: User references something they said before ("what did I say about...", "do you remember when...")

4. create_project: Create a new knowledge project in Obsidian
   Format: {"tool":"create_project","name":"Project Name","description":"what the project tracks"}
   When to use: User agrees to create a project to track knowledge about a topic

5. search_projects: Search accumulated project knowledge
   Format: {"tool":"search_projects","query":"what to search"}
   When to use: User asks about a topic you've been tracking across conversations

6. delete_project: Archive a project (moves to archived folder, nothing is deleted)
   Format: {"tool":"delete_project","name":"Project Name"}
   When to use: User asks to remove or delete a project

CRITICAL RULES:
- If you need information, output ONLY the tool JSON and nothing else
- DO NOT add explanations or commentary with tool calls
- Tool calls must be the entire response: just {"tool":"...","query":"..."}
- After receiving tool results, then provide your answer
- If you can answer without tools, respond normally without any JSON
"#.trim().to_string()
}

/// Extracts tool calls from LLM response text (fallback parsing)
pub fn parse_tool_calls(response: &str) -> Vec<ToolCall> {
    let mut tools = Vec::new();

    // Look for JSON objects that match tool schema
    for line in response.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
            continue;
        }

        // Only parse if it looks like a tool call (has "tool" field)
        if !trimmed.contains("\"tool\"") {
            continue;
        }

        if let Ok(tool) = serde_json::from_str::<ToolCall>(trimmed) {
            tools.push(tool);
        }
    }

    tools
}

/// Checks if response text contains tool calls (fallback detection)
pub fn has_tool_calls(response: &str) -> bool {
    !parse_tool_calls(response).is_empty()
}

/// Formats tool results for feeding back to LLM
pub fn format_tool_results(results: &[ToolResult]) -> String {
    let mut output = String::from("=== TOOL RESULTS (use this information to answer the user) ===\n");
    for result in results {
        output.push_str(&format!("\n[{}]:\n{}\n", result.tool, result.result));
    }
    output.push_str("\n=== END TOOL RESULTS ===\n");
    output.push_str("Now provide your answer to the user based on the tool results above.");
    output
}

/// Execute a tool call and return the result
pub fn execute_tool(
    tool: &ToolCall,
    vault_name: &str,
    vault_path: &str,
    brave_key: &str,
    runtime: Option<&tokio::runtime::Runtime>,
) -> ToolResult {
    match tool {
        ToolCall::SearchNotes { query } => {
            let result = if vault_name.trim().is_empty() {
                "Obsidian vault not configured. Please set up your vault name in settings.".to_string()
            } else {
                match crate::services::obsidian::search_notes(vault_name, query, 5) {
                    Ok(notes) if !notes.is_empty() => {
                        if let Some(formatted) = crate::services::obsidian::format_obsidian_context("Notes", &notes) {
                            formatted
                        } else {
                            format!("Found {} notes but couldn't format them.", notes.len())
                        }
                    }
                    Ok(_) => {
                        format!("No notes found matching '{}'.", query)
                    }
                    Err(e) => {
                        format!("Error searching notes: {}", e)
                    }
                }
            };
            ToolResult {
                tool: "search_notes".to_string(),
                result,
            }
        }
        ToolCall::SearchWeb { query } => {
            let result = if brave_key.trim().is_empty() {
                "Web search not configured.".to_string()
            } else {
                let params = crate::agents::brave::BraveSearchParams::default();
                match crate::agents::brave::search(brave_key, query, &params) {
                    Ok(results) if !results.is_empty() => {
                        let formatted = crate::agents::brave::format_results_for_llm(&results);
                        format!("Search results for '{}':\n{}", query, formatted)
                    }
                    Ok(_) => format!("No search results found for: {}", query),
                    Err(_) => format!("Web search failed for: {}", query),
                }
            };
            ToolResult {
                tool: "search_web".to_string(),
                result,
            }
        }
        ToolCall::RetrieveMemories { query } => {
            // Create storage INSIDE block_on to avoid stale RocksDB lock issues
            // (previous connections may not have fully released their lock yet)
            let result = if let Some(rt) = runtime {
                let embeddings_config = crate::config::Config::load()
                    .map(|config| config.embeddings)
                    .unwrap_or_default();

                match rt.block_on(async {
                    let storage = crate::storage::StorageManager::new().await?;
                    crate::services::retrieval::retrieve_relevant_messages(
                        &storage,
                        query,
                        embeddings_config.max_retrieved_messages,
                        embeddings_config.similarity_threshold,
                    ).await
                }) {
                    Ok(messages) if !messages.is_empty() => {
                        let formatted: Vec<String> = messages.iter()
                            .map(|msg| format!("[{}] {}: {}", msg.timestamp, msg.role, msg.content))
                            .collect();
                        formatted.join("\n")
                    }
                    Ok(_) => format!("No relevant memories found for: {}", query),
                    Err(error) => format!("Memory retrieval error: {}", error),
                }
            } else {
                "Async runtime not available for memory retrieval.".to_string()
            };
            ToolResult {
                tool: "retrieve_memories".to_string(),
                result,
            }
        }
        ToolCall::CreateProject { name, description } => {
            let result = if vault_path.trim().is_empty() {
                "Obsidian vault path not configured. Set vault_path in config.toml.".to_string()
            } else {
                match crate::services::projects::create_project_file(vault_path, name, description) {
                    Ok(()) => {
                        // Clear topic mentions so the suggestion doesn't repeat
                        if let Some(rt) = runtime {
                            rt.block_on(async {
                                if let Ok(storage) = crate::storage::StorageManager::new().await {
                                    let _ = storage.clear_topic_mentions(&name.to_lowercase()).await;
                                }
                            });
                        }
                        format!("Project '{}' created in your Obsidian vault. I'll start tracking relevant information.", name)
                    }
                    Err(error) => format!("{}", error),
                }
            };
            ToolResult {
                tool: "create_project".to_string(),
                result,
            }
        }
        ToolCall::SearchProjects { query } => {
            let result = if vault_path.trim().is_empty() {
                "Obsidian vault path not configured.".to_string()
            } else {
                let project_names = crate::services::projects::list_project_names(vault_path)
                    .unwrap_or_default();
                match crate::services::projects::search_project_entries(vault_path, query, 20) {
                    Ok(results) => {
                        crate::services::projects::format_search_results(&results, &project_names)
                    }
                    Err(error) => format!("Project search error: {}", error),
                }
            };
            ToolResult {
                tool: "search_projects".to_string(),
                result,
            }
        }
        ToolCall::DeleteProject { name } => {
            let result = if vault_path.trim().is_empty() {
                "Obsidian vault path not configured.".to_string()
            } else {
                match crate::services::projects::archive_project(vault_path, name) {
                    Ok(()) => {
                        // Clear topic mentions so the suggestion doesn't re-trigger
                        if let Some(rt) = runtime {
                            rt.block_on(async {
                                if let Ok(storage) = crate::storage::StorageManager::new().await {
                                    let _ = storage.clear_topic_mentions(&name.to_lowercase()).await;
                                }
                            });
                        }
                        format!("Project '{}' has been archived. The file is still in your vault under projects/archived/ if you need it.", name)
                    }
                    Err(error) => format!("{}", error),
                }
            };
            ToolResult {
                tool: "delete_project".to_string(),
                result,
            }
        }
    }
}
