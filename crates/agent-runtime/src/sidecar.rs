//! Sidecar process manager.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use thiserror::Error;
use tokio::process::{Child, Command};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::bridge::AgentRuntimeBridge;
use crate::events::AgentEventHandler;

#[derive(Debug, Error)]
pub enum SidecarError {
    #[error("Failed to spawn sidecar: {0}")]
    SpawnFailed(#[from] std::io::Error),
    #[error("Sidecar not found for workspace: {0}")]
    NotFound(Uuid),
    #[error("Bridge error: {0}")]
    Bridge(#[from] crate::bridge::BridgeError),
    #[error("Python not found")]
    PythonNotFound,
}

/// Information about a running sidecar.
#[allow(dead_code)]
struct SidecarInfo {
    workspace_id: Uuid,
    process: Child,
    socket_path: PathBuf,
    bridge: AgentRuntimeBridge,
}

/// Manages sidecar processes (one per workspace).
pub struct SidecarManager {
    /// Base directory for socket files.
    socket_dir: PathBuf,
    /// Path to the agent-sidecar Python package.
    sidecar_path: PathBuf,
    /// Running sidecars by workspace ID.
    sidecars: RwLock<HashMap<Uuid, SidecarInfo>>,
    /// Event handler for all sidecars.
    event_handler: Arc<dyn AgentEventHandler>,
}

impl SidecarManager {
    /// Create a new sidecar manager.
    pub fn new(
        socket_dir: PathBuf,
        sidecar_path: PathBuf,
        event_handler: Arc<dyn AgentEventHandler>,
    ) -> Self {
        Self {
            socket_dir,
            sidecar_path,
            sidecars: RwLock::new(HashMap::new()),
            event_handler,
        }
    }

    /// Get the socket path for a workspace.
    fn socket_path(&self, workspace_id: Uuid) -> PathBuf {
        self.socket_dir.join(format!("agent-sidecar-{}.sock", workspace_id))
    }

    /// Start a sidecar for a workspace.
    pub async fn start_sidecar(&self, workspace_id: Uuid) -> Result<(), SidecarError> {
        // Check if already running
        {
            let sidecars = self.sidecars.read().await;
            if sidecars.contains_key(&workspace_id) {
                tracing::debug!("Sidecar already running for workspace {}", workspace_id);
                return Ok(());
            }
        }

        let socket_path = self.socket_path(workspace_id);

        // Find Python executable
        let python = self.find_python().await?;

        // Build command
        let mut cmd = Command::new(&python);
        cmd.arg("-m")
            .arg("agent_sidecar")
            .arg("--socket")
            .arg(&socket_path)
            .arg("--workspace-id")
            .arg(workspace_id.to_string())
            .current_dir(&self.sidecar_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Add PYTHONPATH if needed
        if let Some(parent) = self.sidecar_path.parent() {
            cmd.env("PYTHONPATH", parent.join("src"));
        }

        tracing::info!("Starting sidecar for workspace {} at {:?}", workspace_id, socket_path);

        let process = cmd.spawn()?;

        // Wait for socket to be available
        let socket_ready = self.wait_for_socket(&socket_path).await;
        if !socket_ready {
            tracing::error!("Sidecar socket not ready for workspace {}", workspace_id);
            return Err(SidecarError::SpawnFailed(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Socket not ready",
            )));
        }

        // Create and connect bridge
        let mut bridge = AgentRuntimeBridge::new(
            socket_path.to_string_lossy().as_ref(),
            self.event_handler.clone(),
        );
        bridge.connect().await?;

        // Store sidecar info
        let info = SidecarInfo {
            workspace_id,
            process,
            socket_path: socket_path.clone(),
            bridge,
        };

        let mut sidecars = self.sidecars.write().await;
        sidecars.insert(workspace_id, info);

        tracing::info!("Sidecar started for workspace {}", workspace_id);
        Ok(())
    }

    async fn find_python(&self) -> Result<PathBuf, SidecarError> {
        // Try python3 first, then python
        for name in &["python3", "python"] {
            if let Ok(output) = tokio::process::Command::new("which")
                .arg(name)
                .output()
                .await
            {
                if output.status.success() {
                    let path = String::from_utf8_lossy(&output.stdout)
                        .trim()
                        .to_string();
                    return Ok(PathBuf::from(path));
                }
            }
        }
        Err(SidecarError::PythonNotFound)
    }

    async fn wait_for_socket(&self, socket_path: &PathBuf) -> bool {
        for _ in 0..50 {
            // 5 seconds max
            if socket_path.exists() {
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        false
    }

    /// Stop a sidecar for a workspace.
    pub async fn stop_sidecar(&self, workspace_id: Uuid) -> Result<(), SidecarError> {
        let mut sidecars = self.sidecars.write().await;
        if let Some(mut info) = sidecars.remove(&workspace_id) {
            tracing::info!("Stopping sidecar for workspace {}", workspace_id);

            // Try graceful termination first
            let _ = info.bridge.terminate(workspace_id).await;

            // Kill process if still running
            let _ = info.process.kill().await;

            // Remove socket file
            let _ = tokio::fs::remove_file(&info.socket_path).await;
        }
        Ok(())
    }

    /// Stop all sidecars.
    pub async fn stop_all(&self) {
        let workspace_ids: Vec<Uuid> = {
            let sidecars = self.sidecars.read().await;
            sidecars.keys().cloned().collect()
        };

        for workspace_id in workspace_ids {
            if let Err(e) = self.stop_sidecar(workspace_id).await {
                tracing::error!("Failed to stop sidecar for workspace {}: {}", workspace_id, e);
            }
        }
    }

    /// Create an agent session in the sidecar.
    pub async fn create_session(
        &self,
        workspace_id: Uuid,
        session_id: Uuid,
        agent_type: crate::types::AgentType,
        config: crate::types::AgentConfig,
    ) -> Result<(), SidecarError> {
        self.start_sidecar(workspace_id).await?;
        let sidecars = self.sidecars.read().await;
        let info = sidecars
            .get(&workspace_id)
            .ok_or_else(|| SidecarError::NotFound(workspace_id))?;
        info.bridge
            .create_session(session_id, agent_type, config)
            .await
            .map(|_| ())
            .map_err(SidecarError::Bridge)
    }

    /// Send a query to an agent session.
    pub async fn query(
        &self,
        workspace_id: Uuid,
        session_id: Uuid,
        prompt: &str,
        context: Option<crate::types::SessionContext>,
    ) -> Result<(), SidecarError> {
        self.start_sidecar(workspace_id).await?;
        let sidecars = self.sidecars.read().await;
        let info = sidecars
            .get(&workspace_id)
            .ok_or_else(|| SidecarError::NotFound(workspace_id))?;
        info.bridge
            .query(session_id, prompt, context)
            .await
            .map_err(SidecarError::Bridge)
    }

    /// Resume an agent session with a user response.
    pub async fn resume_with_response(
        &self,
        workspace_id: Uuid,
        session_id: Uuid,
        input_id: Uuid,
        response: &str,
    ) -> Result<(), SidecarError> {
        self.start_sidecar(workspace_id).await?;
        let sidecars = self.sidecars.read().await;
        let info = sidecars
            .get(&workspace_id)
            .ok_or_else(|| SidecarError::NotFound(workspace_id))?;
        info.bridge
            .resume_with_response(session_id, input_id, response)
            .await
            .map_err(SidecarError::Bridge)
    }

    /// Approve or deny a tool use request.
    pub async fn approve_tool(
        &self,
        workspace_id: Uuid,
        session_id: Uuid,
        input_id: Uuid,
        tool_call_id: &str,
        approved: bool,
        reason: Option<&str>,
    ) -> Result<(), SidecarError> {
        self.start_sidecar(workspace_id).await?;
        let sidecars = self.sidecars.read().await;
        let info = sidecars
            .get(&workspace_id)
            .ok_or_else(|| SidecarError::NotFound(workspace_id))?;
        info.bridge
            .approve_tool(session_id, input_id, tool_call_id, approved, reason)
            .await
            .map_err(SidecarError::Bridge)
    }

    /// Pause an agent session.
    pub async fn pause_session(
        &self,
        workspace_id: Uuid,
        session_id: Uuid,
    ) -> Result<(), SidecarError> {
        self.start_sidecar(workspace_id).await?;
        let sidecars = self.sidecars.read().await;
        let info = sidecars
            .get(&workspace_id)
            .ok_or_else(|| SidecarError::NotFound(workspace_id))?;
        info.bridge
            .pause(session_id)
            .await
            .map_err(SidecarError::Bridge)
    }

    /// Resume a paused agent session.
    pub async fn resume_session(
        &self,
        workspace_id: Uuid,
        session_id: Uuid,
    ) -> Result<(), SidecarError> {
        self.start_sidecar(workspace_id).await?;
        let sidecars = self.sidecars.read().await;
        let info = sidecars
            .get(&workspace_id)
            .ok_or_else(|| SidecarError::NotFound(workspace_id))?;
        info.bridge
            .resume(session_id)
            .await
            .map_err(SidecarError::Bridge)
    }

    /// Terminate an agent session.
    pub async fn terminate_session(
        &self,
        workspace_id: Uuid,
        session_id: Uuid,
    ) -> Result<(), SidecarError> {
        let sidecars = self.sidecars.read().await;
        if let Some(info) = sidecars.get(&workspace_id) {
            let _ = info.bridge.terminate(session_id).await;
        }
        Ok(())
    }
}
