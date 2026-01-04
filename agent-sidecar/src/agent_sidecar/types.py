"""Type definitions for the agent sidecar."""

from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Optional
from uuid import UUID


class AgentType(str, Enum):
    """Types of agents supported by the sidecar."""

    CODING = "coding"
    ORCHESTRATION = "orchestration"
    DEPLOYMENT = "deployment"


class AgentState(str, Enum):
    """State machine states for agent sessions."""

    CREATED = "created"
    IDLE = "idle"
    EXECUTING = "executing"
    AWAITING_INPUT = "awaiting_input"
    PAUSED = "paused"
    COMPLETED = "completed"
    TERMINATED = "terminated"

    def can_transition_to(self, target: "AgentState") -> bool:
        """Check if a state transition is valid."""
        valid_transitions = {
            AgentState.CREATED: {AgentState.IDLE, AgentState.TERMINATED},
            AgentState.IDLE: {AgentState.EXECUTING, AgentState.TERMINATED},
            AgentState.EXECUTING: {
                AgentState.IDLE,
                AgentState.AWAITING_INPUT,
                AgentState.PAUSED,
                AgentState.COMPLETED,
                AgentState.TERMINATED,
            },
            AgentState.AWAITING_INPUT: {
                AgentState.EXECUTING,
                AgentState.PAUSED,
                AgentState.TERMINATED,
            },
            AgentState.PAUSED: {AgentState.EXECUTING, AgentState.TERMINATED},
            AgentState.COMPLETED: set(),
            AgentState.TERMINATED: set(),
        }
        return target in valid_transitions.get(self, set())

    def is_terminal(self) -> bool:
        """Check if this is a terminal state."""
        return self in {AgentState.COMPLETED, AgentState.TERMINATED}

    def can_query(self) -> bool:
        """Check if agent can receive new queries."""
        return self == AgentState.IDLE


@dataclass
class AgentConfig:
    """Configuration for an agent session."""

    working_dir: str
    system_prompt: Optional[str] = None
    max_context_tokens: int = 200000
    auto_approve_read_tools: bool = True
    allowed_tools: Optional[list[str]] = None
    env_vars: dict[str, str] = field(default_factory=dict)


@dataclass
class SessionContext:
    """Serializable context for pause/resume."""

    messages: list[dict[str, Any]]
    sdk_session_id: Optional[str] = None
    metadata: dict[str, Any] = field(default_factory=dict)


# Event types sent from sidecar to Rust backend
@dataclass
class StateChangedEvent:
    """Agent state has changed."""

    session_id: UUID
    from_state: AgentState
    to_state: AgentState


@dataclass
class MessageStreamEvent:
    """Streaming message content."""

    session_id: UUID
    content_type: str  # 'text', 'thinking', 'tool_use', 'tool_result'
    content: str
    metadata: Optional[dict[str, Any]] = None


@dataclass
class ToolUseEvent:
    """Agent is using a tool."""

    session_id: UUID
    tool_call_id: str
    tool_name: str
    tool_input: dict[str, Any]


@dataclass
class ToolResultEvent:
    """Tool execution result."""

    session_id: UUID
    tool_call_id: str
    result: Any
    is_error: bool = False


@dataclass
class QuestionAskedEvent:
    """Agent is asking a question."""

    session_id: UUID
    question: str
    input_id: UUID


@dataclass
class ApprovalNeededEvent:
    """Agent needs tool approval."""

    session_id: UUID
    tool_call_id: str
    tool_name: str
    tool_input: dict[str, Any]
    input_id: UUID


@dataclass
class CompletedEvent:
    """Agent execution completed."""

    session_id: UUID
    result: Optional[str] = None


@dataclass
class ErrorEvent:
    """Agent execution error."""

    session_id: UUID
    error: str


# Union type for all events
AgentEvent = (
    StateChangedEvent
    | MessageStreamEvent
    | ToolUseEvent
    | ToolResultEvent
    | QuestionAskedEvent
    | ApprovalNeededEvent
    | CompletedEvent
    | ErrorEvent
)


# Command types sent from Rust backend to sidecar
@dataclass
class CreateSessionCommand:
    """Create a new agent session."""

    session_id: UUID
    agent_type: AgentType
    config: AgentConfig


@dataclass
class QueryCommand:
    """Send a query to an agent."""

    session_id: UUID
    prompt: str
    context: Optional[SessionContext] = None


@dataclass
class ResumeCommand:
    """Resume with user response."""

    session_id: UUID
    input_id: UUID
    response: str


@dataclass
class ApproveToolCommand:
    """Approve or deny tool use."""

    session_id: UUID
    input_id: UUID
    tool_call_id: str
    approved: bool
    reason: Optional[str] = None


@dataclass
class InterruptCommand:
    """Interrupt current execution."""

    session_id: UUID


@dataclass
class PauseCommand:
    """Pause execution."""

    session_id: UUID


@dataclass
class TerminateCommand:
    """Terminate session."""

    session_id: UUID


@dataclass
class GetContextCommand:
    """Get serialized context for persistence."""

    session_id: UUID


AgentCommand = (
    CreateSessionCommand
    | QueryCommand
    | ResumeCommand
    | ApproveToolCommand
    | InterruptCommand
    | PauseCommand
    | TerminateCommand
    | GetContextCommand
)
