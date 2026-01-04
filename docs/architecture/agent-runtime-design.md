# Agent Runtime Architecture Design

## Executive Summary

This document proposes transforming claude-jobs from a CLI-process-spawning model to an interactive agent runtime model using the Claude Agent SDK. The core change is replacing the `Command::new()` / `AsyncGroupChild` spawning pattern with an in-process SDK client that maintains true interactive sessions.

## Problem Analysis

### Current Architecture Limitations

The existing Vibe Kanban architecture (and Auto-Claude) share a fundamental limitation:

```
Task → Workspace → Session → ExecutionProcess → spawn(CLI) → stream stdout
```

**Issues with CLI-spawning model:**

1. **Process-Centric Lifecycle**: Agents exist only while the process runs. No natural pause/resume.
2. **Fragile Interactive Flows**: `AskUserQuestion` and clarification loops fail or behave inconsistently because the process expects stdin input that isn't designed for async human interaction.
3. **Session as Process**: A "session" is really just a process ID, not a conversational context.
4. **Control Protocol Workarounds**: The current `ProtocolPeer` (crates/executors/src/executors/claude/protocol.rs) implements a stdin/stdout control protocol that's a band-aid over the fundamental issue.
5. **Scaling Constraints**: Each agent is a spawned process with overhead; no shared context or state between executions.

### Current Implementation Analysis

**Partial SDK Integration (current state):**
- `ClaudeAgentClient` (client.rs:25-186) handles approvals via control protocol
- `ProtocolPeer` (protocol.rs:20-213) manages stdin/stdout JSON-RPC communication
- Still spawns `npx @anthropic-ai/claude-code@2.0.75` as subprocess (claude.rs:42-48)

**What Works:**
- Task/Workspace/Session/ExecutionProcess model is sound
- WebSocket streaming infrastructure is solid
- Worktree isolation for parallel execution is effective
- Log normalization and patching system is mature

**What Must Change:**
- Executor spawning must use Agent SDK instead of CLI subprocess
- Session state must be persisted, not tied to process lifecycle
- Interactive flows (pause/question/resume) must be first-class citizens

---

## Proposed Architecture

### Core Design Principles

1. **SDK-First Execution**: Claude agents run via `claude-agent-sdk` Python or TypeScript SDK, not CLI spawning
2. **Long-Lived Sessions**: Agent sessions persist independently of process lifecycle
3. **Interactive by Default**: Pause, question, and resume are core capabilities
4. **Event-Driven State Machine**: Agent state transitions are explicit and observable
5. **Extensible Agent Types**: Architecture supports multiple agent types (coding, orchestration, deployment)

### New Component Model

```
┌─────────────────────────────────────────────────────────────────────┐
│                           Frontend (React)                          │
├─────────────────────────────────────────────────────────────────────┤
│  WebSocket/SSE  │  REST API  │  Agent State Events  │  Log Streams  │
└────────┬────────┴─────┬──────┴──────────┬───────────┴───────┬───────┘
         │              │                 │                   │
┌────────▼──────────────▼─────────────────▼───────────────────▼───────┐
│                        Rust Backend (Axum)                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌─────────────────┐  ┌──────────────────┐  ┌──────────────────┐   │
│  │  Agent Manager  │  │  Session Store   │  │  Event Dispatcher│   │
│  │                 │  │                  │  │                  │   │
│  │ - lifecycle     │  │ - context        │  │ - state changes  │   │
│  │ - state machine │  │ - messages       │  │ - streams        │   │
│  │ - queue mgmt    │  │ - persistence    │  │ - notifications  │   │
│  └────────┬────────┘  └────────┬─────────┘  └────────┬─────────┘   │
│           │                    │                     │              │
│  ┌────────▼────────────────────▼─────────────────────▼─────────┐   │
│  │                     Agent Runtime Bridge                     │   │
│  │                                                              │   │
│  │  - Communicates with Agent SDK process via gRPC/JSON-RPC    │   │
│  │  - Translates between Rust types and SDK types              │   │
│  │  - Handles tool registration and execution                  │   │
│  │  - Manages streaming responses                               │   │
│  └──────────────────────────────┬───────────────────────────────┘   │
│                                 │                                   │
└─────────────────────────────────┼───────────────────────────────────┘
                                  │
                                  │ gRPC / Unix Socket / JSON-RPC
                                  │
┌─────────────────────────────────▼───────────────────────────────────┐
│                    Agent SDK Sidecar Process                        │
│                                                                     │
│  ┌───────────────────────────────────────────────────────────────┐ │
│  │              Claude Agent SDK (Python/TypeScript)             │ │
│  │                                                               │ │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐   │ │
│  │  │ Coding      │  │ Orchestrator│  │ Deployment          │   │ │
│  │  │ Agent       │  │ Agent       │  │ Agent               │   │ │
│  │  │             │  │             │  │                     │   │ │
│  │  │ - File ops  │  │ - Planning  │  │ - Infra provisioning│   │ │
│  │  │ - Bash      │  │ - Task split│  │ - Deploy scripts    │   │ │
│  │  │ - Search    │  │ - Delegate  │  │ - Health checks     │   │ │
│  │  └─────────────┘  └─────────────┘  └─────────────────────┘   │ │
│  │                                                               │ │
│  │  ┌─────────────────────────────────────────────────────────┐ │ │
│  │  │                   Session Manager                       │ │ │
│  │  │  - Maintains agent instances per session ID             │ │ │
│  │  │  - Context window management                            │ │ │
│  │  │  - Tool/hook registration                               │ │ │
│  │  │  - Pause/Resume state                                   │ │ │
│  │  └─────────────────────────────────────────────────────────┘ │ │
│  └───────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

### Agent State Machine

```
                         ┌─────────────┐
                         │   Created   │
                         └──────┬──────┘
                                │ initialize()
                                ▼
                         ┌─────────────┐
              ┌──────────│    Idle     │◄─────────────┐
              │          └──────┬──────┘              │
              │                 │ query()             │
              │                 ▼                     │
              │          ┌─────────────┐              │
              │    ┌────►│  Executing  │──────────────┤
              │    │     └──────┬──────┘              │
              │    │            │                     │ resume()
              │    │            ├─── tool_result ────►│
              │    │            │                     │
              │    │            ▼                     │
              │    │     ┌─────────────┐              │
              │    │     │  Awaiting   │──────────────┘
              │    │     │   Input     │
              │    │     └──────┬──────┘
              │    │            │
              │    │            │ interrupt / user_response
              │    │            ▼
              │    │     ┌─────────────┐
              │    └─────│  Paused     │
              │          └──────┬──────┘
              │                 │ terminate()
              ▼                 ▼
       ┌─────────────┐   ┌─────────────┐
       │  Completed  │   │  Terminated │
       └─────────────┘   └─────────────┘
```

**State Definitions:**

| State | Description |
|-------|-------------|
| Created | Session initialized, not yet started |
| Idle | Ready to receive prompts, no active execution |
| Executing | Agent is actively processing (tool calls, thinking) |
| AwaitingInput | Agent has asked a question or requires approval |
| Paused | Execution explicitly paused by user or system |
| Completed | Task finished successfully |
| Terminated | Stopped due to error, timeout, or cancellation |

---

## Database Schema Changes

### New Tables

```sql
-- Agent session state (replaces process-centric ExecutionProcess for agents)
CREATE TABLE agent_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID NOT NULL REFERENCES sessions(id),
    agent_type VARCHAR(50) NOT NULL, -- 'coding', 'orchestration', 'deployment'
    state VARCHAR(50) NOT NULL DEFAULT 'created',
    context JSONB, -- Serialized agent context for pause/resume
    current_tool_call_id VARCHAR(255), -- For pending approvals
    pending_question TEXT, -- Question awaiting user response
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,

    CONSTRAINT valid_state CHECK (state IN (
        'created', 'idle', 'executing', 'awaiting_input', 'paused', 'completed', 'terminated'
    ))
);

-- Agent messages (conversation history for context)
CREATE TABLE agent_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_session_id UUID NOT NULL REFERENCES agent_sessions(id),
    role VARCHAR(20) NOT NULL, -- 'user', 'assistant', 'tool_result'
    content TEXT NOT NULL,
    metadata JSONB, -- Tool calls, thinking, etc.
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Pending user inputs (for async question/answer flow)
CREATE TABLE agent_pending_inputs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_session_id UUID NOT NULL REFERENCES agent_sessions(id),
    input_type VARCHAR(50) NOT NULL, -- 'question', 'approval', 'clarification'
    prompt TEXT NOT NULL,
    tool_call_id VARCHAR(255),
    tool_name VARCHAR(255),
    tool_input JSONB,
    response TEXT,
    responded_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Index for efficient queries
CREATE INDEX idx_agent_sessions_session_id ON agent_sessions(session_id);
CREATE INDEX idx_agent_sessions_state ON agent_sessions(state);
CREATE INDEX idx_agent_messages_session ON agent_messages(agent_session_id);
CREATE INDEX idx_agent_pending_inputs_session ON agent_pending_inputs(agent_session_id);
CREATE INDEX idx_agent_pending_inputs_pending ON agent_pending_inputs(agent_session_id)
    WHERE responded_at IS NULL;
```

### Modified Tables

```sql
-- Add agent runtime configuration to projects
ALTER TABLE projects ADD COLUMN agent_config JSONB DEFAULT '{
    "default_agent_type": "coding",
    "allow_parallel_agents": true,
    "max_context_tokens": 200000,
    "auto_approve_read_tools": true
}';

-- Add SDK session tracking to sessions
ALTER TABLE sessions ADD COLUMN sdk_session_id VARCHAR(255);
ALTER TABLE sessions ADD COLUMN agent_session_id UUID REFERENCES agent_sessions(id);
```

---

## Agent Runtime Bridge Implementation

### Rust Side (crates/agent-runtime/)

```rust
// crates/agent-runtime/src/lib.rs
pub mod bridge;
pub mod state;
pub mod messages;
pub mod tools;

// crates/agent-runtime/src/bridge.rs
use tokio::sync::mpsc;

/// Bridge between Rust backend and Agent SDK sidecar
pub struct AgentRuntimeBridge {
    /// Channel to send commands to SDK process
    command_tx: mpsc::Sender<AgentCommand>,
    /// Channel to receive events from SDK process
    event_rx: mpsc::Receiver<AgentEvent>,
    /// Active agent sessions
    sessions: HashMap<Uuid, AgentSessionHandle>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentCommand {
    CreateSession {
        session_id: Uuid,
        agent_type: AgentType,
        config: AgentConfig,
    },
    Query {
        session_id: Uuid,
        prompt: String,
        context: Option<SessionContext>,
    },
    Resume {
        session_id: Uuid,
        user_response: String,
    },
    ApproveToolUse {
        session_id: Uuid,
        tool_call_id: String,
        approved: bool,
        reason: Option<String>,
    },
    Interrupt {
        session_id: Uuid,
    },
    Terminate {
        session_id: Uuid,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentEvent {
    StateChanged {
        session_id: Uuid,
        from: AgentState,
        to: AgentState,
    },
    MessageStream {
        session_id: Uuid,
        content: StreamContent,
    },
    ToolUse {
        session_id: Uuid,
        tool_call_id: String,
        tool_name: String,
        tool_input: serde_json::Value,
    },
    ToolResult {
        session_id: Uuid,
        tool_call_id: String,
        result: serde_json::Value,
        is_error: bool,
    },
    QuestionAsked {
        session_id: Uuid,
        question: String,
        input_id: Uuid,
    },
    ApprovalNeeded {
        session_id: Uuid,
        tool_call_id: String,
        tool_name: String,
        tool_input: serde_json::Value,
    },
    Completed {
        session_id: Uuid,
        result: Option<String>,
    },
    Error {
        session_id: Uuid,
        error: String,
    },
}

impl AgentRuntimeBridge {
    pub async fn new(sidecar_socket: &Path) -> Result<Self, Error> {
        // Connect to SDK sidecar process
        // ...
    }

    pub async fn create_session(
        &self,
        session_id: Uuid,
        agent_type: AgentType,
        config: AgentConfig,
    ) -> Result<AgentSessionHandle, Error> {
        // Send CreateSession command, wait for confirmation
    }

    pub async fn query(
        &self,
        session_id: Uuid,
        prompt: &str,
    ) -> Result<mpsc::Receiver<AgentEvent>, Error> {
        // Send Query command, return event stream
    }

    pub async fn resume_with_response(
        &self,
        session_id: Uuid,
        response: &str,
    ) -> Result<(), Error> {
        // Resume paused session with user's answer
    }
}
```

### SDK Sidecar (agent-sidecar/)

```python
# agent-sidecar/main.py
import asyncio
from typing import Dict, Optional
from uuid import UUID
from claude_agent_sdk import ClaudeSDKClient, ClaudeAgentOptions
from dataclasses import dataclass
from enum import Enum

class AgentState(Enum):
    CREATED = "created"
    IDLE = "idle"
    EXECUTING = "executing"
    AWAITING_INPUT = "awaiting_input"
    PAUSED = "paused"
    COMPLETED = "completed"
    TERMINATED = "terminated"

@dataclass
class AgentSession:
    session_id: UUID
    client: ClaudeSDKClient
    state: AgentState
    pending_response: Optional[asyncio.Event] = None
    user_response: Optional[str] = None

class AgentSessionManager:
    def __init__(self):
        self.sessions: Dict[UUID, AgentSession] = {}

    async def create_session(
        self,
        session_id: UUID,
        agent_type: str,
        config: dict,
    ) -> AgentSession:
        """Create a new agent session with the SDK client."""
        options = ClaudeAgentOptions(
            system_prompt=self._get_system_prompt(agent_type, config),
            allowed_tools=self._get_allowed_tools(agent_type, config),
            cwd=config.get("working_dir"),
            setting_sources=["project"],
        )

        # Create SDK client with custom tools and hooks
        async with ClaudeSDKClient(options) as client:
            session = AgentSession(
                session_id=session_id,
                client=client,
                state=AgentState.IDLE,
            )
            self.sessions[session_id] = session
            return session

    async def query(
        self,
        session_id: UUID,
        prompt: str,
        event_callback,
    ):
        """Send a query to an agent and stream events."""
        session = self.sessions[session_id]
        session.state = AgentState.EXECUTING

        await event_callback({
            "type": "state_changed",
            "session_id": str(session_id),
            "from": "idle",
            "to": "executing",
        })

        try:
            async for msg in session.client.query(prompt):
                # Handle different message types
                if msg.type == "text":
                    await event_callback({
                        "type": "message_stream",
                        "session_id": str(session_id),
                        "content": {"type": "text", "text": msg.text},
                    })
                elif msg.type == "tool_use":
                    await event_callback({
                        "type": "tool_use",
                        "session_id": str(session_id),
                        "tool_call_id": msg.id,
                        "tool_name": msg.name,
                        "tool_input": msg.input,
                    })
                elif msg.type == "tool_result":
                    await event_callback({
                        "type": "tool_result",
                        "session_id": str(session_id),
                        "tool_call_id": msg.tool_use_id,
                        "result": msg.content,
                        "is_error": msg.is_error,
                    })

            session.state = AgentState.IDLE
            await event_callback({
                "type": "completed",
                "session_id": str(session_id),
            })

        except Exception as e:
            session.state = AgentState.TERMINATED
            await event_callback({
                "type": "error",
                "session_id": str(session_id),
                "error": str(e),
            })

    async def wait_for_user_input(
        self,
        session_id: UUID,
        question: str,
        input_id: UUID,
        event_callback,
    ) -> str:
        """Pause execution and wait for user input."""
        session = self.sessions[session_id]
        session.state = AgentState.AWAITING_INPUT
        session.pending_response = asyncio.Event()

        await event_callback({
            "type": "question_asked",
            "session_id": str(session_id),
            "question": question,
            "input_id": str(input_id),
        })

        # Wait for user response (set by resume_with_response)
        await session.pending_response.wait()

        response = session.user_response
        session.user_response = None
        session.pending_response = None
        session.state = AgentState.EXECUTING

        return response

    async def resume_with_response(
        self,
        session_id: UUID,
        response: str,
    ):
        """Resume a paused session with user's response."""
        session = self.sessions[session_id]
        if session.state != AgentState.AWAITING_INPUT:
            raise ValueError(f"Session not awaiting input: {session.state}")

        session.user_response = response
        session.pending_response.set()


# Custom tool for asking user questions
@tool("AskUser", "Ask the user a question and wait for their response", {
    "question": str,
})
async def ask_user(args, context) -> dict:
    """Custom tool that pauses execution and waits for user input."""
    question = args["question"]
    input_id = uuid4()

    # This will pause the agent and emit an event
    response = await context.session_manager.wait_for_user_input(
        context.session_id,
        question,
        input_id,
        context.event_callback,
    )

    return {"content": [{"type": "text", "text": response}]}
```

---

## API Endpoints

### New Agent Runtime Endpoints

```rust
// crates/server/src/routes/agents.rs

/// Create a new agent session for a workspace
#[utoipa::path(
    post,
    path = "/api/workspaces/{workspace_id}/agents",
    request_body = CreateAgentRequest,
    responses(
        (status = 201, body = AgentSession),
        (status = 400, body = ApiError),
    )
)]
pub async fn create_agent_session(
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
    Json(request): Json<CreateAgentRequest>,
) -> Result<Json<AgentSession>, ApiError>;

/// Send a query to an agent session
#[utoipa::path(
    post,
    path = "/api/agents/{agent_session_id}/query",
    request_body = AgentQueryRequest,
    responses(
        (status = 200, body = AgentQueryResponse),
        (status = 400, body = ApiError),
    )
)]
pub async fn query_agent(
    State(state): State<AppState>,
    Path(agent_session_id): Path<Uuid>,
    Json(request): Json<AgentQueryRequest>,
) -> Result<Json<AgentQueryResponse>, ApiError>;

/// Respond to a pending question
#[utoipa::path(
    post,
    path = "/api/agents/{agent_session_id}/respond",
    request_body = AgentRespondRequest,
    responses(
        (status = 200),
        (status = 400, body = ApiError),
    )
)]
pub async fn respond_to_question(
    State(state): State<AppState>,
    Path(agent_session_id): Path<Uuid>,
    Json(request): Json<AgentRespondRequest>,
) -> Result<(), ApiError>;

/// Approve or deny a tool use request
#[utoipa::path(
    post,
    path = "/api/agents/{agent_session_id}/approve",
    request_body = ApprovalRequest,
    responses(
        (status = 200),
        (status = 400, body = ApiError),
    )
)]
pub async fn approve_tool_use(
    State(state): State<AppState>,
    Path(agent_session_id): Path<Uuid>,
    Json(request): Json<ApprovalRequest>,
) -> Result<(), ApiError>;

/// Stream agent events via WebSocket
#[utoipa::path(
    get,
    path = "/api/agents/{agent_session_id}/events",
    responses(
        (status = 101, description = "WebSocket upgrade"),
    )
)]
pub async fn stream_agent_events(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(agent_session_id): Path<Uuid>,
) -> impl IntoResponse;

/// Get current agent state
#[utoipa::path(
    get,
    path = "/api/agents/{agent_session_id}",
    responses(
        (status = 200, body = AgentSession),
        (status = 404, body = ApiError),
    )
)]
pub async fn get_agent_session(
    State(state): State<AppState>,
    Path(agent_session_id): Path<Uuid>,
) -> Result<Json<AgentSession>, ApiError>;

/// Pause an executing agent
#[utoipa::path(
    post,
    path = "/api/agents/{agent_session_id}/pause",
    responses(
        (status = 200),
        (status = 400, body = ApiError),
    )
)]
pub async fn pause_agent(
    State(state): State<AppState>,
    Path(agent_session_id): Path<Uuid>,
) -> Result<(), ApiError>;

/// Resume a paused agent
#[utoipa::path(
    post,
    path = "/api/agents/{agent_session_id}/resume",
    responses(
        (status = 200),
        (status = 400, body = ApiError),
    )
)]
pub async fn resume_agent(
    State(state): State<AppState>,
    Path(agent_session_id): Path<Uuid>,
) -> Result<(), ApiError>;

/// Terminate an agent session
#[utoipa::path(
    delete,
    path = "/api/agents/{agent_session_id}",
    responses(
        (status = 200),
        (status = 400, body = ApiError),
    )
)]
pub async fn terminate_agent(
    State(state): State<AppState>,
    Path(agent_session_id): Path<Uuid>,
) -> Result<(), ApiError>;
```

---

## Implementation Phases

### Phase 1: Foundation (Milestone: Agent SDK Integration)

**Goal**: Replace CLI spawning with Agent SDK for coding agents

1. **Create agent-sidecar Python service**
   - Implement `AgentSessionManager`
   - gRPC/Unix socket server for Rust communication
   - Basic session lifecycle (create, query, terminate)

2. **Create crates/agent-runtime**
   - `AgentRuntimeBridge` for Rust ↔ Python communication
   - State machine implementation
   - Event streaming infrastructure

3. **Database migrations**
   - Add `agent_sessions` table
   - Add `agent_messages` table
   - Add `agent_pending_inputs` table

4. **Modify executor flow**
   - Add new `AgentExecutor` that uses SDK instead of CLI
   - Keep existing `ClaudeCode` executor for backward compatibility
   - Feature flag to switch between modes

### Phase 2: Interactive Capabilities (Milestone: Pause/Resume)

**Goal**: Full interactive agent support

1. **Implement question/answer flow**
   - `AskUser` custom tool in sidecar
   - Frontend UI for pending questions
   - Response API endpoints

2. **Implement pause/resume**
   - State persistence to database
   - Context serialization
   - Resume from saved state

3. **Approval flow modernization**
   - Migrate from current `ApprovalService` to new model
   - UI updates for approval requests

### Phase 3: Multi-Agent Support (Milestone: Orchestration)

**Goal**: Support orchestration and specialized agents

1. **Orchestration agent type**
   - Task decomposition prompts
   - Sub-agent spawning
   - Result aggregation

2. **Deployment agent type**
   - Infrastructure tools
   - Deploy script execution
   - Health check integration

3. **Agent coordination**
   - Parent/child agent relationships
   - Shared context passing
   - Parallel execution management

### Phase 4: Advanced Features (Milestone: Production Ready)

**Goal**: Production hardening and optimization

1. **Context management**
   - Token counting and optimization
   - Context window sliding
   - Conversation summarization

2. **Error handling & recovery**
   - Automatic retry with backoff
   - Graceful degradation
   - State recovery after crashes

3. **Observability**
   - Metrics (latency, token usage, success rates)
   - Distributed tracing
   - Cost tracking

---

## Trade-offs and Decisions

### Language Choice: Python Sidecar vs TypeScript

**Decision**: Python sidecar process

**Rationale**:
- Official `claude-agent-sdk` Python package has richer features
- In-process MCP servers only available in Python SDK
- Better ecosystem for custom tools
- Sidecar model keeps Rust backend focused on orchestration

**Trade-off**: Cross-process communication overhead, but gRPC mitigates this

### Communication Protocol: gRPC vs JSON-RPC

**Decision**: Start with JSON-RPC over Unix socket, migrate to gRPC if needed

**Rationale**:
- JSON-RPC is simpler to implement and debug
- Unix socket provides low latency without network overhead
- gRPC can be added later for additional features (streaming, backpressure)

### State Persistence: Database vs Redis

**Decision**: PostgreSQL with JSONB

**Rationale**:
- Already using SQLite/Postgres in the system
- ACID guarantees for session state
- JSONB provides flexibility for context storage
- Avoids additional infrastructure dependency

**Trade-off**: Higher latency than Redis, but acceptable for session state

### Backward Compatibility

**Decision**: Dual executor mode with feature flag

**Rationale**:
- Existing CLI-based executor continues to work
- Gradual migration path
- Can fall back if issues arise
- Non-breaking for existing users

---

## Success Criteria

### Functional Requirements

- [ ] Claude agents run as SDK clients, not CLI processes
- [ ] Agent sessions persist across process restarts
- [ ] Agents can ask questions and receive user responses
- [ ] Agents can pause and resume without losing context
- [ ] Multiple agent types (coding, orchestration, deployment) supported
- [ ] Tool approvals work through new approval flow

### Non-Functional Requirements

- [ ] Agent response latency < 500ms (first token)
- [ ] Session state persistence < 100ms
- [ ] Support 100+ concurrent agent sessions
- [ ] Graceful degradation under load
- [ ] Observable (metrics, logs, traces)

### Verification Plan

1. **Unit tests**: Agent state machine, message handling
2. **Integration tests**: Full agent session lifecycle
3. **E2E tests**: Frontend to agent round-trip
4. **Load tests**: Concurrent session handling
5. **Chaos tests**: Sidecar restart, database failures

---

## Appendix: Migration from Current Implementation

### Files to Modify

1. **crates/executors/src/executors/claude.rs** - Add SDK executor variant
2. **crates/executors/src/actions/** - New `AgentAction` type
3. **crates/services/src/services/container.rs** - Agent runtime integration
4. **crates/server/src/routes/** - New agent API routes
5. **crates/db/src/models/** - New agent models
6. **frontend/src/** - Agent UI components

### Files to Add

1. **crates/agent-runtime/** - New crate for agent bridge
2. **agent-sidecar/** - Python Agent SDK service
3. **crates/db/migrations/XXXX_agent_sessions.sql** - Schema changes

### Files to Deprecate (Phase 2+)

1. `crates/executors/src/executors/claude/protocol.rs` - Replaced by SDK
2. `crates/executors/src/executors/claude/client.rs` - Replaced by bridge

---

## Open Questions for Discussion

1. **Sidecar lifecycle**: Should the sidecar run as a single long-lived process or spawn per-workspace?
2. **Context limits**: How should we handle context window exhaustion for long sessions?
3. **Cost tracking**: Should we track API costs per session/task?
4. **Multi-tenancy**: Do we need per-user agent isolation?
5. **Caching**: Should we cache agent responses for similar queries?

---

## Migration Guide

### Current Status (Implemented)

The following components have been implemented:

- **Database schema**: `agent_sessions`, `agent_messages`, `agent_pending_inputs` tables
- **Python sidecar**: `agent-sidecar/` with session management and JSON-RPC server
- **Rust agent-runtime crate**: `crates/agent-runtime/` with bridge and sidecar manager
- **API routes**: `/api/agents/` endpoints for session management
- **Frontend components**: `AgentSessionPanel`, `AgentMessageList`, etc.
- **AgentService**: Service layer integration in `crates/services/src/services/agent.rs`

### Migration Steps for Existing Code

1. **For new Claude agent features**: Use the new `/api/agents/` endpoints and `AgentSessionProvider` context
2. **For existing ContainerService usage**: Gradually migrate to `AgentService` for Claude execution
3. **For direct executor usage**: Replace `ClaudeExecutor` with `SidecarManager.create_session()`

### Deprecated Components

The following components are deprecated and should not be used for new development:

- `crates/executors/src/executors/claude.rs` - Marked `#[deprecated]`
- `crates/executors/src/executors/claude/client.rs` - Will be removed
- `crates/executors/src/executors/claude/protocol.rs` - Will be removed

### API Comparison

**Old (CLI spawning):**
```rust
// Spawns Claude CLI as subprocess
let executor = ClaudeExecutor::new(config);
executor.spawn_and_stream(workspace, prompt).await?;
```

**New (SDK sidecar):**
```rust
// Uses Agent SDK via sidecar
let sidecar = sidecar_manager.start_sidecar(workspace_id).await?;
sidecar_manager.create_session(workspace_id, session_id, agent_type, config).await?;
sidecar_manager.query(workspace_id, session_id, prompt, None).await?;
```

### Frontend Migration

**Old pattern:**
```tsx
// Streaming logs from ExecutionProcess
const { logs } = useLogStream(executionProcessId);
```

**New pattern:**
```tsx
// Using AgentSessionProvider
<AgentSessionProvider initialSessionId={sessionId}>
  <AgentSessionPanel />
</AgentSessionProvider>
```
