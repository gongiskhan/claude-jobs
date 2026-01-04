//! Agent Runtime Bridge - communicates with the Python sidecar.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot, RwLock};
use uuid::Uuid;

use crate::events::{AgentEvent, AgentEventHandler};
use crate::types::{AgentConfig, AgentState, AgentType, JsonRpcRequest, JsonRpcResponse, SessionContext};

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Request timeout")]
    Timeout,
    #[error("Sidecar error: {code} - {message}")]
    SidecarError { code: i32, message: String },
    #[error("Session not found: {0}")]
    SessionNotFound(Uuid),
    #[error("Not connected")]
    NotConnected,
    #[error("Channel closed")]
    ChannelClosed,
}

type PendingRequests = Arc<RwLock<HashMap<u64, oneshot::Sender<Result<serde_json::Value, BridgeError>>>>>;

/// Bridge for communicating with the agent sidecar.
pub struct AgentRuntimeBridge {
    socket_path: String,
    request_id: AtomicU64,
    pending_requests: PendingRequests,
    write_tx: Option<mpsc::Sender<String>>,
    event_handler: Arc<dyn AgentEventHandler>,
    connected: Arc<std::sync::atomic::AtomicBool>,
}

impl AgentRuntimeBridge {
    /// Create a new bridge (not yet connected).
    pub fn new(socket_path: &str, event_handler: Arc<dyn AgentEventHandler>) -> Self {
        Self {
            socket_path: socket_path.to_string(),
            request_id: AtomicU64::new(1),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            write_tx: None,
            event_handler,
            connected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Check if connected to the sidecar.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Connect to the sidecar.
    pub async fn connect(&mut self) -> Result<(), BridgeError> {
        let stream = UnixStream::connect(&self.socket_path).await?;
        let (read_half, write_half) = stream.into_split();

        // Create write channel
        let (write_tx, mut write_rx) = mpsc::channel::<String>(100);
        self.write_tx = Some(write_tx);
        self.connected.store(true, Ordering::SeqCst);

        // Spawn write task
        let connected = self.connected.clone();
        tokio::spawn(async move {
            let mut write_half = write_half;
            while let Some(message) = write_rx.recv().await {
                if let Err(e) = write_half.write_all(message.as_bytes()).await {
                    tracing::error!("Failed to write to sidecar: {}", e);
                    break;
                }
                if let Err(e) = write_half.flush().await {
                    tracing::error!("Failed to flush to sidecar: {}", e);
                    break;
                }
            }
            connected.store(false, Ordering::SeqCst);
        });

        // Spawn read task
        let pending_requests = self.pending_requests.clone();
        let event_handler = self.event_handler.clone();
        let connected = self.connected.clone();

        tokio::spawn(async move {
            let mut reader = BufReader::new(read_half);
            let mut line = String::new();

            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        tracing::info!("Sidecar connection closed");
                        break;
                    }
                    Ok(_) => {
                        if let Err(e) = Self::handle_message(&line, &pending_requests, &event_handler).await {
                            tracing::warn!("Failed to handle message: {}", e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to read from sidecar: {}", e);
                        event_handler.handle_connection_error(&e.to_string()).await;
                        break;
                    }
                }
            }

            connected.store(false, Ordering::SeqCst);
        });

        // Verify connection with ping
        self.ping().await?;

        tracing::info!("Connected to agent sidecar at {}", self.socket_path);
        Ok(())
    }

    async fn handle_message(
        line: &str,
        pending_requests: &PendingRequests,
        event_handler: &Arc<dyn AgentEventHandler>,
    ) -> Result<(), BridgeError> {
        let message: serde_json::Value = serde_json::from_str(line.trim())?;

        // Check if this is a response to a request
        if let Some(id) = message.get("id").and_then(|v| v.as_u64()) {
            let response: JsonRpcResponse = serde_json::from_value(message)?;
            let mut pending = pending_requests.write().await;
            if let Some(sender) = pending.remove(&id) {
                let result = if let Some(error) = response.error {
                    Err(BridgeError::SidecarError {
                        code: error.code,
                        message: error.message,
                    })
                } else {
                    Ok(response.result.unwrap_or(serde_json::Value::Null))
                };
                let _ = sender.send(result);
            }
            return Ok(());
        }

        // Check if this is an event notification
        if message.get("method").and_then(|v| v.as_str()) == Some("event") {
            if let Some(params) = message.get("params") {
                let event: AgentEvent = serde_json::from_value(params.clone())?;
                event_handler.handle_event(event).await;
            }
        }

        Ok(())
    }

    async fn send_request(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value, BridgeError> {
        let write_tx = self.write_tx.as_ref().ok_or(BridgeError::NotConnected)?;

        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let request = JsonRpcRequest::new(id, method, params);
        let message = serde_json::to_string(&request)? + "\n";

        // Set up response channel
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending_requests.write().await;
            pending.insert(id, tx);
        }

        // Send request
        write_tx.send(message).await.map_err(|_| BridgeError::ChannelClosed)?;

        // Wait for response with timeout
        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(BridgeError::ChannelClosed),
            Err(_) => {
                // Remove pending request on timeout
                let mut pending = self.pending_requests.write().await;
                pending.remove(&id);
                Err(BridgeError::Timeout)
            }
        }
    }

    /// Ping the sidecar.
    pub async fn ping(&self) -> Result<(), BridgeError> {
        self.send_request("ping", serde_json::json!({})).await?;
        Ok(())
    }

    /// Create a new agent session.
    pub async fn create_session(
        &self,
        session_id: Uuid,
        agent_type: AgentType,
        config: AgentConfig,
    ) -> Result<AgentState, BridgeError> {
        let params = serde_json::json!({
            "session_id": session_id.to_string(),
            "agent_type": agent_type.as_str(),
            "config": config,
        });

        let result = self.send_request("create_session", params).await?;
        let state_str = result
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or("created");

        Ok(AgentState::from_str(state_str).unwrap_or(AgentState::Created))
    }

    /// Send a query to an agent session.
    pub async fn query(
        &self,
        session_id: Uuid,
        prompt: &str,
        context: Option<SessionContext>,
    ) -> Result<(), BridgeError> {
        let mut params = serde_json::json!({
            "session_id": session_id.to_string(),
            "prompt": prompt,
        });

        if let Some(ctx) = context {
            params["context"] = serde_json::to_value(ctx)?;
        }

        self.send_request("query", params).await?;
        Ok(())
    }

    /// Resume with user response.
    pub async fn resume_with_response(
        &self,
        session_id: Uuid,
        input_id: Uuid,
        response: &str,
    ) -> Result<(), BridgeError> {
        let params = serde_json::json!({
            "session_id": session_id.to_string(),
            "input_id": input_id.to_string(),
            "response": response,
        });

        self.send_request("resume_with_response", params).await?;
        Ok(())
    }

    /// Approve or deny tool use.
    pub async fn approve_tool(
        &self,
        session_id: Uuid,
        input_id: Uuid,
        tool_call_id: &str,
        approved: bool,
        reason: Option<&str>,
    ) -> Result<(), BridgeError> {
        let mut params = serde_json::json!({
            "session_id": session_id.to_string(),
            "input_id": input_id.to_string(),
            "tool_call_id": tool_call_id,
            "approved": approved,
        });

        if let Some(r) = reason {
            params["reason"] = serde_json::Value::String(r.to_string());
        }

        self.send_request("approve_tool", params).await?;
        Ok(())
    }

    /// Interrupt current execution.
    pub async fn interrupt(&self, session_id: Uuid) -> Result<(), BridgeError> {
        let params = serde_json::json!({
            "session_id": session_id.to_string(),
        });

        self.send_request("interrupt", params).await?;
        Ok(())
    }

    /// Pause execution.
    pub async fn pause(&self, session_id: Uuid) -> Result<(), BridgeError> {
        let params = serde_json::json!({
            "session_id": session_id.to_string(),
        });

        self.send_request("pause", params).await?;
        Ok(())
    }

    /// Resume paused execution.
    pub async fn resume(&self, session_id: Uuid) -> Result<(), BridgeError> {
        let params = serde_json::json!({
            "session_id": session_id.to_string(),
        });

        self.send_request("resume", params).await?;
        Ok(())
    }

    /// Terminate a session.
    pub async fn terminate(&self, session_id: Uuid) -> Result<(), BridgeError> {
        let params = serde_json::json!({
            "session_id": session_id.to_string(),
        });

        self.send_request("terminate", params).await?;
        Ok(())
    }

    /// Get serialized context for persistence.
    pub async fn get_context(&self, session_id: Uuid) -> Result<SessionContext, BridgeError> {
        let params = serde_json::json!({
            "session_id": session_id.to_string(),
        });

        let result = self.send_request("get_context", params).await?;
        let context: SessionContext = serde_json::from_value(result)?;
        Ok(context)
    }

    /// List all sessions.
    pub async fn list_sessions(&self) -> Result<Vec<(Uuid, AgentState)>, BridgeError> {
        let result = self.send_request("list_sessions", serde_json::json!({})).await?;

        let sessions = result
            .get("sessions")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let session_id = item.get("session_id")?.as_str()?;
                        let state_str = item.get("state")?.as_str()?;
                        Some((
                            Uuid::parse_str(session_id).ok()?,
                            AgentState::from_str(state_str)?,
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(sessions)
    }
}
