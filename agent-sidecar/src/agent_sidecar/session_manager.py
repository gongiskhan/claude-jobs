"""Agent session manager using Claude Agent SDK."""

import asyncio
import logging
from typing import Any, Callable, Coroutine, Optional
from uuid import UUID, uuid4

from .types import (
    AgentConfig,
    AgentEvent,
    AgentState,
    AgentType,
    ApprovalNeededEvent,
    CompletedEvent,
    ErrorEvent,
    MessageStreamEvent,
    QuestionAskedEvent,
    SessionContext,
    StateChangedEvent,
    ToolResultEvent,
    ToolUseEvent,
)

logger = logging.getLogger(__name__)

# Type alias for event callback
EventCallback = Callable[[AgentEvent], Coroutine[Any, Any, None]]


class AgentSession:
    """Represents a single agent session with the Claude Agent SDK."""

    def __init__(
        self,
        session_id: UUID,
        agent_type: AgentType,
        config: AgentConfig,
        event_callback: EventCallback,
    ):
        self.session_id = session_id
        self.agent_type = agent_type
        self.config = config
        self.event_callback = event_callback

        self._state = AgentState.CREATED
        self._sdk_session_id: Optional[str] = None
        self._messages: list[dict[str, Any]] = []
        self._client: Any = None  # ClaudeSDKClient instance

        # For pause/resume with user input
        self._pending_response: Optional[asyncio.Event] = None
        self._user_response: Optional[str] = None
        self._pending_input_id: Optional[UUID] = None

        # For tool approval
        self._pending_approval: Optional[asyncio.Event] = None
        self._approval_result: Optional[tuple[bool, Optional[str]]] = None
        self._pending_approval_id: Optional[UUID] = None

        # For interrupt/pause
        self._interrupt_requested = False
        self._pause_requested = False

    @property
    def state(self) -> AgentState:
        """Get current agent state."""
        return self._state

    async def _set_state(self, new_state: AgentState) -> None:
        """Set state and emit event."""
        if not self._state.can_transition_to(new_state):
            logger.warning(
                f"Invalid state transition from {self._state} to {new_state} "
                f"for session {self.session_id}"
            )
            return

        old_state = self._state
        self._state = new_state

        await self.event_callback(
            StateChangedEvent(
                session_id=self.session_id,
                from_state=old_state,
                to_state=new_state,
            )
        )

    async def initialize(self) -> None:
        """Initialize the agent session with the SDK."""
        try:
            # Import here to avoid issues if SDK not installed
            from claude_agent_sdk import ClaudeAgentOptions, ClaudeSDKClient

            options = ClaudeAgentOptions(
                system_prompt=self._get_system_prompt(),
                cwd=self.config.working_dir,
                setting_sources=["project"],
            )

            # Create the SDK client
            self._client = ClaudeSDKClient(options)
            await self._client.__aenter__()

            await self._set_state(AgentState.IDLE)
            logger.info(f"Session {self.session_id} initialized successfully")

        except ImportError:
            logger.error("claude-agent-sdk not installed")
            await self._set_state(AgentState.TERMINATED)
            await self.event_callback(
                ErrorEvent(
                    session_id=self.session_id,
                    error="claude-agent-sdk not installed",
                )
            )
        except Exception as e:
            logger.exception(f"Failed to initialize session {self.session_id}")
            await self._set_state(AgentState.TERMINATED)
            await self.event_callback(
                ErrorEvent(session_id=self.session_id, error=str(e))
            )

    def _get_system_prompt(self) -> str:
        """Get system prompt based on agent type."""
        if self.config.system_prompt:
            return self.config.system_prompt

        base_prompts = {
            AgentType.CODING: (
                "You are a coding assistant working in a git repository. "
                "You help implement features, fix bugs, and improve code quality. "
                "Always explain your changes and commit them with clear messages."
            ),
            AgentType.ORCHESTRATION: (
                "You are an orchestration agent that plans and coordinates tasks. "
                "Break down complex tasks into smaller steps and delegate to coding agents."
            ),
            AgentType.DEPLOYMENT: (
                "You are a deployment agent that helps with infrastructure and deployment. "
                "Help set up CI/CD, configure environments, and deploy applications."
            ),
        }
        return base_prompts.get(self.agent_type, base_prompts[AgentType.CODING])

    async def query(self, prompt: str, context: Optional[SessionContext] = None) -> None:
        """Send a query to the agent."""
        if not self._state.can_query():
            logger.warning(
                f"Cannot query session {self.session_id} in state {self._state}"
            )
            return

        await self._set_state(AgentState.EXECUTING)
        self._interrupt_requested = False
        self._pause_requested = False

        # Restore context if provided
        if context:
            self._messages = context.messages.copy()
            self._sdk_session_id = context.sdk_session_id

        # Add user message to history
        self._messages.append({"role": "user", "content": prompt})

        try:
            async for msg in self._client.query(prompt):
                # Check for interrupt/pause
                if self._interrupt_requested:
                    logger.info(f"Session {self.session_id} interrupted")
                    await self._set_state(AgentState.TERMINATED)
                    return

                if self._pause_requested:
                    logger.info(f"Session {self.session_id} paused")
                    await self._set_state(AgentState.PAUSED)
                    return

                # Process message based on type
                await self._process_message(msg)

            # Execution completed successfully
            await self._set_state(AgentState.IDLE)
            await self.event_callback(
                CompletedEvent(session_id=self.session_id)
            )

        except Exception as e:
            logger.exception(f"Error during query for session {self.session_id}")
            await self._set_state(AgentState.TERMINATED)
            await self.event_callback(
                ErrorEvent(session_id=self.session_id, error=str(e))
            )

    async def _process_message(self, msg: Any) -> None:
        """Process a message from the SDK."""
        msg_type = getattr(msg, "type", None)

        if msg_type == "text":
            # Streaming text content
            await self.event_callback(
                MessageStreamEvent(
                    session_id=self.session_id,
                    content_type="text",
                    content=msg.text,
                )
            )
            self._messages.append({"role": "assistant", "content": msg.text})

        elif msg_type == "thinking":
            # Thinking/reasoning content
            await self.event_callback(
                MessageStreamEvent(
                    session_id=self.session_id,
                    content_type="thinking",
                    content=msg.thinking,
                )
            )

        elif msg_type == "tool_use":
            # Tool use request
            await self.event_callback(
                ToolUseEvent(
                    session_id=self.session_id,
                    tool_call_id=msg.id,
                    tool_name=msg.name,
                    tool_input=msg.input,
                )
            )

            # Check if this tool needs approval
            if self._needs_approval(msg.name):
                await self._request_approval(msg.id, msg.name, msg.input)

        elif msg_type == "tool_result":
            # Tool result
            await self.event_callback(
                ToolResultEvent(
                    session_id=self.session_id,
                    tool_call_id=msg.tool_use_id,
                    result=msg.content,
                    is_error=getattr(msg, "is_error", False),
                )
            )

    def _needs_approval(self, tool_name: str) -> bool:
        """Check if a tool needs explicit approval."""
        if self.config.auto_approve_read_tools:
            read_tools = {"Read", "Glob", "Grep", "Task", "TodoWrite", "WebSearch", "WebFetch"}
            if tool_name in read_tools:
                return False
        return True

    async def _request_approval(
        self, tool_call_id: str, tool_name: str, tool_input: dict[str, Any]
    ) -> tuple[bool, Optional[str]]:
        """Request approval for a tool use."""
        self._pending_approval = asyncio.Event()
        self._pending_approval_id = uuid4()
        self._approval_result = None

        await self._set_state(AgentState.AWAITING_INPUT)

        await self.event_callback(
            ApprovalNeededEvent(
                session_id=self.session_id,
                tool_call_id=tool_call_id,
                tool_name=tool_name,
                tool_input=tool_input,
                input_id=self._pending_approval_id,
            )
        )

        # Wait for approval response
        await self._pending_approval.wait()

        result = self._approval_result or (False, "No response received")
        self._pending_approval = None
        self._pending_approval_id = None
        self._approval_result = None

        await self._set_state(AgentState.EXECUTING)
        return result

    async def ask_user(self, question: str) -> str:
        """Ask the user a question and wait for response."""
        self._pending_response = asyncio.Event()
        self._pending_input_id = uuid4()
        self._user_response = None

        await self._set_state(AgentState.AWAITING_INPUT)

        await self.event_callback(
            QuestionAskedEvent(
                session_id=self.session_id,
                question=question,
                input_id=self._pending_input_id,
            )
        )

        # Wait for user response
        await self._pending_response.wait()

        response = self._user_response or ""
        self._pending_response = None
        self._pending_input_id = None
        self._user_response = None

        await self._set_state(AgentState.EXECUTING)
        return response

    async def resume_with_response(self, input_id: UUID, response: str) -> None:
        """Resume execution with user's response."""
        if self._state != AgentState.AWAITING_INPUT:
            logger.warning(
                f"Cannot resume session {self.session_id} - not awaiting input"
            )
            return

        if self._pending_input_id != input_id:
            logger.warning(
                f"Input ID mismatch: expected {self._pending_input_id}, got {input_id}"
            )
            return

        self._user_response = response
        if self._pending_response:
            self._pending_response.set()

    async def approve_tool(
        self, input_id: UUID, tool_call_id: str, approved: bool, reason: Optional[str] = None
    ) -> None:
        """Approve or deny a tool use request."""
        if self._state != AgentState.AWAITING_INPUT:
            logger.warning(
                f"Cannot approve tool for session {self.session_id} - not awaiting input"
            )
            return

        if self._pending_approval_id != input_id:
            logger.warning(
                f"Approval ID mismatch: expected {self._pending_approval_id}, got {input_id}"
            )
            return

        self._approval_result = (approved, reason)
        if self._pending_approval:
            self._pending_approval.set()

    async def interrupt(self) -> None:
        """Interrupt current execution."""
        self._interrupt_requested = True
        # Also set any pending events to unblock
        if self._pending_response:
            self._user_response = ""
            self._pending_response.set()
        if self._pending_approval:
            self._approval_result = (False, "Interrupted")
            self._pending_approval.set()

    async def pause(self) -> None:
        """Pause execution."""
        self._pause_requested = True

    async def resume(self) -> None:
        """Resume paused execution."""
        if self._state != AgentState.PAUSED:
            logger.warning(f"Cannot resume session {self.session_id} - not paused")
            return

        await self._set_state(AgentState.EXECUTING)
        # The query loop will continue from where it left off

    async def terminate(self) -> None:
        """Terminate the session."""
        self._interrupt_requested = True
        await self.interrupt()

        if self._client:
            try:
                await self._client.__aexit__(None, None, None)
            except Exception:
                pass
            self._client = None

        if not self._state.is_terminal():
            await self._set_state(AgentState.TERMINATED)

    def get_context(self) -> SessionContext:
        """Get serializable context for persistence."""
        return SessionContext(
            messages=self._messages.copy(),
            sdk_session_id=self._sdk_session_id,
            metadata={
                "agent_type": self.agent_type.value,
                "working_dir": self.config.working_dir,
            },
        )


class AgentSessionManager:
    """Manages multiple agent sessions."""

    def __init__(self):
        self._sessions: dict[UUID, AgentSession] = {}
        self._lock = asyncio.Lock()

    async def create_session(
        self,
        session_id: UUID,
        agent_type: AgentType,
        config: AgentConfig,
        event_callback: EventCallback,
    ) -> AgentSession:
        """Create and initialize a new agent session."""
        async with self._lock:
            if session_id in self._sessions:
                raise ValueError(f"Session {session_id} already exists")

            session = AgentSession(
                session_id=session_id,
                agent_type=agent_type,
                config=config,
                event_callback=event_callback,
            )
            self._sessions[session_id] = session

        await session.initialize()
        return session

    def get_session(self, session_id: UUID) -> Optional[AgentSession]:
        """Get a session by ID."""
        return self._sessions.get(session_id)

    async def remove_session(self, session_id: UUID) -> None:
        """Remove a session."""
        async with self._lock:
            session = self._sessions.pop(session_id, None)

        if session:
            await session.terminate()

    async def terminate_all(self) -> None:
        """Terminate all sessions."""
        async with self._lock:
            sessions = list(self._sessions.values())
            self._sessions.clear()

        for session in sessions:
            try:
                await session.terminate()
            except Exception:
                logger.exception(f"Error terminating session {session.session_id}")

    def list_sessions(self) -> list[tuple[UUID, AgentState]]:
        """List all sessions with their states."""
        return [(sid, s.state) for sid, s in self._sessions.items()]
