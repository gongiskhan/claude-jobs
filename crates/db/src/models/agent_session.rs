use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Row, SqlitePool};
use thiserror::Error;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AgentSessionError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("Agent session not found")]
    NotFound,
    #[error("Invalid state transition from {from} to {to}")]
    InvalidStateTransition { from: String, to: String },
    #[error("Session not in awaiting_input state")]
    NotAwaitingInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    Coding,
    Orchestration,
    Deployment,
}

impl AgentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentType::Coding => "coding",
            AgentType::Orchestration => "orchestration",
            AgentType::Deployment => "deployment",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "coding" => Some(AgentType::Coding),
            "orchestration" => Some(AgentType::Orchestration),
            "deployment" => Some(AgentType::Deployment),
            _ => None,
        }
    }
}

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Created,
    Idle,
    Executing,
    AwaitingInput,
    Paused,
    Completed,
    Terminated,
}

impl AgentState {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentState::Created => "created",
            AgentState::Idle => "idle",
            AgentState::Executing => "executing",
            AgentState::AwaitingInput => "awaiting_input",
            AgentState::Paused => "paused",
            AgentState::Completed => "completed",
            AgentState::Terminated => "terminated",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "created" => Some(AgentState::Created),
            "idle" => Some(AgentState::Idle),
            "executing" => Some(AgentState::Executing),
            "awaiting_input" => Some(AgentState::AwaitingInput),
            "paused" => Some(AgentState::Paused),
            "completed" => Some(AgentState::Completed),
            "terminated" => Some(AgentState::Terminated),
            _ => None,
        }
    }

    /// Check if a state transition is valid
    pub fn can_transition_to(&self, to: AgentState) -> bool {
        matches!(
            (self, to),
            // From Created
            (AgentState::Created, AgentState::Idle)
                | (AgentState::Created, AgentState::Terminated)
                // From Idle
                | (AgentState::Idle, AgentState::Executing)
                | (AgentState::Idle, AgentState::Terminated)
                // From Executing
                | (AgentState::Executing, AgentState::Idle)
                | (AgentState::Executing, AgentState::AwaitingInput)
                | (AgentState::Executing, AgentState::Paused)
                | (AgentState::Executing, AgentState::Completed)
                | (AgentState::Executing, AgentState::Terminated)
                // From AwaitingInput
                | (AgentState::AwaitingInput, AgentState::Executing)
                | (AgentState::AwaitingInput, AgentState::Paused)
                | (AgentState::AwaitingInput, AgentState::Terminated)
                // From Paused
                | (AgentState::Paused, AgentState::Executing)
                | (AgentState::Paused, AgentState::Terminated)
        )
    }

    /// Check if the agent is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self, AgentState::Completed | AgentState::Terminated)
    }

    /// Check if the agent can receive new queries
    pub fn can_query(&self) -> bool {
        matches!(self, AgentState::Idle)
    }
}

impl std::fmt::Display for AgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct AgentSession {
    pub id: Uuid,
    pub session_id: Uuid,
    pub agent_type: String,
    pub state: String,
    pub sdk_session_id: Option<String>,
    pub context: Option<String>,
    pub current_tool_call_id: Option<String>,
    pub pending_question: Option<String>,
    pub error_message: Option<String>,
    pub working_dir: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl AgentSession {
    pub fn agent_type_enum(&self) -> Option<AgentType> {
        AgentType::from_str(&self.agent_type)
    }

    pub fn state_enum(&self) -> Option<AgentState> {
        AgentState::from_str(&self.state)
    }

    fn from_row(row: sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get::<String, _>("id")?.parse().map_err(|_| sqlx::Error::Decode("invalid uuid".into()))?,
            session_id: row.try_get::<String, _>("session_id")?.parse().map_err(|_| sqlx::Error::Decode("invalid uuid".into()))?,
            agent_type: row.try_get("agent_type")?,
            state: row.try_get("state")?,
            sdk_session_id: row.try_get("sdk_session_id")?,
            context: row.try_get("context")?,
            current_tool_call_id: row.try_get("current_tool_call_id")?,
            pending_question: row.try_get("pending_question")?,
            error_message: row.try_get("error_message")?,
            working_dir: row.try_get("working_dir")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            completed_at: row.try_get("completed_at")?,
        })
    }
}

#[derive(Debug, Deserialize, TS)]
pub struct CreateAgentSession {
    pub agent_type: AgentType,
    pub working_dir: Option<String>,
}

#[derive(Debug, Default, Deserialize, TS)]
pub struct UpdateAgentSession {
    pub state: Option<AgentState>,
    pub sdk_session_id: Option<String>,
    pub context: Option<String>,
    pub current_tool_call_id: Option<Option<String>>,
    pub pending_question: Option<Option<String>>,
    pub error_message: Option<String>,
}

const SELECT_FIELDS: &str = r#"
    id, session_id, agent_type, state, sdk_session_id, context,
    current_tool_call_id, pending_question, error_message, working_dir,
    created_at, updated_at, completed_at
"#;

impl AgentSession {
    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        let id_str = id.to_string();
        let sql = format!("SELECT {} FROM agent_sessions WHERE id = $1", SELECT_FIELDS);
        let row = sqlx::query(&sql)
            .bind(&id_str)
            .fetch_optional(pool)
            .await?;
        row.map(Self::from_row).transpose()
    }

    pub async fn find_by_session_id(
        pool: &SqlitePool,
        session_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let id_str = session_id.to_string();
        let sql = format!(
            "SELECT {} FROM agent_sessions WHERE session_id = $1 ORDER BY created_at DESC",
            SELECT_FIELDS
        );
        let rows = sqlx::query(&sql)
            .bind(&id_str)
            .fetch_all(pool)
            .await?;
        rows.into_iter().map(Self::from_row).collect()
    }

    pub async fn find_latest_by_session_id(
        pool: &SqlitePool,
        session_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        let id_str = session_id.to_string();
        let sql = format!(
            "SELECT {} FROM agent_sessions WHERE session_id = $1 ORDER BY created_at DESC LIMIT 1",
            SELECT_FIELDS
        );
        let row = sqlx::query(&sql)
            .bind(&id_str)
            .fetch_optional(pool)
            .await?;
        row.map(Self::from_row).transpose()
    }

    pub async fn find_active(pool: &SqlitePool) -> Result<Vec<Self>, sqlx::Error> {
        let sql = format!(
            "SELECT {} FROM agent_sessions WHERE state NOT IN ('completed', 'terminated') ORDER BY updated_at DESC",
            SELECT_FIELDS
        );
        let rows = sqlx::query(&sql)
            .fetch_all(pool)
            .await?;
        rows.into_iter().map(Self::from_row).collect()
    }

    pub async fn find_awaiting_input(pool: &SqlitePool) -> Result<Vec<Self>, sqlx::Error> {
        let sql = format!(
            "SELECT {} FROM agent_sessions WHERE state = 'awaiting_input' ORDER BY updated_at DESC",
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
        session_id: Uuid,
        data: &CreateAgentSession,
    ) -> Result<Self, AgentSessionError> {
        let id_str = id.to_string();
        let session_id_str = session_id.to_string();
        let agent_type = data.agent_type.as_str();
        let state = AgentState::Created.as_str();

        let sql = format!(
            r#"INSERT INTO agent_sessions (id, session_id, agent_type, state, working_dir)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING {}"#,
            SELECT_FIELDS
        );
        let row = sqlx::query(&sql)
            .bind(&id_str)
            .bind(&session_id_str)
            .bind(agent_type)
            .bind(state)
            .bind(&data.working_dir)
            .fetch_one(pool)
            .await?;
        Ok(Self::from_row(row)?)
    }

    pub async fn update_state(
        pool: &SqlitePool,
        id: Uuid,
        new_state: AgentState,
    ) -> Result<Self, AgentSessionError> {
        // First fetch current state to validate transition
        let current = Self::find_by_id(pool, id)
            .await?
            .ok_or(AgentSessionError::NotFound)?;

        let current_state = current
            .state_enum()
            .unwrap_or(AgentState::Created);

        if !current_state.can_transition_to(new_state) {
            return Err(AgentSessionError::InvalidStateTransition {
                from: current_state.to_string(),
                to: new_state.to_string(),
            });
        }

        let id_str = id.to_string();
        let state_str = new_state.as_str();
        let completed_at = if new_state.is_terminal() {
            Some(Utc::now())
        } else {
            None
        };

        let sql = format!(
            r#"UPDATE agent_sessions
               SET state = $2, completed_at = $3, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE id = $1
               RETURNING {}"#,
            SELECT_FIELDS
        );
        let row = sqlx::query(&sql)
            .bind(&id_str)
            .bind(state_str)
            .bind(completed_at)
            .fetch_one(pool)
            .await?;
        Ok(Self::from_row(row)?)
    }

    pub async fn set_sdk_session_id(
        pool: &SqlitePool,
        id: Uuid,
        sdk_session_id: &str,
    ) -> Result<(), sqlx::Error> {
        let id_str = id.to_string();
        sqlx::query(
            r#"UPDATE agent_sessions
               SET sdk_session_id = $2, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE id = $1"#,
        )
        .bind(&id_str)
        .bind(sdk_session_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn set_pending_question(
        pool: &SqlitePool,
        id: Uuid,
        question: Option<&str>,
        tool_call_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let id_str = id.to_string();
        sqlx::query(
            r#"UPDATE agent_sessions
               SET pending_question = $2,
                   current_tool_call_id = $3,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE id = $1"#,
        )
        .bind(&id_str)
        .bind(question)
        .bind(tool_call_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn save_context(
        pool: &SqlitePool,
        id: Uuid,
        context: &str,
    ) -> Result<(), sqlx::Error> {
        let id_str = id.to_string();
        sqlx::query(
            r#"UPDATE agent_sessions
               SET context = $2, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE id = $1"#,
        )
        .bind(&id_str)
        .bind(context)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn set_error(
        pool: &SqlitePool,
        id: Uuid,
        error_message: &str,
    ) -> Result<(), sqlx::Error> {
        let id_str = id.to_string();
        let terminated = AgentState::Terminated.as_str();
        let now = Utc::now();
        sqlx::query(
            r#"UPDATE agent_sessions
               SET state = $2, error_message = $3, completed_at = $4,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE id = $1"#,
        )
        .bind(&id_str)
        .bind(terminated)
        .bind(error_message)
        .bind(now)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<(), sqlx::Error> {
        let id_str = id.to_string();
        sqlx::query("DELETE FROM agent_sessions WHERE id = $1")
            .bind(&id_str)
            .execute(pool)
            .await?;
        Ok(())
    }
}
