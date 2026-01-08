//! Agent Service Client
//!
//! This module provides a client for the TypeScript Agent Service that uses
//! the Anthropic Agent SDK. It enables interactive agent sessions with
//! pause/resume capabilities for user questions.
//!
//! The agent service is an alternative to spawning Claude Code CLI as a subprocess,
//! providing proper interactive behavior including:
//! - Agent asking questions and waiting for user input
//! - Session persistence across interactions
//! - Streaming events via SSE

use std::sync::Arc;

use futures::TryStreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{error, info};
use utils::{log_msg::LogMsg, msg_store::MsgStore};

/// Default agent service URL (localhost, fixed port 3202)
const DEFAULT_AGENT_SERVICE_URL: &str = "http://127.0.0.1:3202";

/// Errors from the agent service client
#[derive(Debug, Error)]
pub enum AgentServiceError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
    #[error("Job not found: {0}")]
    JobNotFound(String),
    #[error("Job failed: {0}")]
    JobFailed(String),
    #[error("Parse error: {0}")]
    Parse(String),
}

/// Job status from the agent service
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

/// Pending question when job is paused
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingQuestion {
    pub id: String,
    pub question: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<QuestionOption>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    #[serde(rename = "multiSelect")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multi_select: Option<bool>,
}

/// Question option
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Job representation from agent service
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub workspace_id: String,
    pub workspace_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub prompt: String,
    pub agent_type: String,
    pub status: JobStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_question: Option<PendingQuestion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JobError>,
}

/// Job error details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobError {
    pub code: String,
    pub message: String,
}

/// Request to start a new job
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartJobRequest {
    pub workspace_id: String,
    pub workspace_path: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
}

/// Request to resume a paused job
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeJobRequest {
    pub answer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question_id: Option<String>,
}

/// SSE event from the agent service
#[derive(Debug, Clone)]
pub enum AgentServiceEvent {
    Status { status: JobStatus },
    Output { content: String, is_partial: bool },
    ToolUse { tool_name: String, input: String },
    ToolResult { content: String },
    Thinking { content: String },
    Question { question: PendingQuestion },
    SessionId { session_id: String },
    Error { code: String, message: String },
    Complete { duration_ms: u64 },
}

/// Agent Service Client
///
/// Provides methods to interact with the TypeScript Agent Service.
#[derive(Clone)]
pub struct AgentServiceClient {
    client: Client,
    base_url: String,
}

impl Default for AgentServiceClient {
    fn default() -> Self {
        Self::new(None)
    }
}

impl AgentServiceClient {
    /// Create a new agent service client
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.unwrap_or_else(|| DEFAULT_AGENT_SERVICE_URL.to_string()),
        }
    }

    /// Check if the agent service is available
    pub async fn health_check(&self) -> Result<bool, AgentServiceError> {
        let url = format!("{}/health", self.base_url);
        match self.client.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(e) => {
                if e.is_connect() {
                    Ok(false)
                } else {
                    Err(e.into())
                }
            }
        }
    }

    /// Start a new job
    pub async fn start_job(&self, request: StartJobRequest) -> Result<Job, AgentServiceError> {
        let url = format!("{}/jobs", self.base_url);
        let resp = self.client.post(&url).json(&request).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AgentServiceError::ServiceUnavailable(format!(
                "Status {}: {}",
                status, body
            )));
        }

        resp.json().await.map_err(|e| e.into())
    }

    /// Get job status
    pub async fn get_job(&self, job_id: &str) -> Result<Job, AgentServiceError> {
        let url = format!("{}/jobs/{}", self.base_url, job_id);
        let resp = self.client.get(&url).send().await?;

        if resp.status().as_u16() == 404 {
            return Err(AgentServiceError::JobNotFound(job_id.to_string()));
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AgentServiceError::ServiceUnavailable(format!(
                "Status {}: {}",
                status, body
            )));
        }

        resp.json().await.map_err(|e| e.into())
    }

    /// Resume a paused job with user answer
    pub async fn resume_job(
        &self,
        job_id: &str,
        request: ResumeJobRequest,
    ) -> Result<Job, AgentServiceError> {
        let url = format!("{}/jobs/{}/resume", self.base_url, job_id);
        let resp = self.client.post(&url).json(&request).send().await?;

        if resp.status().as_u16() == 404 {
            return Err(AgentServiceError::JobNotFound(job_id.to_string()));
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AgentServiceError::ServiceUnavailable(format!(
                "Status {}: {}",
                status, body
            )));
        }

        resp.json().await.map_err(|e| e.into())
    }

    /// Cancel a running job
    pub async fn cancel_job(&self, job_id: &str) -> Result<(), AgentServiceError> {
        let url = format!("{}/jobs/{}/cancel", self.base_url, job_id);
        let resp = self.client.post(&url).send().await?;

        if resp.status().as_u16() == 404 {
            return Err(AgentServiceError::JobNotFound(job_id.to_string()));
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AgentServiceError::ServiceUnavailable(format!(
                "Status {}: {}",
                status, body
            )));
        }

        Ok(())
    }

    /// Subscribe to job events via SSE
    ///
    /// Returns a channel receiver that yields events as they arrive.
    /// The channel is closed when the job completes or an error occurs.
    pub async fn subscribe_to_job(
        &self,
        job_id: &str,
    ) -> Result<mpsc::Receiver<AgentServiceEvent>, AgentServiceError> {
        let url = format!("{}/jobs/{}/stream", self.base_url, job_id);

        let resp = self
            .client
            .get(&url)
            .header("Accept", "text/event-stream")
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AgentServiceError::ServiceUnavailable(format!(
                "Status {}: {}",
                status, body
            )));
        }

        let (tx, rx) = mpsc::channel(100);

        // Spawn task to read SSE events
        let byte_stream = resp.bytes_stream();
        tokio::spawn(async move {
            let mut buffer = String::new();
            let mut event_type = String::new();

            let mut stream = byte_stream.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));

            while let Ok(Some(chunk)) = stream.try_next().await {
                let text = String::from_utf8_lossy(&chunk);
                buffer.push_str(&text);

                // Process complete SSE messages
                while let Some(pos) = buffer.find("\n\n") {
                    let message = buffer[..pos].to_string();
                    buffer = buffer[pos + 2..].to_string();

                    for line in message.lines() {
                        if let Some(event) = line.strip_prefix("event: ") {
                            event_type = event.to_string();
                        } else if let Some(data) = line.strip_prefix("data: ") {
                            if let Some(event) = parse_sse_event(&event_type, data) {
                                let is_complete = matches!(event, AgentServiceEvent::Complete { .. });
                                if tx.send(event).await.is_err() {
                                    return; // Receiver dropped
                                }
                                if is_complete {
                                    return; // Job finished
                                }
                            }
                        }
                    }
                }
            }
        });

        Ok(rx)
    }

    /// Execute a job and stream events to MsgStore
    ///
    /// This is the main integration point with the existing container infrastructure.
    /// It starts a job, subscribes to events, and forwards them to the MsgStore
    /// in the format expected by the frontend.
    pub async fn execute_and_stream(
        &self,
        request: StartJobRequest,
        msg_store: Arc<MsgStore>,
    ) -> Result<Job, AgentServiceError> {
        // Start the job
        let job = self.start_job(request).await?;
        info!(job_id = %job.id, "Started agent service job");

        // Subscribe to events
        let mut events = self.subscribe_to_job(&job.id).await?;

        // Forward events to MsgStore
        while let Some(event) = events.recv().await {
            let log_msg = event_to_log_msg(&event);
            msg_store.push(log_msg);

            // If job completed or failed, we're done
            match event {
                AgentServiceEvent::Complete { .. } => {
                    msg_store.push_finished();
                    break;
                }
                AgentServiceEvent::Error { code, message } => {
                    error!(code = %code, message = %message, "Agent service job error");
                    msg_store.push_finished();
                    break;
                }
                _ => {}
            }
        }

        // Return final job state
        self.get_job(&job.id).await
    }
}

/// Parse an SSE event from event type and data
fn parse_sse_event(event_type: &str, data: &str) -> Option<AgentServiceEvent> {
    let json: serde_json::Value = serde_json::from_str(data).ok()?;

    match event_type {
        "status" => {
            let status_str = json.get("status")?.as_str()?;
            let status = match status_str {
                "queued" => JobStatus::Queued,
                "running" => JobStatus::Running,
                "paused" => JobStatus::Paused,
                "completed" => JobStatus::Completed,
                "failed" => JobStatus::Failed,
                "cancelled" => JobStatus::Cancelled,
                _ => return None,
            };
            Some(AgentServiceEvent::Status { status })
        }
        "output" => {
            let output_type = json.get("type")?.as_str()?;
            let content = json.get("content")?.as_str()?.to_string();
            let is_partial = json.get("isPartial").and_then(|v| v.as_bool()).unwrap_or(false);

            match output_type {
                "text" => Some(AgentServiceEvent::Output { content, is_partial }),
                "thinking" => Some(AgentServiceEvent::Thinking { content }),
                "tool_use" => {
                    let tool_name = json.get("toolName")?.as_str()?.to_string();
                    Some(AgentServiceEvent::ToolUse {
                        tool_name,
                        input: content,
                    })
                }
                "tool_result" => Some(AgentServiceEvent::ToolResult { content }),
                _ => None,
            }
        }
        "question" => {
            let question = PendingQuestion {
                id: json.get("questionId")?.as_str()?.to_string(),
                question: json.get("question")?.as_str()?.to_string(),
                options: json.get("options").and_then(|v| {
                    serde_json::from_value(v.clone()).ok()
                }),
                header: json.get("header").and_then(|v| v.as_str()).map(String::from),
                multi_select: json.get("multiSelect").and_then(|v| v.as_bool()),
            };
            Some(AgentServiceEvent::Question { question })
        }
        "error" => {
            let code = json.get("code")?.as_str()?.to_string();
            let message = json.get("message")?.as_str()?.to_string();
            Some(AgentServiceEvent::Error { code, message })
        }
        "session_id" => {
            let session_id = json.get("sessionId")?.as_str()?.to_string();
            Some(AgentServiceEvent::SessionId { session_id })
        }
        "complete" => {
            let duration = json.get("duration").and_then(|v| v.as_u64()).unwrap_or(0);
            Some(AgentServiceEvent::Complete { duration_ms: duration })
        }
        _ => None,
    }
}

/// Convert an agent service event to a LogMsg for the MsgStore
fn event_to_log_msg(event: &AgentServiceEvent) -> LogMsg {
    match event {
        AgentServiceEvent::Status { status } => {
            LogMsg::Stdout(format!(
                r#"{{"type":"status","status":"{}"}}"#,
                match status {
                    JobStatus::Queued => "queued",
                    JobStatus::Running => "running",
                    JobStatus::Paused => "paused",
                    JobStatus::Completed => "completed",
                    JobStatus::Failed => "failed",
                    JobStatus::Cancelled => "cancelled",
                }
            ))
        }
        AgentServiceEvent::Output { content, is_partial } => {
            // Format as Claude Code-style JSON output
            LogMsg::Stdout(format!(
                r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":{}}}]}},"isPartial":{}}}"#,
                serde_json::to_string(content).unwrap_or_default(),
                is_partial
            ))
        }
        AgentServiceEvent::Thinking { content } => {
            LogMsg::Stdout(format!(
                r#"{{"type":"assistant","message":{{"content":[{{"type":"thinking","thinking":{}}}]}}}}"#,
                serde_json::to_string(content).unwrap_or_default()
            ))
        }
        AgentServiceEvent::ToolUse { tool_name, input } => {
            LogMsg::Stdout(format!(
                r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","name":"{}","input":{}}}]}}}}"#,
                tool_name,
                input
            ))
        }
        AgentServiceEvent::ToolResult { content } => {
            LogMsg::Stdout(format!(
                r#"{{"type":"result","content":{}}}"#,
                serde_json::to_string(content).unwrap_or_default()
            ))
        }
        AgentServiceEvent::Question { question } => {
            // Format as a special question event for the frontend
            LogMsg::Stdout(format!(
                r#"{{"type":"question","questionId":"{}","question":"{}","options":{}}}"#,
                question.id,
                question.question,
                serde_json::to_string(&question.options).unwrap_or_else(|_| "null".to_string())
            ))
        }
        AgentServiceEvent::SessionId { session_id } => {
            // Use LogMsg::SessionId to integrate with existing session persistence flow
            // This gets captured by spawn_stream_raw_logs_to_db in container.rs
            LogMsg::SessionId(session_id.clone())
        }
        AgentServiceEvent::Error { code, message } => {
            LogMsg::Stderr(format!(
                r#"{{"type":"error","code":"{}","message":{}}}"#,
                code,
                serde_json::to_string(message).unwrap_or_default()
            ))
        }
        AgentServiceEvent::Complete { duration_ms } => {
            LogMsg::Stdout(format!(
                r#"{{"type":"complete","duration":{}}}"#,
                duration_ms
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_status_event() {
        let event = parse_sse_event("status", r#"{"status":"running","timestamp":"2024-01-01T00:00:00Z"}"#);
        assert!(matches!(event, Some(AgentServiceEvent::Status { status: JobStatus::Running })));
    }

    #[test]
    fn test_parse_output_event() {
        let event = parse_sse_event("output", r#"{"type":"text","content":"Hello","isPartial":false}"#);
        assert!(matches!(event, Some(AgentServiceEvent::Output { content, is_partial: false }) if content == "Hello"));
    }

    #[test]
    fn test_parse_question_event() {
        let event = parse_sse_event(
            "question",
            r#"{"questionId":"q1","question":"Which option?","options":[{"label":"A"},{"label":"B"}]}"#,
        );
        assert!(matches!(event, Some(AgentServiceEvent::Question { question }) if question.id == "q1"));
    }

    #[test]
    fn test_parse_session_id_event() {
        let event = parse_sse_event(
            "session_id",
            r#"{"sessionId":"abc123-session","timestamp":"2024-01-01T00:00:00Z"}"#,
        );
        assert!(matches!(event, Some(AgentServiceEvent::SessionId { session_id }) if session_id == "abc123-session"));
    }
}
