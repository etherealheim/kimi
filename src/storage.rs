use color_eyre::Result;
use rusqlite::{Connection, params};
use std::path::PathBuf;

/// Summary of a saved conversation
#[derive(Debug, Clone)]
pub struct ConversationSummary {
    pub id: i64,
    pub agent_name: String,
    pub summary: Option<String>,
    #[allow(dead_code)]
    pub detailed_summary: Option<String>,
    pub created_at: String,
    pub message_count: usize,
}

/// A stored message from conversation history
#[derive(Debug, Clone)]
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

/// Manages persistent storage of conversations using SQLite
pub struct StorageManager {
    db_path: PathBuf,
}

impl StorageManager {
    /// Creates a new storage manager and initializes the database
    pub fn new() -> Result<Self> {
        let proj_dirs = directories::ProjectDirs::from("", "", "kimi")
            .ok_or_else(|| color_eyre::eyre::eyre!("Could not determine config directory"))?;

        let data_dir = proj_dirs.data_dir();
        std::fs::create_dir_all(data_dir)?;

        let db_path = data_dir.join("history.db");

        let manager = Self { db_path };
        manager.init_db()?;

        Ok(manager)
    }

    fn get_connection(&self) -> Result<Connection> {
        Ok(Connection::open(&self.db_path)?)
    }

    fn init_db(&self) -> Result<()> {
        let conn = self.get_connection()?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS conversations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                agent_name TEXT NOT NULL,
                summary TEXT,
                detailed_summary TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                conversation_id INTEGER NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                display_name TEXT,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            )",
            [],
        )?;

        self.ensure_column(&conn, "conversations", "detailed_summary", "TEXT")?;
        self.ensure_column(&conn, "messages", "display_name", "TEXT")?;
        Ok(())
    }

    fn ensure_column(
        &self,
        conn: &Connection,
        table: &str,
        column: &str,
        column_type: &str,
    ) -> Result<()> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
        let existing = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if !existing.iter().any(|name| name == column) {
            conn.execute(
                &format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, column_type),
                [],
            )?;
        }
        Ok(())
    }

    /// Saves a conversation with messages to the database
    pub fn save_conversation(&self, data: ConversationData) -> Result<i64> {
        let conn = self.get_connection()?;
        let now = chrono::Local::now().to_rfc3339();

        conn.execute(
            "INSERT INTO conversations (agent_name, summary, detailed_summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![data.agent_name, data.summary, data.detailed_summary, &now, &now],
        )?;

        let conversation_id = conn.last_insert_rowid();

        for message in data.messages {
            conn.execute(
                "INSERT INTO messages (conversation_id, role, content, timestamp, display_name) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    conversation_id,
                    &message.role,
                    &message.content,
                    &message.timestamp,
                    &message.display_name
                ],
            )?;
        }

        Ok(conversation_id)
    }

    /// Loads all conversation summaries from the database
    pub fn load_conversations(&self) -> Result<Vec<ConversationSummary>> {
        let conn = self.get_connection()?;

        let mut stmt = conn.prepare(
            "SELECT c.id, c.agent_name, c.summary, c.detailed_summary, c.created_at, COUNT(m.id) as msg_count
             FROM conversations c
             LEFT JOIN messages m ON c.id = m.conversation_id
             GROUP BY c.id
             ORDER BY c.created_at DESC",
        )?;

        let conversations = stmt
            .query_map([], |row: &rusqlite::Row| {
                Ok(ConversationSummary {
                    id: row.get(0)?,
                    agent_name: row.get(1)?,
                    summary: row.get(2)?,
                    detailed_summary: row.get(3)?,
                    created_at: row.get(4)?,
                    message_count: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(conversations)
    }

    /// Loads a specific conversation with all its messages
    pub fn load_conversation(&self, id: i64) -> Result<(String, Vec<StoredMessage>)> {
        let conn = self.get_connection()?;

        let agent_name: String = conn.query_row(
            "SELECT agent_name FROM conversations WHERE id = ?1",
            params![id],
            |row: &rusqlite::Row| row.get(0),
        )?;

        let mut stmt = conn.prepare(
            "SELECT role, content, timestamp, display_name FROM messages WHERE conversation_id = ?1 ORDER BY id ASC"
        )?;

        let messages = stmt
            .query_map(params![id], |row: &rusqlite::Row| {
                Ok(StoredMessage {
                    role: row.get(0)?,
                    content: row.get(1)?,
                    timestamp: row.get(2)?,
                    display_name: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok((agent_name, messages))
    }

    /// Deletes a conversation and all its messages
    pub fn delete_conversation(&self, id: i64) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Deletes all conversations and messages
    pub fn delete_all_conversations(&self) -> Result<()> {
        let mut conn = self.get_connection()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM messages", [])?;
        tx.execute("DELETE FROM conversations", [])?;
        tx.commit()?;
        Ok(())
    }

    /// Updates summary and messages for an existing conversation
    pub fn update_conversation(
        &self,
        id: i64,
        summary: &str,
        detailed_summary: &str,
        messages: &[ConversationMessage],
    ) -> Result<()> {
        let mut conn = self.get_connection()?;
        let now = chrono::Local::now().to_rfc3339();
        let tx = conn.transaction()?;

        tx.execute(
            "UPDATE conversations SET summary = ?1, detailed_summary = ?2, updated_at = ?3 WHERE id = ?4",
            params![summary, detailed_summary, &now, id],
        )?;

        tx.execute(
            "DELETE FROM messages WHERE conversation_id = ?1",
            params![id],
        )?;

        for message in messages {
            tx.execute(
                "INSERT INTO messages (conversation_id, role, content, timestamp, display_name) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    id,
                    &message.role,
                    &message.content,
                    &message.timestamp,
                    &message.display_name
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }


    /// Filters conversations by summary, agent name, or message content
    pub fn filter_conversations(&self, filter: &str) -> Result<Vec<ConversationSummary>> {
        let conn = self.get_connection()?;
        let filter_pattern = format!("%{}%", filter);

        let mut stmt = conn.prepare(
            "SELECT c.id, c.agent_name, c.summary, c.detailed_summary, c.created_at,
                    (SELECT COUNT(*) FROM messages m2 WHERE m2.conversation_id = c.id) as msg_count
             FROM conversations c
             WHERE c.summary LIKE ?1
                OR c.agent_name LIKE ?1
                OR EXISTS (
                    SELECT 1 FROM messages m
                    WHERE m.conversation_id = c.id AND m.content LIKE ?1
                )
             ORDER BY c.created_at DESC",
        )?;

        let conversations = stmt
            .query_map(params![filter_pattern], |row: &rusqlite::Row| {
                Ok(ConversationSummary {
                    id: row.get(0)?,
                    agent_name: row.get(1)?,
                    summary: row.get(2)?,
                    detailed_summary: row.get(3)?,
                    created_at: row.get(4)?,
                    message_count: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(conversations)
    }
}
