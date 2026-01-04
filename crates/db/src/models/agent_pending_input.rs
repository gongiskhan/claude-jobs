use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Row, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum InputType {
    Question,
    Approval,
    Clarification,
}

impl InputType {
    pub fn as_str(&self) -> &'static str {
        match self {
            InputType::Question => "question",
            InputType::Approval => "approval",
            InputType::Clarification => "clarification",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "question" => Some(InputType::Question),
            "approval" => Some(InputType::Approval),
            "clarification" => Some(InputType::Clarification),
            _ => None,
        }
    }
}

impl std::fmt::Display for InputType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct AgentPendingInput {
    pub id: Uuid,
    pub agent_session_id: Uuid,
    pub input_type: String,
    pub prompt: String,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_input: Option<String>,
    pub response: Option<String>,
    pub approved: Option<bool>,
    pub responded_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl AgentPendingInput {
    pub fn input_type_enum(&self) -> Option<InputType> {
        InputType::from_str(&self.input_type)
    }

    pub fn tool_input_json(&self) -> Option<serde_json::Value> {
        self.tool_input
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
    }

    pub fn is_pending(&self) -> bool {
        self.responded_at.is_none()
    }

    fn from_row(row: sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        let approved_int: Option<i32> = row.try_get("approved")?;
        let approved = approved_int.map(|v| v != 0);

        Ok(Self {
            id: row.try_get::<String, _>("id")?.parse().map_err(|_| sqlx::Error::Decode("invalid uuid".into()))?,
            agent_session_id: row.try_get::<String, _>("agent_session_id")?.parse().map_err(|_| sqlx::Error::Decode("invalid uuid".into()))?,
            input_type: row.try_get("input_type")?,
            prompt: row.try_get("prompt")?,
            tool_call_id: row.try_get("tool_call_id")?,
            tool_name: row.try_get("tool_name")?,
            tool_input: row.try_get("tool_input")?,
            response: row.try_get("response")?,
            approved,
            responded_at: row.try_get("responded_at")?,
            created_at: row.try_get("created_at")?,
        })
    }
}

#[derive(Debug, Deserialize, TS)]
pub struct CreateAgentPendingInput {
    pub input_type: InputType,
    pub prompt: String,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_input: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, TS)]
pub struct RespondToInput {
    pub response: Option<String>,
    pub approved: Option<bool>,
}

const SELECT_FIELDS: &str = r#"
    id, agent_session_id, input_type, prompt, tool_call_id,
    tool_name, tool_input, response, approved, responded_at, created_at
"#;

impl AgentPendingInput {
    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        let id_str = id.to_string();
        let sql = format!("SELECT {} FROM agent_pending_inputs WHERE id = $1", SELECT_FIELDS);
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
            "SELECT {} FROM agent_pending_inputs WHERE agent_session_id = $1 ORDER BY created_at DESC",
            SELECT_FIELDS
        );
        let rows = sqlx::query(&sql)
            .bind(&id_str)
            .fetch_all(pool)
            .await?;
        rows.into_iter().map(Self::from_row).collect()
    }

    /// Find pending (unanswered) inputs for an agent session
    pub async fn find_pending(
        pool: &SqlitePool,
        agent_session_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let id_str = agent_session_id.to_string();
        let sql = format!(
            "SELECT {} FROM agent_pending_inputs WHERE agent_session_id = $1 AND responded_at IS NULL ORDER BY created_at ASC",
            SELECT_FIELDS
        );
        let rows = sqlx::query(&sql)
            .bind(&id_str)
            .fetch_all(pool)
            .await?;
        rows.into_iter().map(Self::from_row).collect()
    }

    /// Find the latest pending input for an agent session
    pub async fn find_latest_pending(
        pool: &SqlitePool,
        agent_session_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        let id_str = agent_session_id.to_string();
        let sql = format!(
            "SELECT {} FROM agent_pending_inputs WHERE agent_session_id = $1 AND responded_at IS NULL ORDER BY created_at DESC LIMIT 1",
            SELECT_FIELDS
        );
        let row = sqlx::query(&sql)
            .bind(&id_str)
            .fetch_optional(pool)
            .await?;
        row.map(Self::from_row).transpose()
    }

    /// Find pending input by tool call ID
    pub async fn find_by_tool_call_id(
        pool: &SqlitePool,
        tool_call_id: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
        let sql = format!(
            "SELECT {} FROM agent_pending_inputs WHERE tool_call_id = $1 AND responded_at IS NULL",
            SELECT_FIELDS
        );
        let row = sqlx::query(&sql)
            .bind(tool_call_id)
            .fetch_optional(pool)
            .await?;
        row.map(Self::from_row).transpose()
    }

    /// Find all pending inputs across all sessions (for dashboard)
    pub async fn find_all_pending(pool: &SqlitePool) -> Result<Vec<Self>, sqlx::Error> {
        let sql = format!(
            "SELECT {} FROM agent_pending_inputs WHERE responded_at IS NULL ORDER BY created_at ASC",
            SELECT_FIELDS
        );
        let rows = sqlx::query(&sql)
            .fetch_all(pool)
            .await?;
        rows.into_iter().map(Self::from_row).collect()
    }

    pub async fn create(
        pool: &SqlitePool,
        id: Uuid,
        agent_session_id: Uuid,
        data: &CreateAgentPendingInput,
    ) -> Result<Self, sqlx::Error> {
        let id_str = id.to_string();
        let agent_session_id_str = agent_session_id.to_string();
        let input_type = data.input_type.as_str();
        let tool_input = data.tool_input.as_ref().map(|v| v.to_string());

        let sql = format!(
            r#"INSERT INTO agent_pending_inputs
               (id, agent_session_id, input_type, prompt, tool_call_id, tool_name, tool_input)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               RETURNING {}"#,
            SELECT_FIELDS
        );
        let row = sqlx::query(&sql)
            .bind(&id_str)
            .bind(&agent_session_id_str)
            .bind(input_type)
            .bind(&data.prompt)
            .bind(&data.tool_call_id)
            .bind(&data.tool_name)
            .bind(&tool_input)
            .fetch_one(pool)
            .await?;
        Self::from_row(row)
    }

    /// Record user response to a pending input
    pub async fn respond(
        pool: &SqlitePool,
        id: Uuid,
        data: &RespondToInput,
    ) -> Result<Self, sqlx::Error> {
        let id_str = id.to_string();
        let now = Utc::now();
        let approved_int = data.approved.map(|b| if b { 1i32 } else { 0i32 });

        let sql = format!(
            r#"UPDATE agent_pending_inputs
               SET response = $2, approved = $3, responded_at = $4
               WHERE id = $1
               RETURNING {}"#,
            SELECT_FIELDS
        );
        let row = sqlx::query(&sql)
            .bind(&id_str)
            .bind(&data.response)
            .bind(approved_int)
            .bind(now)
            .fetch_one(pool)
            .await?;
        Self::from_row(row)
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<(), sqlx::Error> {
        let id_str = id.to_string();
        sqlx::query("DELETE FROM agent_pending_inputs WHERE id = $1")
            .bind(&id_str)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Delete all pending inputs for an agent session
    pub async fn delete_all_for_session(
        pool: &SqlitePool,
        agent_session_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        let id_str = agent_session_id.to_string();
        let result = sqlx::query("DELETE FROM agent_pending_inputs WHERE agent_session_id = $1")
            .bind(&id_str)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }
}
