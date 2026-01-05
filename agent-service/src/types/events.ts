/**
 * SSE Event Types
 *
 * Defines the event types for streaming job updates to clients.
 */

/**
 * All SSE event types
 */
export type SSEEventType =
  | 'status'
  | 'output'
  | 'progress'
  | 'result'
  | 'error'
  | 'question'
  | 'usage'
  | 'session_id'
  | 'complete';

/**
 * SSE event type constants
 */
export const SSEEventTypes = {
  STATUS: 'status' as const,
  OUTPUT: 'output' as const,
  PROGRESS: 'progress' as const,
  RESULT: 'result' as const,
  ERROR: 'error' as const,
  QUESTION: 'question' as const,
  USAGE: 'usage' as const,
  SESSION_ID: 'session_id' as const,
  COMPLETE: 'complete' as const,
};

/**
 * Status event - job status changed
 */
export interface SSEStatusEvent {
  status: 'queued' | 'running' | 'paused' | 'completed' | 'failed' | 'cancelled';
  timestamp: string;
}

/**
 * Output event - text or tool use
 */
export interface SSEOutputEvent {
  type: 'text' | 'tool_use' | 'tool_result' | 'thinking';
  content: string;
  toolName?: string;
  toolInput?: unknown;
  isPartial?: boolean;
  timestamp: string;
}

/**
 * Progress event - phase update
 */
export interface SSEProgressEvent {
  phase: string;
  percentage: number;
  message: string;
  timestamp: string;
}

/**
 * Result event - job completed with result
 */
export interface SSEResultEvent {
  success: boolean;
  summary: string;
  artifacts?: Record<string, unknown>;
  timestamp: string;
}

/**
 * Error event - job error
 */
export interface SSEErrorEvent {
  code: string;
  message: string;
  details?: unknown;
  timestamp: string;
}

/**
 * Question event - agent is asking a question
 */
export interface SSEQuestionEvent {
  questionId: string;
  question: string;
  options?: Array<{
    label: string;
    description?: string;
  }>;
  header?: string;
  multiSelect?: boolean;
  timestamp: string;
}

/**
 * Usage event - token usage info
 */
export interface SSEUsageEvent {
  inputTokens: number;
  outputTokens: number;
  contextWindow?: number;
  percentage?: number;
  timestamp: string;
}

/**
 * Session ID event - agent session ID for resumption
 */
export interface SSESessionIdEvent {
  sessionId: string;
  timestamp: string;
}

/**
 * Complete event - job finished
 */
export interface SSECompleteEvent {
  jobId: string;
  duration: number;
  timestamp: string;
}

/**
 * Union of all SSE event data types
 */
export type SSEEventData =
  | SSEStatusEvent
  | SSEOutputEvent
  | SSEProgressEvent
  | SSEResultEvent
  | SSEErrorEvent
  | SSEQuestionEvent
  | SSEUsageEvent
  | SSESessionIdEvent
  | SSECompleteEvent;

/**
 * Agent event from SDK (internal)
 */
export interface AgentEvent {
  type: 'text' | 'tool_use' | 'tool_result' | 'thinking' | 'pause' | 'error' | 'complete';
  content?: string;
  toolName?: string;
  toolInput?: unknown;
  isPartial?: boolean;
  question?: {
    id: string;
    question: string;
    options?: Array<{ label: string; description?: string }>;
    header?: string;
    multiSelect?: boolean;
  };
  error?: {
    code: string;
    message: string;
    details?: unknown;
  };
  sessionId?: string;
}
