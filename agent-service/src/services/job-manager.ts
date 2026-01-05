/**
 * Job Manager Service
 *
 * Manages job lifecycle including pause/resume for interactive sessions.
 * This is the core component that coordinates between:
 * - The Agent Client (executes Claude via SDK)
 * - The Stream Manager (broadcasts events to SSE clients)
 * - The API routes (receives commands from Rust backend)
 */

import { v4 as uuidv4 } from 'uuid';
import { config } from '../config/index.js';
import { getStreamManager } from './stream-manager.js';
import { getAgentClient } from './agent-client.js';
import type { Job, JobStatus, StartJobRequest, ResumeJobRequest, PendingQuestion } from '../types/job.js';
import type { AgentEvent } from '../types/events.js';

/**
 * Internal state for a running job
 */
interface RunningJobState {
  job: Job;
  abortController: AbortController;
  resumeResolver?: (answer: string) => void;
}

export class JobManager {
  private jobs: Map<string, Job> = new Map();
  private runningJobs: Map<string, RunningJobState> = new Map();
  private maxConcurrentJobs: number;

  constructor(maxConcurrentJobs?: number) {
    this.maxConcurrentJobs = maxConcurrentJobs ?? config.maxConcurrentJobs;
  }

  /**
   * Start a new job
   */
  async startJob(request: StartJobRequest): Promise<Job> {
    // Check concurrency limit
    if (!this.canStartJob()) {
      throw new Error('Maximum concurrent jobs limit reached');
    }

    // Create job
    const job: Job = {
      id: uuidv4(),
      workspaceId: request.workspaceId,
      workspacePath: request.workspacePath,
      sessionId: request.sessionId,
      prompt: request.prompt,
      agentType: request.agentType || 'coding',
      status: 'queued',
      metadata: request.metadata,
      createdAt: new Date(),
    };

    this.jobs.set(job.id, job);

    // Start execution asynchronously
    this.executeJob(job, request.allowedTools);

    return job;
  }

  /**
   * Get a job by ID
   */
  getJob(id: string): Job | undefined {
    return this.jobs.get(id);
  }

  /**
   * Resume a paused job with user input
   */
  async resumeJob(id: string, request: ResumeJobRequest): Promise<Job | undefined> {
    const state = this.runningJobs.get(id);
    if (!state) {
      return undefined;
    }

    const job = state.job;
    if (job.status !== 'paused') {
      throw new Error(`Job ${id} is not paused (status: ${job.status})`);
    }

    // Verify question ID if provided
    if (request.questionId && job.pendingQuestion?.id !== request.questionId) {
      throw new Error(`Question ID mismatch`);
    }

    // Clear pending question
    job.pendingQuestion = undefined;
    job.pausedAt = undefined;
    job.status = 'running';
    this.updateJob(job);

    // Notify stream subscribers
    const streamManager = getStreamManager();
    streamManager.sendStatus(job.id, 'running');

    // Resume the agent with the answer
    if (state.resumeResolver) {
      state.resumeResolver(request.answer);
      state.resumeResolver = undefined;
    }

    return job;
  }

  /**
   * Cancel a running or paused job
   */
  async cancelJob(id: string): Promise<boolean> {
    const state = this.runningJobs.get(id);
    if (!state) {
      const job = this.jobs.get(id);
      if (job && (job.status === 'queued' || job.status === 'running' || job.status === 'paused')) {
        job.status = 'cancelled';
        job.completedAt = new Date();
        this.updateJob(job);
        return true;
      }
      return false;
    }

    // Abort the running job
    state.abortController.abort();

    // Update job status
    const job = state.job;
    job.status = 'cancelled';
    job.completedAt = new Date();
    this.updateJob(job);

    // Clean up
    this.runningJobs.delete(id);

    // Notify stream subscribers
    const streamManager = getStreamManager();
    streamManager.sendStatus(job.id, 'cancelled');
    streamManager.sendComplete(job.id, Date.now() - (job.startedAt?.getTime() || job.createdAt.getTime()));

    return true;
  }

  /**
   * Check if we can start a new job
   */
  canStartJob(): boolean {
    return this.getRunningCount() < this.maxConcurrentJobs;
  }

  /**
   * Get count of running jobs
   */
  getRunningCount(): number {
    return this.runningJobs.size;
  }

  /**
   * Update job in store
   */
  private updateJob(job: Job): void {
    this.jobs.set(job.id, job);
  }

  /**
   * Execute a job (internal)
   */
  private async executeJob(job: Job, allowedTools?: string[]): Promise<void> {
    const abortController = new AbortController();
    const state: RunningJobState = {
      job,
      abortController,
    };
    this.runningJobs.set(job.id, state);

    // Update job status
    job.status = 'running';
    job.startedAt = new Date();
    this.updateJob(job);

    const streamManager = getStreamManager();
    streamManager.sendStatus(job.id, 'running');

    try {
      const agentClient = getAgentClient();

      // Execute the agent
      for await (const event of agentClient.executeJob(job, allowedTools, abortController.signal)) {
        // Handle the event
        await this.handleAgentEvent(state, event);

        // Check if we need to pause for user input
        if (event.type === 'pause' && event.question) {
          job.status = 'paused';
          job.pausedAt = new Date();
          job.pendingQuestion = {
            id: event.question.id,
            question: event.question.question,
            options: event.question.options,
            header: event.question.header,
            multiSelect: event.question.multiSelect,
            timestamp: new Date(),
          };
          this.updateJob(job);

          // Notify stream subscribers
          streamManager.sendStatus(job.id, 'paused');
          streamManager.sendQuestion(job.id, {
            questionId: event.question.id,
            question: event.question.question,
            options: event.question.options,
            header: event.question.header,
            multiSelect: event.question.multiSelect,
          });

          // Wait for user input
          const answer = await this.waitForResume(state);

          // Resume the agent with the answer
          // The agent client will receive this via the generator
          agentClient.provideAnswer(job.id, answer);
        }

        // Capture session ID for future resumption and emit to SSE
        if (event.sessionId && !job.sessionId) {
          job.sessionId = event.sessionId;
          this.updateJob(job);
          // Emit session ID so Rust backend can store it for resumption
          streamManager.sendSessionId(job.id, event.sessionId);
        }
      }

      // Job completed successfully
      job.status = 'completed';
      job.completedAt = new Date();
      this.updateJob(job);

      streamManager.sendStatus(job.id, 'completed');
      streamManager.sendComplete(job.id, Date.now() - (job.startedAt?.getTime() || job.createdAt.getTime()));
    } catch (error) {
      // Handle job failure
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';

      if (abortController.signal.aborted) {
        // Job was cancelled, already handled
        return;
      }

      job.status = 'failed';
      job.completedAt = new Date();
      job.error = {
        code: 'EXECUTION_ERROR',
        message: errorMessage,
      };
      this.updateJob(job);

      streamManager.sendError(job.id, {
        code: 'EXECUTION_ERROR',
        message: errorMessage,
      });
      streamManager.sendStatus(job.id, 'failed');
      streamManager.sendComplete(job.id, Date.now() - (job.startedAt?.getTime() || job.createdAt.getTime()));
    } finally {
      this.runningJobs.delete(job.id);
    }
  }

  /**
   * Handle an agent event
   */
  private async handleAgentEvent(state: RunningJobState, event: AgentEvent): Promise<void> {
    const streamManager = getStreamManager();
    const job = state.job;

    switch (event.type) {
      case 'text':
        streamManager.sendOutput(job.id, {
          type: 'text',
          content: event.content || '',
          isPartial: event.isPartial,
        });
        break;

      case 'thinking':
        streamManager.sendOutput(job.id, {
          type: 'thinking',
          content: event.content || '',
          isPartial: event.isPartial,
        });
        break;

      case 'tool_use':
        streamManager.sendOutput(job.id, {
          type: 'tool_use',
          content: event.content || '',
          toolName: event.toolName,
          toolInput: event.toolInput,
        });
        break;

      case 'tool_result':
        streamManager.sendOutput(job.id, {
          type: 'tool_result',
          content: event.content || '',
          toolName: event.toolName,
        });
        break;

      case 'error':
        if (event.error) {
          streamManager.sendError(job.id, {
            code: event.error.code,
            message: event.error.message,
            details: event.error.details,
          });
        }
        break;

      // 'pause' and 'complete' are handled in executeJob
    }
  }

  /**
   * Wait for the user to resume the job with an answer
   */
  private waitForResume(state: RunningJobState): Promise<string> {
    return new Promise((resolve) => {
      state.resumeResolver = resolve;
    });
  }

  /**
   * Shutdown the job manager
   */
  async shutdown(): Promise<void> {
    // Cancel all running jobs
    for (const [id] of this.runningJobs) {
      await this.cancelJob(id);
    }
  }
}

// Singleton instance
let instance: JobManager | null = null;

export function getJobManager(): JobManager {
  if (!instance) {
    instance = new JobManager();
  }
  return instance;
}
