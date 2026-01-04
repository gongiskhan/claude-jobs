use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Row, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
    ToolUse,
    ToolResult,
    System,
}

impl MessageRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::ToolUse => "tool_use",
            MessageRole::ToolResult => "tool_result",
            MessageRole::System => "system",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "user" => Some(MessageRole::User),
            "assistant" => Some(MessageRole::Assistant),
            "tool_use" => Some(MessageRole::ToolUse),
            "tool_result" => Some(MessageRole::ToolResult),
            "system" => Some(MessageRole::System),
            _ => None,
        }
    }
}

impl std::fmt::Display for MessageRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct AgentMessage {
    pub id: Uuid,
    pub agent_session_id: Uuid,
    pub role: String,
    pub content: String,
    pub metadata: Option<String>,
    pub token_count: Option<i32>,
    pub created_at: DateTime<Utc>,
}

impl AgentMessage {
    pub fn role_enum(&self) -> Option<MessageRole> {
        MessageRole::from_str(&self.role)
    }

    pub fn metadata_json(&self) -> Option<serde_json::Value> {
        self.metadata
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
    }

    fn from_row(row: sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get::<String, _>("id")?.parse().map_err(|_| sqlx::Error::Decode("invalid uuid".into()))?,
            agent_session_id: row.try_get::<String, _>("agent_session_id")?.parse().map_err(|_| sqlx::Error::Decode("invalid uuid".into()))?,
            role: row.try_get("role")?,
            content: row.try_get("content")?,
            metadata: row.try_get("metadata")?,
            token_count: row.try_get("token_count")?,
            created_at: row.try_get("created_at")?,
        })
    }
}

#[derive(Debug, Deserialize, TS)]
pub struct CreateAgentMessage {
    pub role: MessageRole,
    pub content: String,
    pub metadata: Option<serde_json::Value>,
    pub token_count: Option<i32>,
}

const SELECT_FIELDS: &str = "id, agent_session_id, role, content, metadata, token_count, created_at";

impl AgentMessage {
    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        let id_str = id.to_string();
        let sql = format!("SELECT {} FROM agent_messages WHERE id = $1", SELECT_FIELDS);
        let row = sqlx::query(&sql)
            .bind(&id_str)
            .fetch_optional(pool)
            .await?;
        row.map(Self::from_row).transpose()
    }

    pub async fn find_by_agent_session_id(
        pool: &SqlitePool,
        agent_session_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let id_str = agent_session_id.to_string();
        let sql = format!(
            "SELECT {} FROM agent_messages WHERE agent_session_id = $1 ORDER BY created_at ASC",
            SELECT_FIELDS
        );
        let rows = sqlx::query(&sql)
            .bind(&id_str)
            .fetch_all(pool)
            .await?;
        rows.into_iter().map(Self::from_row).collect()
    }

    /// Get messages with pagination for context management
    pub async fn find_recent(
        pool: &SqlitePool,
        agent_session_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let id_str = agent_session_id.to_string();
        let sql = format!(
            "SELECT {} FROM agent_messages WHERE agent_session_id = $1 ORDER BY created_at DESC LIMIT $2",
            SELECT_FIELDS
        );
        let rows = sqlx::query(&sql)
            .bind(&id_str)
            .bind(limit)
            .fetch_all(pool)
            .await?;
        let mut messages: Vec<Self> = rows.into_iter().map(Self::from_row).collect::<Result<Vec<_>, _>>()?;
        messages.reverse(); // Return in chronological order
        Ok(messages)
    }

    /// Get total token count for a session
    pub async fn total_tokens(
        pool: &SqlitePool,
        agent_session_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        let id_str = agent_session_id.to_string();
        let row = sqlx::query(
            "SELECT COALESCE(SUM(token_count), 0) as total FROM agent_messages WHERE agent_session_id = $1"
        )
        .bind(&id_str)
        .fetch_one(pool)
        .await?;
        Ok(row.try_get::<i64, _>("total")?)
    }

    pub async fn create(
        pool: &SqlitePool,
        id: Uuid,
        agent_session_id: Uuid,
        data: &CreateAgentMessage,
    ) -> Result<Self, sqlx::Error> {
        let id_str = id.to_string();
        let agent_session_id_str = agent_session_id.to_string();
        let role = data.role.as_str();
        let metadata = data.metadata.as_ref().map(|v| v.to_string());

        let sql = format!(
            r#"INSERT INTO agent_messages (id, agent_session_id, role, content, metadata, token_count)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING {}"#,
            SELECT_FIELDS
        );
        let row = sqlx::query(&sql)
            .bind(&id_str)
            .bind(&agent_session_id_str)
            .bind(role)
            .bind(&data.content)
            .bind(&metadata)
            .bind(data.token_count)
            .fetch_one(pool)
            .await?;
        Self::from_row(row)
    }

    /// Bulk insert messages (for restoring context)
    pub async fn create_many(
        pool: &SqlitePool,
        agent_session_id: Uuid,
        messages: &[CreateAgentMessage],
    ) -> Result<Vec<Self>, sqlx::Error> {
        let mut results = Vec::with_capacity(messages.len());
        for msg in messages {
            let id = Uuid::new_v4();
            let created = Self::create(pool, id, agent_session_id, msg).await?;
            results.push(created);
        }
        Ok(results)
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<(), sqlx::Error> {
        let id_str = id.to_string();
        sqlx::query("DELETE FROM agent_messages WHERE id = $1")
            .bind(&id_str)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Delete all messages for an agent session
    pub async fn delete_all_for_session(
        pool: &SqlitePool,
        agent_session_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        let id_str = agent_session_id.to_string();
        let result = sqlx::query("DELETE FROM agent_messages WHERE agent_session_id = $1")
            .bind(&id_str)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Delete old messages to stay within token limit (keeps most recent)
    pub async fn prune_old_messages(
        pool: &SqlitePool,
        agent_session_id: Uuid,
        keep_count: i64,
    ) -> Result<u64, sqlx::Error> {
        let id_str = agent_session_id.to_string();
        let result = sqlx::query(
            r#"DELETE FROM agent_messages
               WHERE agent_session_id = $1
               AND id NOT IN (
                   SELECT id FROM agent_messages
                   WHERE agent_session_id = $1
                   ORDER BY created_at DESC
                   LIMIT $2
               )"#,
        )
        .bind(&id_str)
        .bind(keep_count)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }
}
