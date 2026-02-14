use color_eyre::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use surrealdb::engine::local::{Db, RocksDb};
use surrealdb::sql::Thing;
use surrealdb::Surreal;

/// Summary of a saved conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub id: String,
    pub agent_name: String,
    pub summary: Option<String>,
    #[allow(dead_code)]
    pub detailed_summary: Option<String>,
    pub created_at: String,
}

/// A stored message from conversation history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub role: String,
    pub content: String,
    pub timestamp: String,
    pub display_name: Option<String>,
}

/// Message data for persistence
#[derive(Debug, Clone)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
    pub timestamp: String,
    pub display_name: Option<String>,
}

/// A conversation with its messages, used for date-range recall
#[derive(Debug, Clone)]
pub struct ConversationWithMessages {
    pub created_at: String,
    pub messages: Vec<StoredMessage>,
}

/// Data for saving a new conversation
pub struct ConversationData<'a> {
    pub agent_name: &'a str,
    pub summary: Option<&'a str>,
    pub detailed_summary: Option<&'a str>,
    pub messages: &'a [ConversationMessage],
}

impl<'a> ConversationData<'a> {
    /// Creates new conversation data
    pub fn new(agent_name: &'a str, messages: &'a [ConversationMessage]) -> Self {
        Self {
            agent_name,
            summary: None,
            detailed_summary: None,
            messages,
        }
    }

    /// Sets the conversation summary
    pub fn with_summary(mut self, summary: &'a str) -> Self {
        self.summary = Some(summary);
        self
    }

    pub fn with_detailed_summary(mut self, summary: &'a str) -> Self {
        self.detailed_summary = Some(summary);
        self
    }
}

/// Retrieved message with fused relevance score
#[derive(Debug, Clone)]
pub struct RetrievedMessage {
    pub content: String,
    pub role: String,
    pub timestamp: String,
    pub similarity: f32,
    pub score: f32,
    pub source: RetrievalSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetrievalSource {
    Dense,
    Sparse,
    Hybrid,
    Heuristic,
}

/// Message embedding update payload
pub struct MessageEmbeddingUpdate<'a> {
    pub conversation_id: &'a str,
    pub role: &'a str,
    pub content: &'a str,
    pub timestamp: &'a str,
    pub display_name: Option<&'a str>,
    pub embedding: Option<Vec<f32>>,
}

/// Internal message record for SurrealDB
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MessageRecord {
    id: Option<surrealdb::sql::Thing>,
    conversation: Thing,
    role: String,
    content: String,
    embedding: Option<Vec<f32>>,
    timestamp: String,
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MessageEmbeddingCandidate {
    pub id: surrealdb::sql::Thing,
    pub content: String,
}

/// Internal conversation record for SurrealDB
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConversationRecord {
    id: Option<surrealdb::sql::Thing>,
    agent_name: String,
    summary: Option<String>,
    detailed_summary: Option<String>,
    created_at: String,
    updated_at: String,
}

/// Manages persistent storage of conversations using SurrealDB
#[derive(Clone)]
pub struct StorageManager {
    db: Surreal<Db>,
}

impl StorageManager {
    /// Creates a new storage manager and initializes the database
    pub async fn new() -> Result<Self> {
        let project_data_dir = Self::project_data_dir()?;
        std::fs::create_dir_all(&project_data_dir)?;
        let db_path = project_data_dir.join("kimi.db");

        let db = Surreal::new::<RocksDb>(db_path).await?;
        db.use_ns("kimi").use_db("main").await?;

        let manager = Self { db };
        manager.init_db().await?;

        Ok(manager)
    }

    async fn init_db(&self) -> Result<()> {
        // Define conversation table
        self.db.query("
            DEFINE TABLE IF NOT EXISTS conversation SCHEMAFULL;
            DEFINE FIELD agent_name ON conversation TYPE string;
            DEFINE FIELD summary ON conversation TYPE option<string>;
            DEFINE FIELD detailed_summary ON conversation TYPE option<string>;
            DEFINE FIELD created_at ON conversation TYPE string;
            DEFINE FIELD updated_at ON conversation TYPE string;
        ").await?;

        // Define message table with embedding field
        self.db.query("
            DEFINE ANALYZER IF NOT EXISTS content_analyzer TOKENIZERS class FILTERS lowercase;

            DEFINE TABLE IF NOT EXISTS message SCHEMAFULL;
            DEFINE FIELD conversation ON message TYPE record<conversation>;
            DEFINE FIELD role ON message TYPE string;
            DEFINE FIELD content ON message TYPE string;
            DEFINE FIELD embedding ON message TYPE option<array<float>>;
            DEFINE FIELD timestamp ON message TYPE string;
            DEFINE FIELD display_name ON message TYPE option<string>;

            DEFINE INDEX IF NOT EXISTS idx_msg_embedding ON message
                FIELDS embedding MTREE DIMENSION 1024 DIST COSINE;
            DEFINE INDEX IF NOT EXISTS idx_msg_content ON message
                FIELDS content SEARCH ANALYZER content_analyzer BM25;
        ").await?;

        // Define topic_mention table for project topic tracking
        self.db.query("
            DEFINE TABLE IF NOT EXISTS topic_mention SCHEMAFULL;
            DEFINE FIELD topic ON topic_mention TYPE string;
            DEFINE FIELD conversation_id ON topic_mention TYPE string;
            DEFINE FIELD created_at ON topic_mention TYPE string;
        ").await?;

        Ok(())
    }

    fn project_data_dir() -> Result<PathBuf> {
        let current_dir = std::env::current_dir()?;
        Ok(current_dir.join("data"))
    }

    fn normalize_conversation_id(id: &str) -> &str {
        id.strip_prefix("conversation:").unwrap_or(id)
    }

    fn conversation_ref(id: &str) -> Thing {
        let normalized_id = Self::normalize_conversation_id(id);
        Thing::from(("conversation", normalized_id))
    }

    /// Saves a conversation with messages to the database
    pub async fn save_conversation(&self, data: ConversationData<'_>) -> Result<String> {
        let now = chrono::Local::now().to_rfc3339();

        let conv: Option<ConversationRecord> = self.db
            .create("conversation")
            .content(ConversationRecord {
                id: None,
                agent_name: data.agent_name.to_string(),
                summary: data.summary.map(str::to_string),
                detailed_summary: data.detailed_summary.map(str::to_string),
                created_at: now.clone(),
                updated_at: now,
            })
            .await?;

        let conversation_ref = conv
            .and_then(|c| c.id)
            .ok_or_else(|| color_eyre::eyre::eyre!("Failed to create conversation"))?;
        let conversation_id = conversation_ref.to_string();

        // Save messages without embeddings initially
        for message in data.messages {
            let _: Option<MessageRecord> = self.db
                .create("message")
                .content(MessageRecord {
                    id: None,
                    conversation: conversation_ref.clone(),
                    role: message.role.clone(),
                    content: message.content.clone(),
                    embedding: None,
                    timestamp: message.timestamp.clone(),
                    display_name: message.display_name.clone(),
                })
                .await?;
        }

        Ok(conversation_id)
    }

    /// Updates embedding for an existing message
    pub async fn update_message_embedding(
        &self,
        update: MessageEmbeddingUpdate<'_>,
    ) -> Result<()> {
        let Some(embedding) = update.embedding else {
            return Ok(());
        };
        let conversation_ref = Self::conversation_ref(update.conversation_id);
        let role = update.role.to_string();
        let content = update.content.to_string();
        let timestamp = update.timestamp.to_string();

        // Use IS NULL check for display_name since NULL = NULL returns NULL, not TRUE
        let query = if update.display_name.is_some() {
            "UPDATE message
             SET embedding = $embedding
             WHERE conversation = $conv_id
               AND role = $role
               AND content = $content
               AND timestamp = $timestamp
               AND display_name = $display_name"
        } else {
            "UPDATE message
             SET embedding = $embedding
             WHERE conversation = $conv_id
               AND role = $role
               AND content = $content
               AND timestamp = $timestamp
               AND display_name IS NONE"
        };

        let mut query_builder = self.db.query(query)
            .bind(("embedding", embedding))
            .bind(("conv_id", conversation_ref))
            .bind(("role", role))
            .bind(("content", content))
            .bind(("timestamp", timestamp));

        if let Some(name) = update.display_name {
            query_builder = query_builder.bind(("display_name", name.to_string()));
        }

        let _ = query_builder.await?;
        Ok(())
    }

    pub async fn update_message_embedding_by_id(
        &self,
        id: surrealdb::sql::Thing,
        embedding: Vec<f32>,
    ) -> Result<()> {
        let _ = self.db
            .query("UPDATE $id SET embedding = $embedding")
            .bind(("id", id))
            .bind(("embedding", embedding))
            .await?;
        Ok(())
    }

    pub async fn load_messages_missing_embeddings(
        &self,
        limit: usize,
    ) -> Result<Vec<MessageEmbeddingCandidate>> {
        let mut response = self.db.query("
            SELECT id, content
            FROM message
            WHERE embedding IS NONE
            ORDER BY timestamp ASC
            LIMIT $limit
        ")
        .bind(("limit", limit))
        .await?;

        let results: Vec<MessageEmbeddingCandidate> = response.take(0)?;
        Ok(results)
    }

    /// Returns count of messages missing embeddings (for opportunistic backfill)
    pub async fn count_messages_missing_embeddings(&self) -> Result<usize> {
        #[derive(Debug, Deserialize)]
        struct CountResult {
            count: usize,
        }

        let mut response = self.db.query("
            SELECT count() AS count
            FROM message
            WHERE embedding IS NONE
            GROUP ALL
        ").await?;

        let results: Vec<CountResult> = response.take(0)?;
        Ok(results.first().map_or(0, |entry| entry.count))
    }

    /// Returns total message count and count with embeddings for debugging
    pub async fn get_embedding_stats(&self) -> Result<(usize, usize)> {
        #[derive(Debug, Deserialize)]
        struct CountResult {
            count: usize,
        }

        let mut total_response = self.db.query("
            SELECT count() AS count FROM message GROUP ALL
        ").await?;
        let total_results: Vec<CountResult> = total_response.take(0)?;
        let total = total_results.first().map_or(0, |entry| entry.count);

        let mut with_embedding_response = self.db.query("
            SELECT count() AS count FROM message WHERE embedding IS NOT NONE GROUP ALL
        ").await?;
        let with_results: Vec<CountResult> = with_embedding_response.take(0)?;
        let with_embedding = with_results.first().map_or(0, |entry| entry.count);

        Ok((total, with_embedding))
    }

    /// Searches for similar messages using vector similarity
    pub async fn search_similar_messages(
        &self,
        query_embedding: Vec<f32>,
        limit: usize,
    ) -> Result<Vec<RetrievedMessage>> {
        #[derive(Debug, Deserialize)]
        struct SearchResult {
            content: String,
            role: String,
            timestamp: String,
            similarity: f32,
        }

        let mut response = self.db.query("
            SELECT 
                content,
                role,
                timestamp,
                vector::similarity::cosine(embedding, $query_embedding) AS similarity
            FROM message
            WHERE embedding IS NOT NONE
            ORDER BY similarity DESC
            LIMIT $limit
        ")
        .bind(("query_embedding", query_embedding))
        .bind(("limit", limit))
        .await?;

        let results: Vec<SearchResult> = response.take(0)?;

        Ok(results
            .into_iter()
            .map(|r| RetrievedMessage {
                content: r.content,
                role: r.role,
                timestamp: r.timestamp,
                similarity: r.similarity,
                score: r.similarity,
                source: RetrievalSource::Dense,
            })
            .collect())
    }

    pub async fn search_keyword_messages(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<RetrievedMessage>> {
        #[derive(Debug, Deserialize)]
        struct SearchResult {
            content: String,
            role: String,
            timestamp: String,
            score: f32,
        }

        let query_string = query.to_string();
        let mut response = self.db.query("
            SELECT
                content,
                role,
                timestamp,
                search::score(1) AS score
            FROM message
            WHERE content @@ $query
            ORDER BY score DESC
            LIMIT $limit
        ")
        .bind(("query", query_string))
        .bind(("limit", limit))
        .await?;

        let results: Vec<SearchResult> = response.take(0)?;

        Ok(results
            .into_iter()
            .map(|r| RetrievedMessage {
                content: r.content,
                role: r.role,
                timestamp: r.timestamp,
                similarity: 0.0,
                score: r.score,
                source: RetrievalSource::Sparse,
            })
            .collect())
    }

    pub async fn load_recent_user_messages(
        &self,
        limit: usize,
    ) -> Result<Vec<StoredMessage>> {
        let mut response = self.db.query("
            SELECT role, content, timestamp, display_name
            FROM message
            WHERE role = \"User\"
            ORDER BY timestamp DESC
            LIMIT $limit
        ")
        .bind(("limit", limit))
        .await?;

        let messages: Vec<StoredMessage> = response.take(0)?;
        Ok(messages)
    }

    #[allow(dead_code, unused_variables)]
    async fn message_count_for_conversation(&self, conversation_id: &Thing) -> Result<usize> {
        #[derive(Debug, Deserialize)]
        struct MessageCount {
            count: usize,
        }

        let mut response = self.db.query("
            SELECT count() AS count
            FROM message
            WHERE conversation = $conv_id
              AND role != \"System\"
            GROUP ALL
        ")
        .bind(("conv_id", conversation_id.clone()))
        .await?;

        let counts: Vec<MessageCount> = response.take(0)?;
        Ok(counts.first().map_or(0, |entry| entry.count))
    }

    /// Loads all conversation summaries from the database
    pub async fn load_conversations(&self) -> Result<Vec<ConversationSummary>> {
        self.load_conversations_with_limit(20).await
    }

    pub async fn load_conversations_with_limit(&self, limit: usize) -> Result<Vec<ConversationSummary>> {
        #[derive(Debug, Deserialize)]
        struct ConvRow {
            id: surrealdb::sql::Thing,
            agent_name: String,
            summary: Option<String>,
            detailed_summary: Option<String>,
            created_at: String,
        }

        let query = format!("
            SELECT
                id,
                agent_name,
                summary,
                detailed_summary,
                created_at
            FROM conversation
            ORDER BY created_at DESC
            LIMIT {}
        ", limit);

        let mut response = self.db.query(query).await?;
        let results: Vec<ConvRow> = response.take(0)?;

        let summaries = results.into_iter().map(|row| {
            ConversationSummary {
                id: row.id.to_string(),
                agent_name: row.agent_name,
                summary: row.summary,
                detailed_summary: row.detailed_summary,
                created_at: row.created_at,
            }
        }).collect();

        Ok(summaries)
    }

    /// Loads a specific conversation with all its messages
    pub async fn load_conversation(&self, id: &str) -> Result<(String, Vec<StoredMessage>)> {
        #[derive(Debug, Deserialize)]
        struct ConvAgent {
            agent_name: String,
        }

        let normalized_id = Self::normalize_conversation_id(id);
        let conv: Option<ConvAgent> = self.db.select(("conversation", normalized_id)).await?;
        let agent_name = conv
            .ok_or_else(|| color_eyre::eyre::eyre!("Conversation not found"))?
            .agent_name;

        let conversation_ref = Self::conversation_ref(normalized_id);
        let mut response = self.db.query("
            SELECT role, content, timestamp, display_name
            FROM message
            WHERE conversation = $conv_id
            ORDER BY timestamp ASC
        ")
        .bind(("conv_id", conversation_ref))
        .await?;

        let messages: Vec<StoredMessage> = response.take(0)?;

        Ok((agent_name, messages))
    }

    /// Loads messages from all conversations within a date range (RFC 3339 strings).
    /// Returns conversations grouped with their messages, newest conversations first.
    /// Each conversation is truncated to `max_messages_per_conversation` messages.
    pub async fn load_conversations_in_date_range(
        &self,
        range_start: &str,
        range_end: &str,
        max_messages_per_conversation: usize,
    ) -> Result<Vec<ConversationWithMessages>> {
        // First, get conversations in the date range
        #[derive(Debug, Deserialize)]
        struct ConvRow {
            id: surrealdb::sql::Thing,
            created_at: String,
        }

        let mut conv_response = self.db.query("
            SELECT id, created_at
            FROM conversation
            WHERE created_at >= $start AND created_at < $end
            ORDER BY created_at ASC
        ")
        .bind(("start", range_start.to_string()))
        .bind(("end", range_end.to_string()))
        .await?;

        let conv_rows: Vec<ConvRow> = conv_response.take(0)?;

        let mut results = Vec::new();
        for row in conv_rows {
            let conversation_ref = Thing::from(("conversation", row.id.id.to_string().as_str()));
            let mut msg_response = self.db.query("
                SELECT role, content, timestamp, display_name
                FROM message
                WHERE conversation = $conv_id AND role != 'System'
                ORDER BY timestamp ASC
            ")
            .bind(("conv_id", conversation_ref))
            .await?;

            let messages: Vec<StoredMessage> = msg_response.take(0)?;
            if messages.is_empty() {
                continue;
            }

            // Take the last N messages to keep the most recent context
            let truncated: Vec<StoredMessage> = if messages.len() > max_messages_per_conversation {
                messages
                    .into_iter()
                    .rev()
                    .take(max_messages_per_conversation)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect()
            } else {
                messages
            };

            results.push(ConversationWithMessages {
                created_at: row.created_at,
                messages: truncated,
            });
        }

        Ok(results)
    }

    /// Deletes a conversation and all its messages
    pub async fn delete_conversation(&self, id: &str) -> Result<()> {
        let normalized_id = Self::normalize_conversation_id(id);
        let conversation_ref = Self::conversation_ref(normalized_id);
        // Delete messages first
        self.db.query("
            DELETE FROM message WHERE conversation = $conv_id
        ")
        .bind(("conv_id", conversation_ref))
        .await?;

        // Delete conversation
        let _: Option<ConversationRecord> = self.db.delete(("conversation", normalized_id)).await?;

        Ok(())
    }

    /// Deletes all conversations and messages
    pub async fn delete_all_conversations(&self) -> Result<()> {
        self.db.query("DELETE FROM message").await?;
        self.db.query("DELETE FROM conversation").await?;
        Ok(())
    }

    /// Updates summary and messages for an existing conversation
    pub async fn update_conversation(
        &self,
        id: &str,
        summary: &str,
        detailed_summary: &str,
        messages: &[ConversationMessage],
    ) -> Result<()> {
        let normalized_id = Self::normalize_conversation_id(id);
        let conversation_ref = Self::conversation_ref(normalized_id);
        let now = chrono::Local::now().to_rfc3339();

        // Update conversation
        let _: Option<ConversationRecord> = self.db
            .update(("conversation", normalized_id))
            .merge(serde_json::json!({
                "summary": summary,
                "detailed_summary": detailed_summary,
                "updated_at": now,
            }))
            .await?;

        // Delete old messages
        self.db.query("
            DELETE FROM message WHERE conversation = $conv_id
        ")
        .bind(("conv_id", conversation_ref.clone()))
        .await?;

        // Insert new messages
        for message in messages {
            let _: Option<MessageRecord> = self.db
                .create("message")
                .content(MessageRecord {
                    id: None,
                    conversation: conversation_ref.clone(),
                    role: message.role.clone(),
                    content: message.content.clone(),
                    embedding: None,
                    timestamp: message.timestamp.clone(),
                    display_name: message.display_name.clone(),
                })
                .await?;
        }

        Ok(())
    }

    /// Filters conversations by summary, agent name, or message content
    pub async fn filter_conversations(&self, filter: &str) -> Result<Vec<ConversationSummary>> {
        #[derive(Debug, Deserialize)]
        struct ConvRow {
            id: surrealdb::sql::Thing,
            agent_name: String,
            summary: Option<String>,
            detailed_summary: Option<String>,
            created_at: String,
        }

        let filter_str = filter.to_string();
        let mut response = self.db.query("
            SELECT
                id,
                agent_name,
                summary,
                detailed_summary,
                created_at
            FROM conversation
            WHERE
                string::contains(string::lowercase(summary), string::lowercase($filter))
                OR string::contains(string::lowercase(agent_name), string::lowercase($filter))
                OR id IN (
                    SELECT conversation FROM message
                    WHERE string::contains(string::lowercase(content), string::lowercase($filter))
                )
            ORDER BY created_at DESC
        ")
        .bind(("filter", filter_str))
        .await?;

        let results: Vec<ConvRow> = response.take(0)?;

        let summaries = results.into_iter().map(|row| {
            ConversationSummary {
                id: row.id.to_string(),
                agent_name: row.agent_name,
                summary: row.summary,
                detailed_summary: row.detailed_summary,
                created_at: row.created_at,
            }
        }).collect();
        Ok(summaries)
    }

    /// Updates only conversation messages (keeps existing summaries)
    pub async fn update_conversation_messages(
        &self,
        id: &str,
        messages: &[ConversationMessage],
    ) -> Result<()> {
        let normalized_id = Self::normalize_conversation_id(id);
        let conversation_ref = Self::conversation_ref(normalized_id);
        let now = chrono::Local::now().to_rfc3339();

        let _: Option<ConversationRecord> = self.db
            .update(("conversation", normalized_id))
            .merge(serde_json::json!({
                "updated_at": now,
            }))
            .await?;

        self.db.query("
            DELETE FROM message WHERE conversation = $conv_id
        ")
        .bind(("conv_id", conversation_ref.clone()))
        .await?;

        for message in messages {
            let _: Option<MessageRecord> = self.db
                .create("message")
                .content(MessageRecord {
                    id: None,
                    conversation: conversation_ref.clone(),
                    role: message.role.clone(),
                    content: message.content.clone(),
                    embedding: None,
                    timestamp: message.timestamp.clone(),
                    display_name: message.display_name.clone(),
                })
                .await?;
        }

        Ok(())
    }

    // ── Topic tracking for project suggestions ──────────────────────────────

    /// Records topic mentions for a conversation (batch insert)
    pub async fn record_topic_mentions(
        &self,
        topics: &[String],
        conversation_id: &str,
    ) -> Result<()> {
        let now = chrono::Local::now().to_rfc3339();
        for topic in topics {
            let normalized = topic.to_lowercase().trim().to_string();
            if normalized.is_empty() {
                continue;
            }
            self.db.query(
                "CREATE topic_mention SET topic = $topic, conversation_id = $conv_id, created_at = $now"
            )
            .bind(("topic", normalized))
            .bind(("conv_id", conversation_id.to_string()))
            .bind(("now", now.clone()))
            .await?;
        }
        Ok(())
    }

    /// Loads topics that have >= threshold mentions and don't yet have a project file.
    /// Returns (topic_name, mention_count) pairs.
    pub async fn load_frequent_topics(
        &self,
        threshold: usize,
    ) -> Result<Vec<(String, usize)>> {
        #[derive(Debug, Deserialize)]
        struct TopicCount {
            topic: String,
            count: usize,
        }

        let mut response = self.db.query("
            SELECT topic, count() AS count
            FROM topic_mention
            GROUP BY topic
            ORDER BY count DESC
        ").await?;

        let results: Vec<TopicCount> = response.take(0)?;
        Ok(results
            .into_iter()
            .filter(|entry| entry.count >= threshold)
            .map(|entry| (entry.topic, entry.count))
            .collect())
    }

    /// Clears all topic mentions for a given topic (after project creation or archival)
    pub async fn clear_topic_mentions(&self, topic: &str) -> Result<()> {
        let normalized = topic.to_lowercase().trim().to_string();
        self.db.query(
            "DELETE FROM topic_mention WHERE topic = $topic"
        )
        .bind(("topic", normalized))
        .await?;
        Ok(())
    }
}
