//! Agent Service - manages Claude Agent SDK sessions.
//!
//! This service integrates the agent-runtime crate with the broader
//! application services, handling agent lifecycle, event propagation,
//! and database persistence.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use agent_runtime::{
    AgentConfig, AgentEvent, AgentEventHandler, AgentType, SidecarManager,
};
use db::models::{
    agent_message::{AgentMessage, CreateAgentMessage, MessageRole},
    agent_pending_input::{AgentPendingInput, CreateAgentPendingInput, InputType},
    agent_session::{AgentSession, AgentState as DbAgentState, CreateAgentSession},
    workspace::Workspace,
};
use db::DBService;
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AgentServiceError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Agent session error: {0}")]
    AgentSession(#[from] db::models::agent_session::AgentSessionError),
    #[error("Sidecar error: {0}")]
    Sidecar(#[from] agent_runtime::sidecar::SidecarError),
    #[error("Bridge error: {0}")]
    Bridge(#[from] agent_runtime::bridge::BridgeError),
    #[error("Session not found: {0}")]
    SessionNotFound(Uuid),
    #[error("Workspace not found: {0}")]
    WorkspaceNotFound(Uuid),
    #[error("Invalid state: {0}")]
    InvalidState(String),
}

/// Event broadcast for real-time UI updates.
#[derive(Debug, Clone)]
pub struct AgentEventBroadcast {
    pub agent_session_id: Uuid,
    pub event: AgentEvent,
}

/// Agent service managing SDK sessions.
pub struct AgentService {
    db: DBService,
    sidecar_manager: Arc<SidecarManager>,
    /// Broadcast channel for agent events (for WebSocket streaming).
    event_tx: broadcast::Sender<AgentEventBroadcast>,
    /// Mapping of agent_session_id to workspace_id for routing.
    session_to_workspace: RwLock<HashMap<Uuid, Uuid>>,
}

impl AgentService {
    /// Create a new agent service.
    pub fn new(db: DBService, sidecar_path: PathBuf, socket_dir: PathBuf) -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(1000);

        let service = Arc::new(Self {
            db,
            sidecar_manager: Arc::new(SidecarManager::new(
                socket_dir,
                sidecar_path,
                Arc::new(NoOpEventHandler), // Will be replaced
            )),
            event_tx,
            session_to_workspace: RwLock::new(HashMap::new()),
        });

        // Recreate sidecar manager with proper event handler
        let _event_handler = AgentServiceEventHandler {
            db: service.db.clone(),
            event_tx: service.event_tx.clone(),
        };

        // Note: In a real implementation, we'd need to properly initialize
        // the sidecar manager with the event handler. For now, we'll handle
        // events through a different mechanism.

        service
    }

    /// Subscribe to agent events for a specific session.
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEventBroadcast> {
        self.event_tx.subscribe()
    }

    /// Create a new agent session for a workspace.
    pub async fn create_session(
        &self,
        workspace_id: Uuid,
        session_id: Uuid,
        agent_type: AgentType,
        working_dir: Option<String>,
    ) -> Result<AgentSession, AgentServiceError> {
        let pool = &self.db.pool;

        // Verify workspace exists
        let workspace = Workspace::find_by_id(pool, workspace_id)
            .await?
            .ok_or(AgentServiceError::WorkspaceNotFound(workspace_id))?;

        // Determine working directory
        let effective_working_dir = working_dir
            .or_else(|| workspace.agent_working_dir.clone())
            .unwrap_or_else(|| ".".to_string());

        // Create agent session in database
        let db_agent_type = match agent_type {
            AgentType::Coding => db::models::agent_session::AgentType::Coding,
            AgentType::Orchestration => db::models::agent_session::AgentType::Orchestration,
            AgentType::Deployment => db::models::agent_session::AgentType::Deployment,
        };

        let agent_session = AgentSession::create(
            pool,
            Uuid::new_v4(),
            session_id,
            &CreateAgentSession {
                agent_type: db_agent_type,
                working_dir: Some(effective_working_dir.clone()),
            },
        )
        .await?;

        // Track session to workspace mapping
        {
            let mut mapping = self.session_to_workspace.write().await;
            mapping.insert(agent_session.id, workspace_id);
        }

        // Start sidecar and create session
        let config = AgentConfig::new(effective_working_dir);
        self.sidecar_manager
            .create_session(workspace_id, agent_session.id, agent_type, config)
            .await?;

        // Update state to Idle
        let agent_session = AgentSession::update_state(pool, agent_session.id, DbAgentState::Idle)
            .await?;

        Ok(agent_session)
    }

    /// Send a query to an agent session.
    pub async fn query(
        &self,
        agent_session_id: Uuid,
        prompt: &str,
    ) -> Result<(), AgentServiceError> {
        let pool = &self.db.pool;

        // Get session and validate state
        let session = AgentSession::find_by_id(pool, agent_session_id)
            .await?
            .ok_or(AgentServiceError::SessionNotFound(agent_session_id))?;

        let current_state = session
            .state_enum()
            .ok_or_else(|| AgentServiceError::InvalidState(session.state.clone()))?;

        if !current_state.can_query() {
            return Err(AgentServiceError::InvalidState(format!(
                "Cannot query in state: {}",
                session.state
            )));
        }

        // Get workspace ID for this session
        let workspace_id = {
            let mapping = self.session_to_workspace.read().await;
            *mapping
                .get(&agent_session_id)
                .ok_or(AgentServiceError::SessionNotFound(agent_session_id))?
        };

        // Record user message
        AgentMessage::create(
            pool,
            Uuid::new_v4(),
            agent_session_id,
            &CreateAgentMessage {
                role: MessageRole::User,
                content: prompt.to_string(),
                metadata: None,
                token_count: None,
            },
        )
        .await?;

        // Update state to Executing
        AgentSession::update_state(pool, agent_session_id, DbAgentState::Executing).await?;

        // Send query to sidecar
        self.sidecar_manager
            .query(workspace_id, agent_session_id, prompt, None)
            .await?;

        Ok(())
    }

    /// Respond to a pending question.
    pub async fn respond_to_question(
        &self,
        agent_session_id: Uuid,
        input_id: Uuid,
        response: &str,
    ) -> Result<(), AgentServiceError> {
        let pool = &self.db.pool;

        // Validate session state
        let session = AgentSession::find_by_id(pool, agent_session_id)
            .await?
            .ok_or(AgentServiceError::SessionNotFound(agent_session_id))?;

        if session.state != DbAgentState::AwaitingInput.as_str() {
            return Err(AgentServiceError::InvalidState(
                "Session not awaiting input".to_string(),
            ));
        }

        // Get workspace ID
        let workspace_id = {
            let mapping = self.session_to_workspace.read().await;
            *mapping
                .get(&agent_session_id)
                .ok_or(AgentServiceError::SessionNotFound(agent_session_id))?
        };

        // Record response in pending input
        AgentPendingInput::respond(
            pool,
            input_id,
            &db::models::agent_pending_input::RespondToInput {
                response: Some(response.to_string()),
                approved: None,
            },
        )
        .await?;

        // Clear pending question
        AgentSession::set_pending_question(pool, agent_session_id, None, None).await?;

        // Update state
        AgentSession::update_state(pool, agent_session_id, DbAgentState::Executing).await?;

        // Resume sidecar with response
        self.sidecar_manager
            .resume_with_response(workspace_id, agent_session_id, input_id, response)
            .await?;

        Ok(())
    }

    /// Approve or deny a tool use request.
    pub async fn approve_tool(
        &self,
        agent_session_id: Uuid,
        input_id: Uuid,
        tool_call_id: &str,
        approved: bool,
        reason: Option<&str>,
    ) -> Result<(), AgentServiceError> {
        let pool = &self.db.pool;

        // Validate session state
        let session = AgentSession::find_by_id(pool, agent_session_id)
            .await?
            .ok_or(AgentServiceError::SessionNotFound(agent_session_id))?;

        if session.state != DbAgentState::AwaitingInput.as_str() {
            return Err(AgentServiceError::InvalidState(
                "Session not awaiting input".to_string(),
            ));
        }

        // Get workspace ID
        let workspace_id = {
            let mapping = self.session_to_workspace.read().await;
            *mapping
                .get(&agent_session_id)
                .ok_or(AgentServiceError::SessionNotFound(agent_session_id))?
        };

        // Record approval in pending input
        AgentPendingInput::respond(
            pool,
            input_id,
            &db::models::agent_pending_input::RespondToInput {
                response: reason.map(|s| s.to_string()),
                approved: Some(approved),
            },
        )
        .await?;

        // Clear pending
        AgentSession::set_pending_question(pool, agent_session_id, None, None).await?;

        // Update state
        AgentSession::update_state(pool, agent_session_id, DbAgentState::Executing).await?;

        // Send approval to sidecar
        self.sidecar_manager
            .approve_tool(workspace_id, agent_session_id, input_id, tool_call_id, approved, reason)
            .await?;

        Ok(())
    }

    /// Pause an executing agent.
    pub async fn pause(&self, agent_session_id: Uuid) -> Result<(), AgentServiceError> {
        let pool = &self.db.pool;

        let _session = AgentSession::find_by_id(pool, agent_session_id)
            .await?
            .ok_or(AgentServiceError::SessionNotFound(agent_session_id))?;

        let workspace_id = {
            let mapping = self.session_to_workspace.read().await;
            *mapping
                .get(&agent_session_id)
                .ok_or(AgentServiceError::SessionNotFound(agent_session_id))?
        };

        // Send pause to sidecar
        self.sidecar_manager
            .pause_session(workspace_id, agent_session_id)
            .await?;

        // State update will come from sidecar event
        Ok(())
    }

    /// Resume a paused agent.
    pub async fn resume(&self, agent_session_id: Uuid) -> Result<(), AgentServiceError> {
        let pool = &self.db.pool;

        let session = AgentSession::find_by_id(pool, agent_session_id)
            .await?
            .ok_or(AgentServiceError::SessionNotFound(agent_session_id))?;

        if session.state != DbAgentState::Paused.as_str() {
            return Err(AgentServiceError::InvalidState(
                "Session not paused".to_string(),
            ));
        }

        let workspace_id = {
            let mapping = self.session_to_workspace.read().await;
            *mapping
                .get(&agent_session_id)
                .ok_or(AgentServiceError::SessionNotFound(agent_session_id))?
        };

        // Send resume to sidecar
        self.sidecar_manager
            .resume_session(workspace_id, agent_session_id)
            .await?;

        Ok(())
    }

    /// Terminate an agent session.
    pub async fn terminate(&self, agent_session_id: Uuid) -> Result<(), AgentServiceError> {
        let pool = &self.db.pool;

        let _session = AgentSession::find_by_id(pool, agent_session_id)
            .await?
            .ok_or(AgentServiceError::SessionNotFound(agent_session_id))?;

        let workspace_id = {
            let mapping = self.session_to_workspace.read().await;
            mapping.get(&agent_session_id).copied()
        };

        // Terminate in sidecar if we have a workspace mapping
        if let Some(workspace_id) = workspace_id {
            let _ = self
                .sidecar_manager
                .terminate_session(workspace_id, agent_session_id)
                .await;
        }

        // Update state in database
        AgentSession::update_state(pool, agent_session_id, DbAgentState::Terminated).await?;

        // Remove from mapping
        {
            let mut mapping = self.session_to_workspace.write().await;
            mapping.remove(&agent_session_id);
        }

        Ok(())
    }

    /// Stop the sidecar for a workspace.
    pub async fn stop_workspace_sidecar(&self, workspace_id: Uuid) -> Result<(), AgentServiceError> {
        self.sidecar_manager.stop_sidecar(workspace_id).await?;
        Ok(())
    }

    /// Stop all sidecars (for shutdown).
    pub async fn shutdown(&self) {
        self.sidecar_manager.stop_all().await;
    }
}

/// No-op event handler for initial creation.
struct NoOpEventHandler;

#[async_trait::async_trait]
impl AgentEventHandler for NoOpEventHandler {
    async fn handle_event(&self, _event: AgentEvent) {}
    async fn handle_connection_error(&self, _error: &str) {}
}

/// Event handler that persists events to database and broadcasts.
struct AgentServiceEventHandler {
    db: DBService,
    event_tx: broadcast::Sender<AgentEventBroadcast>,
}

#[async_trait::async_trait]
impl AgentEventHandler for AgentServiceEventHandler {
    async fn handle_event(&self, event: AgentEvent) {
        let agent_session_id = event.session_id();
        let pool = &self.db.pool;

        // Handle different event types
        match &event {
            AgentEvent::StateChangedEvent {
                session_id,
                to_state,
                ..
            } => {
                // Update database state
                if let Some(db_state) = DbAgentState::from_str(to_state.as_str()) {
                    if let Err(e) = AgentSession::update_state(pool, *session_id, db_state).await {
                        tracing::error!("Failed to update agent session state: {}", e);
                    }
                }
            }
            AgentEvent::MessageStreamEvent {
                session_id,
                content_type,
                content,
                metadata,
            } => {
                // Store assistant messages
                if content_type == "text" {
                    let _ = AgentMessage::create(
                        pool,
                        Uuid::new_v4(),
                        *session_id,
                        &CreateAgentMessage {
                            role: MessageRole::Assistant,
                            content: content.clone(),
                            metadata: metadata.as_ref().and_then(|m| serde_json::to_value(m).ok()),
                            token_count: None,
                        },
                    )
                    .await;
                }
            }
            AgentEvent::QuestionAskedEvent {
                session_id,
                question,
                input_id,
            } => {
                // Create pending input
                let _ = AgentPendingInput::create(
                    pool,
                    *input_id,
                    *session_id,
                    &CreateAgentPendingInput {
                        input_type: InputType::Question,
                        prompt: question.clone(),
                        tool_call_id: None,
                        tool_name: None,
                        tool_input: None,
                    },
                )
                .await;

                // Update session with pending question
                let _ = AgentSession::set_pending_question(pool, *session_id, Some(question), None).await;
            }
            AgentEvent::ApprovalNeededEvent {
                session_id,
                tool_call_id,
                tool_name,
                tool_input,
                input_id,
            } => {
                // Create pending input
                let _ = AgentPendingInput::create(
                    pool,
                    *input_id,
                    *session_id,
                    &CreateAgentPendingInput {
                        input_type: InputType::Approval,
                        prompt: format!("Approve tool: {}", tool_name),
                        tool_call_id: Some(tool_call_id.clone()),
                        tool_name: Some(tool_name.clone()),
                        tool_input: Some(tool_input.clone()),
                    },
                )
                .await;

                // Update session
                let _ = AgentSession::set_pending_question(
                    pool,
                    *session_id,
                    Some(&format!("Approve tool: {}", tool_name)),
                    Some(tool_call_id),
                )
                .await;
            }
            AgentEvent::ErrorEvent { session_id, error } => {
                let _ = AgentSession::set_error(pool, *session_id, error).await;
            }
            _ => {}
        }

        // Broadcast event for WebSocket clients
        let _ = self.event_tx.send(AgentEventBroadcast {
            agent_session_id,
            event,
        });
    }

    async fn handle_connection_error(&self, error: &str) {
        tracing::error!("Agent sidecar connection error: {}", error);
    }
}
