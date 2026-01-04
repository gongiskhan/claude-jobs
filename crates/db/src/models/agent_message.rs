use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
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
}

#[derive(Debug, Deserialize, TS)]
pub struct CreateAgentMessage {
    pub role: MessageRole,
    pub content: String,
    pub metadata: Option<serde_json::Value>,
    pub token_count: Option<i32>,
}

impl AgentMessage {
    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            AgentMessage,
            r#"SELECT
                id AS "id!: Uuid",
                agent_session_id AS "agent_session_id!: Uuid",
                role,
                content,
                metadata,
                token_count,
                created_at AS "created_at!: DateTime<Utc>"
            FROM agent_messages
            WHERE id = $1"#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_agent_session_id(
        pool: &SqlitePool,
        agent_session_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            AgentMessage,
            r#"SELECT
                id AS "id!: Uuid",
                agent_session_id AS "agent_session_id!: Uuid",
                role,
                content,
                metadata,
                token_count,
                created_at AS "created_at!: DateTime<Utc>"
            FROM agent_messages
            WHERE agent_session_id = $1
            ORDER BY created_at ASC"#,
            agent_session_id
        )
        .fetch_all(pool)
        .await
    }

    /// Get messages with pagination for context management
    pub async fn find_recent(
        pool: &SqlitePool,
        agent_session_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            AgentMessage,
            r#"SELECT
                id AS "id!: Uuid",
                agent_session_id AS "agent_session_id!: Uuid",
                role,
                content,
                metadata,
                token_count,
                created_at AS "created_at!: DateTime<Utc>"
            FROM agent_messages
            WHERE agent_session_id = $1
            ORDER BY created_at DESC
            LIMIT $2"#,
            agent_session_id,
            limit
        )
        .fetch_all(pool)
        .await
        .map(|mut msgs| {
            msgs.reverse(); // Return in chronological order
            msgs
        })
    }

    /// Get total token count for a session
    pub async fn total_tokens(
        pool: &SqlitePool,
        agent_session_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"SELECT COALESCE(SUM(token_count), 0) AS "total!: i64"
            FROM agent_messages
            WHERE agent_session_id = $1"#,
            agent_session_id
        )
        .fetch_one(pool)
        .await?;
        Ok(result)
    }

    pub async fn create(
        pool: &SqlitePool,
        id: Uuid,
        agent_session_id: Uuid,
        data: &CreateAgentMessage,
    ) -> Result<Self, sqlx::Error> {
        let role = data.role.as_str();
        let metadata = data.metadata.as_ref().map(|v| v.to_string());

        sqlx::query_as!(
            AgentMessage,
            r#"INSERT INTO agent_messages (id, agent_session_id, role, content, metadata, token_count)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING
                   id AS "id!: Uuid",
                   agent_session_id AS "agent_session_id!: Uuid",
                   role,
                   content,
                   metadata,
                   token_count,
                   created_at AS "created_at!: DateTime<Utc>""#,
            id,
            agent_session_id,
            role,
            data.content,
            metadata,
            data.token_count
        )
        .fetch_one(pool)
        .await
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
        sqlx::query!("DELETE FROM agent_messages WHERE id = $1", id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Delete all messages for an agent session
    pub async fn delete_all_for_session(
        pool: &SqlitePool,
        agent_session_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            "DELETE FROM agent_messages WHERE agent_session_id = $1",
            agent_session_id
        )
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
        let result = sqlx::query!(
            r#"DELETE FROM agent_messages
               WHERE agent_session_id = $1
               AND id NOT IN (
                   SELECT id FROM agent_messages
                   WHERE agent_session_id = $1
                   ORDER BY created_at DESC
                   LIMIT $2
               )"#,
            agent_session_id,
            keep_count
        )
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }
}
