//! Event types received from the agent sidecar.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::types::AgentState;

/// Events emitted by agent sessions.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "event_type")]
pub enum AgentEvent {
    /// Agent state has changed.
    StateChangedEvent {
        session_id: Uuid,
        from_state: AgentState,
        to_state: AgentState,
    },
    /// Streaming message content.
    MessageStreamEvent {
        session_id: Uuid,
        content_type: String, // 'text', 'thinking', 'tool_use', 'tool_result'
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
    /// Agent is using a tool.
    ToolUseEvent {
        session_id: Uuid,
        tool_call_id: String,
        tool_name: String,
        tool_input: serde_json::Value,
    },
    /// Tool execution result.
    ToolResultEvent {
        session_id: Uuid,
        tool_call_id: String,
        result: serde_json::Value,
        #[serde(default)]
        is_error: bool,
    },
    /// Agent is asking a question.
    QuestionAskedEvent {
        session_id: Uuid,
        question: String,
        input_id: Uuid,
    },
    /// Agent needs tool approval.
    ApprovalNeededEvent {
        session_id: Uuid,
        tool_call_id: String,
        tool_name: String,
        tool_input: serde_json::Value,
        input_id: Uuid,
    },
    /// Agent execution completed.
    CompletedEvent {
        session_id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<String>,
    },
    /// Agent execution error.
    ErrorEvent { session_id: Uuid, error: String },
}

impl AgentEvent {
    /// Get the session ID from any event type.
    pub fn session_id(&self) -> Uuid {
        match self {
            AgentEvent::StateChangedEvent { session_id, .. } => *session_id,
            AgentEvent::MessageStreamEvent { session_id, .. } => *session_id,
            AgentEvent::ToolUseEvent { session_id, .. } => *session_id,
            AgentEvent::ToolResultEvent { session_id, .. } => *session_id,
            AgentEvent::QuestionAskedEvent { session_id, .. } => *session_id,
            AgentEvent::ApprovalNeededEvent { session_id, .. } => *session_id,
            AgentEvent::CompletedEvent { session_id, .. } => *session_id,
            AgentEvent::ErrorEvent { session_id, .. } => *session_id,
        }
    }
}

/// Trait for handling agent events.
#[async_trait::async_trait]
pub trait AgentEventHandler: Send + Sync {
    /// Handle an agent event.
    async fn handle_event(&self, event: AgentEvent);

    /// Handle a connection error.
    async fn handle_connection_error(&self, error: &str);
}
