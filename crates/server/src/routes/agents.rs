//! Agent session API routes.
//!
//! These routes provide the API for managing Claude Agent SDK sessions.

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State, WebSocketUpgrade},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use db::models::{
    agent_message::{AgentMessage, CreateAgentMessage, MessageRole},
    agent_pending_input::{
        AgentPendingInput, CreateAgentPendingInput, InputType, RespondToInput,
    },
    agent_session::{AgentSession, AgentState, AgentType, CreateAgentSession},
    session::Session,
    workspace::Workspace,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

/// Request to create a new agent session.
#[derive(Debug, Deserialize, TS)]
pub struct CreateAgentSessionRequest {
    pub agent_type: Option<String>,
    pub working_dir: Option<String>,
}

/// Request to send a query to an agent.
#[derive(Debug, Deserialize, TS)]
pub struct AgentQueryRequest {
    pub prompt: String,
}

/// Request to respond to a pending question.
#[derive(Debug, Deserialize, TS)]
pub struct AgentRespondRequest {
    pub response: String,
}

/// Request to approve or deny tool use.
#[derive(Debug, Deserialize, TS)]
pub struct ApprovalRequest {
    pub tool_call_id: String,
    pub approved: bool,
    pub reason: Option<String>,
}

/// Response for agent session creation.
#[derive(Debug, Serialize, TS)]
pub struct AgentSessionResponse {
    pub id: Uuid,
    pub session_id: Uuid,
    pub agent_type: String,
    pub state: String,
    pub created_at: String,
}

impl From<AgentSession> for AgentSessionResponse {
    fn from(session: AgentSession) -> Self {
        Self {
            id: session.id,
            session_id: session.session_id,
            agent_type: session.agent_type,
            state: session.state,
            created_at: session.created_at.to_rfc3339(),
        }
    }
}

/// Query parameters for listing agent sessions.
#[derive(Debug, Deserialize)]
pub struct ListAgentSessionsQuery {
    pub session_id: Option<Uuid>,
    pub state: Option<String>,
}

/// Create a new agent session for a workspace.
pub async fn create_agent_session(
    State(deployment): State<DeploymentImpl>,
    Path(workspace_id): Path<Uuid>,
    Json(request): Json<CreateAgentSessionRequest>,
) -> Result<Json<ApiResponse<AgentSessionResponse>>, ApiError> {
    let pool = &deployment.db().pool;

    // Verify workspace exists
    let workspace = Workspace::find_by_id(pool, workspace_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Workspace not found".to_string()))?;

    // Get or create a session for this workspace
    let session = Session::find_latest_by_workspace_id(pool, workspace_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("No session found for workspace".to_string()))?;

    // Parse agent type
    let agent_type = request
        .agent_type
        .as_deref()
        .and_then(AgentType::from_str)
        .unwrap_or(AgentType::Coding);

    // Determine working directory
    let working_dir = request.working_dir.or_else(|| workspace.agent_working_dir.clone());

    // Create agent session
    let agent_session = AgentSession::create(
        pool,
        Uuid::new_v4(),
        session.id,
        &CreateAgentSession {
            agent_type,
            working_dir,
        },
    )
    .await
    .map_err(|e| ApiError::Database(e.into()))?;

    // TODO: Start the sidecar process for this workspace and initialize the agent
    // This will be implemented in Phase 2 when we integrate with the agent-runtime crate

    Ok(Json(ApiResponse::success(agent_session.into())))
}

/// Get an agent session by ID.
pub async fn get_agent_session(
    State(deployment): State<DeploymentImpl>,
    Path(agent_session_id): Path<Uuid>,
) -> Result<Json<ApiResponse<AgentSessionResponse>>, ApiError> {
    let pool = &deployment.db().pool;

    let agent_session = AgentSession::find_by_id(pool, agent_session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Agent session not found".to_string()))?;

    Ok(Json(ApiResponse::success(agent_session.into())))
}

/// List agent sessions.
pub async fn list_agent_sessions(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListAgentSessionsQuery>,
) -> Result<Json<ApiResponse<Vec<AgentSessionResponse>>>, ApiError> {
    let pool = &deployment.db().pool;

    let sessions = if let Some(session_id) = query.session_id {
        AgentSession::find_by_session_id(pool, session_id).await?
    } else if query.state.as_deref() == Some("awaiting_input") {
        AgentSession::find_awaiting_input(pool).await?
    } else {
        AgentSession::find_active(pool).await?
    };

    let responses: Vec<AgentSessionResponse> = sessions.into_iter().map(Into::into).collect();
    Ok(Json(ApiResponse::success(responses)))
}

/// Send a query to an agent session.
pub async fn query_agent(
    State(deployment): State<DeploymentImpl>,
    Path(agent_session_id): Path<Uuid>,
    Json(request): Json<AgentQueryRequest>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let pool = &deployment.db().pool;

    // Verify agent session exists and is in Idle state
    let agent_session = AgentSession::find_by_id(pool, agent_session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Agent session not found".to_string()))?;

    let current_state = agent_session
        .state_enum()
        .ok_or_else(|| ApiError::BadRequest("Invalid agent state".to_string()))?;

    if !current_state.can_query() {
        return Err(ApiError::BadRequest(format!(
            "Agent cannot receive queries in state: {}",
            agent_session.state
        )));
    }

    // Record the user message
    AgentMessage::create(
        pool,
        Uuid::new_v4(),
        agent_session_id,
        &CreateAgentMessage {
            role: MessageRole::User,
            content: request.prompt.clone(),
            metadata: None,
            token_count: None,
        },
    )
    .await?;

    // Update state to Executing
    AgentSession::update_state(pool, agent_session_id, AgentState::Executing)
        .await
        .map_err(|e| ApiError::Database(e.into()))?;

    // TODO: Send query to sidecar via agent-runtime bridge
    // This will be implemented in Phase 2

    Ok(Json(ApiResponse::success(())))
}

/// Respond to a pending question from an agent.
pub async fn respond_to_question(
    State(deployment): State<DeploymentImpl>,
    Path((agent_session_id, input_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<AgentRespondRequest>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let pool = &deployment.db().pool;

    // Verify agent session exists and is awaiting input
    let agent_session = AgentSession::find_by_id(pool, agent_session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Agent session not found".to_string()))?;

    if agent_session.state != AgentState::AwaitingInput.as_str() {
        return Err(ApiError::BadRequest(
            "Agent is not awaiting input".to_string(),
        ));
    }

    // Find and respond to the pending input
    let pending_input = AgentPendingInput::find_by_id(pool, input_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Pending input not found".to_string()))?;

    if pending_input.agent_session_id != agent_session_id {
        return Err(ApiError::BadRequest(
            "Pending input does not belong to this session".to_string(),
        ));
    }

    // Record the response
    AgentPendingInput::respond(
        pool,
        input_id,
        &RespondToInput {
            response: Some(request.response.clone()),
            approved: None,
        },
    )
    .await?;

    // Clear pending question from session
    AgentSession::set_pending_question(pool, agent_session_id, None, None).await?;

    // Update state back to Executing
    AgentSession::update_state(pool, agent_session_id, AgentState::Executing)
        .await
        .map_err(|e| ApiError::Database(e.into()))?;

    // TODO: Send response to sidecar to resume execution

    Ok(Json(ApiResponse::success(())))
}

/// Approve or deny a tool use request.
pub async fn approve_tool_use(
    State(deployment): State<DeploymentImpl>,
    Path((agent_session_id, input_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<ApprovalRequest>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let pool = &deployment.db().pool;

    // Verify agent session exists and is awaiting input
    let agent_session = AgentSession::find_by_id(pool, agent_session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Agent session not found".to_string()))?;

    if agent_session.state != AgentState::AwaitingInput.as_str() {
        return Err(ApiError::BadRequest(
            "Agent is not awaiting input".to_string(),
        ));
    }

    // Find and respond to the pending input
    let pending_input = AgentPendingInput::find_by_id(pool, input_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Pending input not found".to_string()))?;

    if pending_input.agent_session_id != agent_session_id {
        return Err(ApiError::BadRequest(
            "Pending input does not belong to this session".to_string(),
        ));
    }

    if pending_input.input_type != InputType::Approval.as_str() {
        return Err(ApiError::BadRequest(
            "Pending input is not an approval request".to_string(),
        ));
    }

    // Record the approval/denial
    AgentPendingInput::respond(
        pool,
        input_id,
        &RespondToInput {
            response: request.reason.clone(),
            approved: Some(request.approved),
        },
    )
    .await?;

    // Clear pending from session
    AgentSession::set_pending_question(pool, agent_session_id, None, None).await?;

    // Update state back to Executing
    AgentSession::update_state(pool, agent_session_id, AgentState::Executing)
        .await
        .map_err(|e| ApiError::Database(e.into()))?;

    // TODO: Send approval to sidecar

    Ok(Json(ApiResponse::success(())))
}

/// Pause an executing agent.
pub async fn pause_agent(
    State(deployment): State<DeploymentImpl>,
    Path(agent_session_id): Path<Uuid>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let pool = &deployment.db().pool;

    let agent_session = AgentSession::find_by_id(pool, agent_session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Agent session not found".to_string()))?;

    let current_state = agent_session
        .state_enum()
        .ok_or_else(|| ApiError::BadRequest("Invalid agent state".to_string()))?;

    if !matches!(current_state, AgentState::Executing | AgentState::AwaitingInput) {
        return Err(ApiError::BadRequest(format!(
            "Cannot pause agent in state: {}",
            agent_session.state
        )));
    }

    // Update state to Paused
    AgentSession::update_state(pool, agent_session_id, AgentState::Paused)
        .await
        .map_err(|e| ApiError::Database(e.into()))?;

    // TODO: Send pause command to sidecar

    Ok(Json(ApiResponse::success(())))
}

/// Resume a paused agent.
pub async fn resume_agent(
    State(deployment): State<DeploymentImpl>,
    Path(agent_session_id): Path<Uuid>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let pool = &deployment.db().pool;

    let agent_session = AgentSession::find_by_id(pool, agent_session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Agent session not found".to_string()))?;

    if agent_session.state != AgentState::Paused.as_str() {
        return Err(ApiError::BadRequest(
            "Agent is not paused".to_string(),
        ));
    }

    // Update state to Executing
    AgentSession::update_state(pool, agent_session_id, AgentState::Executing)
        .await
        .map_err(|e| ApiError::Database(e.into()))?;

    // TODO: Send resume command to sidecar

    Ok(Json(ApiResponse::success(())))
}

/// Terminate an agent session.
pub async fn terminate_agent(
    State(deployment): State<DeploymentImpl>,
    Path(agent_session_id): Path<Uuid>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let pool = &deployment.db().pool;

    let agent_session = AgentSession::find_by_id(pool, agent_session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Agent session not found".to_string()))?;

    let current_state = agent_session
        .state_enum()
        .ok_or_else(|| ApiError::BadRequest("Invalid agent state".to_string()))?;

    if current_state.is_terminal() {
        return Err(ApiError::BadRequest(
            "Agent is already terminated".to_string(),
        ));
    }

    // Update state to Terminated
    AgentSession::update_state(pool, agent_session_id, AgentState::Terminated)
        .await
        .map_err(|e| ApiError::Database(e.into()))?;

    // TODO: Send terminate command to sidecar

    Ok(Json(ApiResponse::success(())))
}

/// Get messages for an agent session.
pub async fn get_agent_messages(
    State(deployment): State<DeploymentImpl>,
    Path(agent_session_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<AgentMessage>>>, ApiError> {
    let pool = &deployment.db().pool;

    // Verify session exists
    let _ = AgentSession::find_by_id(pool, agent_session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Agent session not found".to_string()))?;

    let messages = AgentMessage::find_by_agent_session_id(pool, agent_session_id).await?;
    Ok(Json(ApiResponse::success(messages)))
}

/// Get pending inputs for an agent session.
pub async fn get_pending_inputs(
    State(deployment): State<DeploymentImpl>,
    Path(agent_session_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<AgentPendingInput>>>, ApiError> {
    let pool = &deployment.db().pool;

    // Verify session exists
    let _ = AgentSession::find_by_id(pool, agent_session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Agent session not found".to_string()))?;

    let pending = AgentPendingInput::find_pending(pool, agent_session_id).await?;
    Ok(Json(ApiResponse::success(pending)))
}

/// Stream agent events via WebSocket.
pub async fn stream_agent_events(
    ws: WebSocketUpgrade,
    State(deployment): State<DeploymentImpl>,
    Path(agent_session_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let pool = &deployment.db().pool;

    // Verify session exists
    let _ = AgentSession::find_by_id(pool, agent_session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Agent session not found".to_string()))?;

    // TODO: Implement WebSocket streaming for agent events
    // This will connect to the sidecar and forward events to the client

    Ok(ws.on_upgrade(move |socket| async move {
        // Placeholder - will be implemented in Phase 2
        tracing::info!("WebSocket connection for agent session {}", agent_session_id);
    }))
}

/// Build the agent routes.
pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let agent_session_routes = Router::new()
        .route("/", get(get_agent_session).delete(terminate_agent))
        .route("/query", post(query_agent))
        .route("/pause", post(pause_agent))
        .route("/resume", post(resume_agent))
        .route("/messages", get(get_agent_messages))
        .route("/pending-inputs", get(get_pending_inputs))
        .route("/respond/{input_id}", post(respond_to_question))
        .route("/approve/{input_id}", post(approve_tool_use))
        .route("/events", get(stream_agent_events));

    Router::new()
        .route("/", get(list_agent_sessions))
        .route("/workspaces/{workspace_id}", post(create_agent_session))
        .nest("/{agent_session_id}", agent_session_routes)
}
