//! Type definitions for agent runtime.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// Types of agents supported by the runtime.
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
}

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Agent state machine states.
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

    pub fn is_terminal(&self) -> bool {
        matches!(self, AgentState::Completed | AgentState::Terminated)
    }

    pub fn can_query(&self) -> bool {
        matches!(self, AgentState::Idle)
    }
}

impl std::fmt::Display for AgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Configuration for an agent session.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct AgentConfig {
    pub working_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: u32,
    #[serde(default = "default_true")]
    pub auto_approve_read_tools: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub env_vars: std::collections::HashMap<String, String>,
}

fn default_max_context_tokens() -> u32 {
    200000
}

fn default_true() -> bool {
    true
}

impl AgentConfig {
    pub fn new(working_dir: String) -> Self {
        Self {
            working_dir,
            system_prompt: None,
            max_context_tokens: 200000,
            auto_approve_read_tools: true,
            allowed_tools: None,
            env_vars: std::collections::HashMap::new(),
        }
    }
}

/// Serializable context for pause/resume.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SessionContext {
    pub messages: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdk_session_id: Option<String>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

/// Commands sent to the sidecar.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum AgentCommand {
    #[serde(rename = "create_session")]
    CreateSession {
        session_id: Uuid,
        agent_type: AgentType,
        config: AgentConfig,
    },
    #[serde(rename = "query")]
    Query {
        session_id: Uuid,
        prompt: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        context: Option<SessionContext>,
    },
    #[serde(rename = "resume_with_response")]
    ResumeWithResponse {
        session_id: Uuid,
        input_id: Uuid,
        response: String,
    },
    #[serde(rename = "approve_tool")]
    ApproveTool {
        session_id: Uuid,
        input_id: Uuid,
        tool_call_id: String,
        approved: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    #[serde(rename = "interrupt")]
    Interrupt { session_id: Uuid },
    #[serde(rename = "pause")]
    Pause { session_id: Uuid },
    #[serde(rename = "resume")]
    Resume { session_id: Uuid },
    #[serde(rename = "terminate")]
    Terminate { session_id: Uuid },
    #[serde(rename = "get_context")]
    GetContext { session_id: Uuid },
    #[serde(rename = "list_sessions")]
    ListSessions,
    #[serde(rename = "ping")]
    Ping,
}

/// JSON-RPC request wrapper.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    pub params: serde_json::Value,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: &str, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        }
    }
}

/// JSON-RPC response wrapper.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcResponse {
    pub id: Option<u64>,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC error.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}
