/**
 * Job Types
 *
 * Defines the job model for agent execution, including pause/resume
 * support for interactive sessions.
 */

import type { MCPServerConfig } from './agents.js';

/**
 * Job status values
 */
export type JobStatus = 'queued' | 'running' | 'paused' | 'completed' | 'failed' | 'cancelled';

/**
 * Agent type configuration
 */
export interface AgentType {
  name: string;
  tools: string[];
  maxTurns?: number;
  mcpServers?: Record<string, MCPServerConfig>;
}

/**
 * Pending question when job is paused
 */
export interface PendingQuestion {
  id: string;
  question: string;
  options?: Array<{
    label: string;
    description?: string;
  }>;
  header?: string;
  multiSelect?: boolean;
  timestamp: Date;
}

/**
 * Job progress information
 */
export interface JobProgress {
  phase: string;
  percentage: number;
  message: string;
}

/**
 * Job result
 */
export interface JobResult {
  success: boolean;
  summary: string;
  artifacts?: Record<string, unknown>;
}

/**
 * Job error
 */
export interface JobError {
  code: string;
  message: string;
  details?: unknown;
}

/**
 * Job model
 */
export interface Job {
  id: string;
  workspaceId: string;
  workspacePath: string;
  sessionId?: string; // Claude session ID for resumption
  prompt: string;
  agentType: string;
  status: JobStatus;
  progress?: JobProgress;
  result?: JobResult;
  error?: JobError;
  pendingQuestion?: PendingQuestion;
  metadata?: Record<string, unknown>;
  createdAt: Date;
  startedAt?: Date;
  completedAt?: Date;
  pausedAt?: Date;
}

/**
 * Request to start a new job
 */
export interface StartJobRequest {
  workspaceId: string;
  workspacePath: string;
  prompt: string;
  agentType?: string; // Defaults to 'coding'
  sessionId?: string; // For resuming a previous session
  allowedTools?: string[];
  metadata?: Record<string, unknown>;
}

/**
 * Request to resume a paused job
 */
export interface ResumeJobRequest {
  answer: string;
  questionId?: string;
}
