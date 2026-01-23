use color_eyre::Result;
use std::collections::HashMap;
use crate::storage::{RetrievedMessage, RetrievalSource, StorageManager};

// Debug logging (disabled in production)
#[allow(unused)]
fn debug_log(_msg: &str) {
    // Uncomment to enable debug logging:
    // use std::io::Write;
    // if let Ok(mut file) = std::fs::OpenOptions::new()
    //     .create(true)
    //     .append(true)
    //     .open("/tmp/kimi-retrieval.log")
    // {
    //     let now = chrono::Local::now().format("%H:%M:%S%.3f");
    //     let _ = writeln!(file, "[{}] {}", now, _msg);
    // }
}

const EMBEDDING_BACKFILL_LIMIT: usize = 50;
const RRF_K: f32 = 60.0;
const RECENT_USER_LIMIT: usize = 50;
/// Number of messages missing embeddings that triggers opportunistic backfill
const BACKFILL_THRESHOLD: usize = 10;

/// Retrieves relevant messages from storage based on semantic similarity
pub async fn retrieve_relevant_messages(
    storage: &StorageManager,
    query: &str,
    limit: usize,
    similarity_threshold: f32,
) -> Result<Vec<RetrievedMessage>> {
    debug_log(&format!("=== retrieve_relevant_messages called for: '{}' ===", query));
    
    // Debug: check embedding stats
    if let Ok((total, with_embedding)) = storage.get_embedding_stats().await {
        debug_log(&format!("DB: {} messages total, {} with embeddings", total, with_embedding));
    }

    let query_embedding = match crate::services::embeddings::generate_embedding(query).await {
        Ok(embedding) => {
            debug_log(&format!("Query embedding OK (dim={})", embedding.len()));
            Some(embedding)
        }
        Err(error) => {
            debug_log(&format!("Query embedding FAILED: {}", error));
            None
        }
    };

    let should_backfill = query_embedding.is_some();
    let missing_count = if should_backfill {
        storage.count_messages_missing_embeddings().await.unwrap_or(0)
    } else {
        0
    };
    debug_log(&format!("Missing embeddings: {}", missing_count));
    
    if should_backfill && missing_count >= BACKFILL_THRESHOLD {
        debug_log("Running backfill...");
        if let Ok(count) = backfill_missing_embeddings(storage).await {
            debug_log(&format!("Backfilled {} messages", count));
        }
    }

    let mut dense_results = if let Some(embedding) = &query_embedding {
        let results = storage.search_similar_messages(embedding.clone(), limit).await?;
        debug_log(&format!("Dense search: {} results", results.len()));
        for result in &results {
            debug_log(&format!(
                "  sim={:.3} {}",
                result.similarity,
                result.content.chars().take(60).collect::<String>()
            ));
        }
        results
    } else {
        Vec::new()
    };
    
    if should_backfill && dense_results.is_empty() && missing_count > 0 {
        debug_log("Retry after backfill...");
        let _ = backfill_missing_embeddings(storage).await;
        if let Some(embedding) = &query_embedding {
            dense_results = storage.search_similar_messages(embedding.clone(), limit).await?;
            debug_log(&format!("Retry got {} results", dense_results.len()));
        }
    }
    
    let sparse_results = match build_keyword_query(query) {
        Some(keyword_query) => {
            debug_log(&format!("Keyword query: '{}'", keyword_query));
            match storage.search_keyword_messages(&keyword_query, limit).await {
                Ok(results) => {
                    debug_log(&format!("Sparse search: {} results", results.len()));
                    results
                }
                Err(e) => {
                    debug_log(&format!("Sparse search ERROR: {}", e));
                    Vec::new() // Continue without sparse results
                }
            }
        }
        None => {
            debug_log("No keyword query (all stopwords)");
            Vec::new()
        }
    };
    let mut fused_results = fuse_results(dense_results, sparse_results, limit);
    debug_log(&format!("Fused: {} results", fused_results.len()));

    // For profile queries, ALWAYS check heuristic fallback since vector search
    // often returns similar questions rather than actual preference statements
    let is_profile = is_profile_query(query);
    debug_log(&format!("is_profile_query: {}", is_profile));
    
    if is_profile {
        debug_log("Profile query - checking heuristic fallback...");
        let heuristic_results = build_profile_fallback(storage).await?;
        debug_log(&format!("Heuristic: {} candidates", heuristic_results.len()));
        for result in &heuristic_results {
            debug_log(&format!("  heuristic: {}", result.content.chars().take(60).collect::<String>()));
        }
        
        // For profile queries, prioritize heuristic results (actual "i like X" statements)
        // over vector similarity results (which might be other questions)
        if !heuristic_results.is_empty() {
            // Filter fused results to only keep preference statements, then add heuristic
            let preference_fused: Vec<_> = fused_results
                .into_iter()
                .filter(|msg| is_profile_fact_candidate(&msg.content))
                .collect();
            debug_log(&format!("Fused after preference filter: {}", preference_fused.len()));
            fused_results = merge_heuristic_results(preference_fused, heuristic_results, limit);
            debug_log(&format!("After merge: {} results", fused_results.len()));
        } else if fused_results.is_empty() {
            fused_results = heuristic_results;
        }
    }

    // Filter out low similarity results
    let filtered: Vec<_> = fused_results
        .into_iter()
        .filter(|msg| msg.source != RetrievalSource::Dense || msg.similarity > similarity_threshold)
        .collect();
    debug_log(&format!(
        "After threshold ({:.2}): {} results",
        similarity_threshold, filtered.len()
    ));
    
    for result in &filtered {
        debug_log(&format!("  FINAL: src={:?} sim={:.3} '{}'", result.source, result.similarity, result.content.chars().take(50).collect::<String>()));
    }
    
    debug_log(&format!("=== Returning {} results ===", filtered.len()));
    Ok(filtered)
}

/// Maximum character length for embeddings (to avoid context length errors)
const MAX_EMBEDDING_LENGTH: usize = 2000;

/// Generates and returns an embedding for a message
pub async fn generate_message_embedding(content: &str) -> Result<Option<Vec<f32>>> {
    let trimmed = content.trim();
    
    // Skip embedding for very short messages
    if trimmed.len() < 10 {
        return Ok(None);
    }
    
    // Truncate if too long to avoid context length errors
    let embedding_text = if trimmed.len() > MAX_EMBEDDING_LENGTH {
        let truncated = &trimmed[..MAX_EMBEDDING_LENGTH];
        // Try to truncate at word boundary
        if let Some(last_space) = truncated.rfind(' ') {
            &trimmed[..last_space]
        } else {
            truncated
        }
    } else {
        trimmed
    };
    
    match crate::services::embeddings::generate_embedding(embedding_text).await {
        Ok(embedding) => Ok(Some(embedding)),
        Err(error) => {
            // Log error but don't fail the entire operation
            eprintln!("Warning: Failed to generate embedding: {}", error);
            Ok(None)
        }
    }
}

async fn backfill_missing_embeddings(storage: &StorageManager) -> Result<usize> {
    let candidates = storage
        .load_messages_missing_embeddings(EMBEDDING_BACKFILL_LIMIT)
        .await?;
    let mut updated = 0;
    for candidate in candidates {
        if let Some(embedding) = generate_message_embedding(&candidate.content).await? {
            storage.update_message_embedding_by_id(candidate.id, embedding).await?;
            updated += 1;
        }
    }
    Ok(updated)
}

fn fuse_results(
    dense_results: Vec<RetrievedMessage>,
    sparse_results: Vec<RetrievedMessage>,
    limit: usize,
) -> Vec<RetrievedMessage> {
    let mut fused: HashMap<String, RetrievedMessage> = HashMap::new();
    let mut dense_ranks: HashMap<String, usize> = HashMap::new();
    let mut sparse_ranks: HashMap<String, usize> = HashMap::new();

    for (index, result) in dense_results.into_iter().enumerate() {
        let key = result_key(&result);
        dense_ranks.insert(key.clone(), index + 1);
        fused.entry(key).or_insert(result);
    }

    for (index, result) in sparse_results.into_iter().enumerate() {
        let key = result_key(&result);
        sparse_ranks.insert(key.clone(), index + 1);
        fused
            .entry(key.clone())
            .and_modify(|entry| {
                entry.source = RetrievalSource::Hybrid;
            })
            .or_insert(result);
    }

    let mut results: Vec<RetrievedMessage> = fused
        .into_iter()
        .map(|(key, mut entry)| {
            let dense_rank = dense_ranks.get(&key).copied();
            let sparse_rank = sparse_ranks.get(&key).copied();
            let score = rrf_score(dense_rank) + rrf_score(sparse_rank);
            entry.score = score;
            entry
        })
        .collect();

    results.sort_by(|left, right| right.score.total_cmp(&left.score));
    results.truncate(limit);
    results
}

fn rrf_score(rank: Option<usize>) -> f32 {
    rank.map_or(0.0, |value| 1.0 / (RRF_K + value as f32))
}

fn result_key(result: &RetrievedMessage) -> String {
    format!("{}:{}:{}", result.role, result.timestamp, result.content)
}

fn merge_heuristic_results(
    mut current: Vec<RetrievedMessage>,
    heuristic_results: Vec<RetrievedMessage>,
    limit: usize,
) -> Vec<RetrievedMessage> {
    for result in heuristic_results {
        if current.len() >= limit {
            break;
        }
        current.push(result);
    }
    current
}

fn build_keyword_query(query: &str) -> Option<String> {
    let tokens = tokenize_query(query);
    let filtered: Vec<String> = tokens
        .into_iter()
        .filter(|token| !is_stopword(token))
        .collect();
    if filtered.is_empty() {
        None
    } else {
        Some(filtered.join(" OR "))
    }
}

fn tokenize_query(query: &str) -> Vec<String> {
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

fn is_stopword(token: &str) -> bool {
    matches!(
        token,
        "a" | "an" | "the" | "and" | "or" | "but" | "if" | "then" | "else" | "about" | "to"
            | "of" | "with" | "by" | "from" | "up" | "is" | "are" | "was" | "were" | "be"
            | "been" | "being" | "have" | "has" | "had" | "do" | "does" | "did" | "can" | "could"
            | "will" | "would" | "should" | "may" | "might" | "must" | "i" | "you" | "he"
            | "she" | "it" | "we" | "they" | "my" | "your" | "his" | "her" | "its" | "our"
            | "their" | "what" | "when" | "where" | "why" | "how" | "which" | "who" | "whom"
            | "me" | "know"
    )
}

pub fn is_profile_query(query: &str) -> bool {
    let lowered = query.to_lowercase();
    
    // Check for direct profile triggers
    let profile_triggers = [
        "about me",
        "who am i",
        "what do you know about me",
        "my profile",
        "my preferences",
        "what do i like",
        "what do i love",
        "what do i prefer",
        "do i like",
        "do i love",
        "what did i say",
        "what did i tell",
        "what did i mention",
        "what have i said",
        "my favorite",
        "my favourite",
        "you know about me",
        "you know that i",
        "told you",
        "i mentioned",
        // Additional patterns for rephrased queries
        "think i like",
        "think i love",
        "think i prefer",
        "guess i like",
        "guess i love",
        "believe i like",
        "remember about me",
        "recall about me",
        "know i like",
        "know i love",
        "know about my",
    ];
    if profile_triggers.iter().any(|trigger| lowered.contains(trigger)) {
        return true;
    }
    
    // Pattern: contains "i like/love/prefer" as a question about user preferences
    let preference_words = ["like", "love", "prefer", "favorite", "favourite"];
    let question_indicators = ["what", "do", "which", "any"];
    let has_preference = preference_words.iter().any(|word| lowered.contains(word));
    let has_question = question_indicators.iter().any(|word| lowered.starts_with(word));
    let about_user = lowered.contains(" i ") || lowered.contains(" my ");
    
    has_preference && has_question && about_user
}

async fn build_profile_fallback(
    storage: &StorageManager,
) -> Result<Vec<RetrievedMessage>> {
    let messages = storage.load_recent_user_messages(RECENT_USER_LIMIT).await?;
    let mut results = Vec::new();
    for message in messages {
        if is_profile_fact_candidate(&message.content) {
            results.push(RetrievedMessage {
                content: message.content,
                role: message.role,
                timestamp: message.timestamp,
                similarity: 0.0,
                score: 0.01,
                source: RetrievalSource::Heuristic,
            });
        }
    }
    Ok(results)
}

fn is_profile_fact_candidate(content: &str) -> bool {
    let lowered = content.to_lowercase();
    lowered.contains("i am ")
        || lowered.contains("i'm ")
        || lowered.contains("my name ")
        || lowered.contains("i live ")
        || lowered.contains("i like ")
        || lowered.contains("i love ")
        || lowered.contains("i prefer ")
        || lowered.contains("my favorite ")
        || lowered.contains("my favourite ")
        || lowered.contains("my job ")
        || lowered.contains("i work ")
}
