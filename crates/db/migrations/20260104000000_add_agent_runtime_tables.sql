-- Agent Runtime Tables
-- Replaces process-centric ExecutionProcess model with SDK-based agent sessions

-- Agent session state (core agent lifecycle)
CREATE TABLE agent_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    agent_type TEXT NOT NULL DEFAULT 'coding', -- 'coding', 'orchestration', 'deployment'
    state TEXT NOT NULL DEFAULT 'created', -- 'created', 'idle', 'executing', 'awaiting_input', 'paused', 'completed', 'terminated'
    sdk_session_id TEXT, -- Session ID from Claude Agent SDK
    context TEXT, -- Serialized agent context for pause/resume (JSON)
    current_tool_call_id TEXT, -- For pending approvals
    pending_question TEXT, -- Question awaiting user response
    error_message TEXT, -- Error details if terminated
    working_dir TEXT, -- Working directory for this agent
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    completed_at TEXT,

    CHECK (state IN ('created', 'idle', 'executing', 'awaiting_input', 'paused', 'completed', 'terminated')),
    CHECK (agent_type IN ('coding', 'orchestration', 'deployment'))
);

-- Agent messages (conversation history for context persistence)
CREATE TABLE agent_messages (
    id TEXT PRIMARY KEY NOT NULL,
    agent_session_id TEXT NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
    role TEXT NOT NULL, -- 'user', 'assistant', 'tool_use', 'tool_result', 'system'
    content TEXT NOT NULL,
    metadata TEXT, -- JSON: tool calls, thinking blocks, etc.
    token_count INTEGER, -- Estimated token count for context management
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),

    CHECK (role IN ('user', 'assistant', 'tool_use', 'tool_result', 'system'))
);

-- Pending user inputs (for async question/answer and approval flow)
CREATE TABLE agent_pending_inputs (
    id TEXT PRIMARY KEY NOT NULL,
    agent_session_id TEXT NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
    input_type TEXT NOT NULL, -- 'question', 'approval', 'clarification'
    prompt TEXT NOT NULL, -- The question or approval request text
    tool_call_id TEXT, -- For approval requests
    tool_name TEXT, -- For approval requests
    tool_input TEXT, -- JSON: For approval requests
    response TEXT, -- User's response
    approved INTEGER, -- For approval: 1 = approved, 0 = denied
    responded_at TEXT, -- When user responded
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),

    CHECK (input_type IN ('question', 'approval', 'clarification'))
);

-- Indexes for efficient queries
CREATE INDEX idx_agent_sessions_session_id ON agent_sessions(session_id);
CREATE INDEX idx_agent_sessions_state ON agent_sessions(state);
CREATE INDEX idx_agent_sessions_updated_at ON agent_sessions(updated_at);
CREATE INDEX idx_agent_messages_session ON agent_messages(agent_session_id);
CREATE INDEX idx_agent_messages_created_at ON agent_messages(agent_session_id, created_at);
CREATE INDEX idx_agent_pending_inputs_session ON agent_pending_inputs(agent_session_id);
CREATE INDEX idx_agent_pending_inputs_pending ON agent_pending_inputs(agent_session_id)
    WHERE responded_at IS NULL;

-- Add agent configuration to projects
ALTER TABLE projects ADD COLUMN agent_config TEXT DEFAULT '{"default_agent_type":"coding","max_context_tokens":200000,"auto_approve_read_tools":true}';

-- Trigger to update updated_at on agent_sessions
CREATE TRIGGER update_agent_sessions_updated_at
    AFTER UPDATE ON agent_sessions
    FOR EACH ROW
    WHEN OLD.updated_at = NEW.updated_at
BEGIN
    UPDATE agent_sessions SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;
