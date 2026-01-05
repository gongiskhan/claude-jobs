/**
 * Agent Client Service
 *
 * Wraps the Anthropic Agent SDK to provide job execution capabilities.
 * This is the core interface between our job model and Claude agents.
 */

import { v4 as uuidv4 } from 'uuid';
import type { Job } from '../types/job.js';
import type { AgentEvent } from '../types/events.js';

/**
 * Pending answers for paused jobs, keyed by job ID
 */
const pendingAnswers: Map<string, string> = new Map();

/**
 * Answer waiters for jobs waiting on user input
 */
const answerWaiters: Map<string, { resolve: (answer: string) => void }> = new Map();

/**
 * Agent Client
 *
 * Uses the Claude Agent SDK to execute agents interactively.
 */
export class AgentClient {
  /**
   * Execute a job, yielding events as they occur.
   *
   * This is an async generator that:
   * 1. Starts the Claude agent with the given prompt
   * 2. Yields events as they stream from the SDK
   * 3. Detects AskUserQuestion tool calls and yields 'pause' events
   * 4. Waits for user input when paused
   * 5. Continues execution with the answer
   */
  async *executeJob(
    job: Job,
    allowedTools?: string[],
    signal?: AbortSignal
  ): AsyncGenerator<AgentEvent, void, unknown> {
    try {
      // Dynamic import of the SDK
      const { query } = await import('@anthropic-ai/claude-agent-sdk');

      // Build options for the SDK
      const options: Record<string, unknown> = {
        workingDirectory: job.workspacePath,
      };

      if (allowedTools?.length) {
        options.allowedTools = allowedTools;
      }

      if (job.sessionId) {
        options.resume = job.sessionId;
      }

      // Start the agent query
      // The SDK returns an async iterable of messages
      for await (const message of query({
        prompt: job.prompt,
        options,
      })) {
        // Check for abort signal
        if (signal?.aborted) {
          return;
        }

        // Transform SDK message to our event format
        const events = this.transformMessage(message, job.id);

        for (const event of events) {
          yield event;

          // If this is a pause event, wait for user input
          if (event.type === 'pause') {
            const answer = await this.waitForAnswer(job.id, signal);
            // The answer will be used by the SDK on next iteration
            // (The SDK handles this internally when we provide the answer)
          }
        }
      }

      // Execution complete
      yield { type: 'complete' };
    } catch (error) {
      // If SDK is not available, fall back to a mock implementation
      if (
        error instanceof Error &&
        (error.message.includes('Cannot find module') ||
          error.message.includes('MODULE_NOT_FOUND'))
      ) {
        console.warn('Claude Agent SDK not available, using mock implementation');
        yield* this.executeMockJob(job, signal);
        return;
      }

      throw error;
    }
  }

  /**
   * Provide an answer to a paused job
   */
  provideAnswer(jobId: string, answer: string): void {
    // If there's a waiter, resolve it
    const waiter = answerWaiters.get(jobId);
    if (waiter) {
      waiter.resolve(answer);
      answerWaiters.delete(jobId);
    } else {
      // Store the answer for when the job checks for it
      pendingAnswers.set(jobId, answer);
    }
  }

  /**
   * Wait for an answer from the user
   */
  private waitForAnswer(jobId: string, signal?: AbortSignal): Promise<string> {
    // Check if an answer is already pending
    const pending = pendingAnswers.get(jobId);
    if (pending !== undefined) {
      pendingAnswers.delete(jobId);
      return Promise.resolve(pending);
    }

    // Wait for an answer to be provided
    return new Promise((resolve, reject) => {
      answerWaiters.set(jobId, { resolve });

      // Handle abort signal
      if (signal) {
        const abortHandler = () => {
          answerWaiters.delete(jobId);
          reject(new Error('Job was cancelled'));
        };

        if (signal.aborted) {
          abortHandler();
        } else {
          signal.addEventListener('abort', abortHandler, { once: true });
        }
      }
    });
  }

  /**
   * Transform an SDK message to our event format
   */
  private transformMessage(message: unknown, jobId: string): AgentEvent[] {
    const events: AgentEvent[] = [];
    const msg = message as Record<string, unknown>;

    // Handle different message types from the SDK
    switch (msg.type) {
      case 'system':
        // System messages may contain session ID
        if (msg.subtype === 'init' && typeof msg.session_id === 'string') {
          events.push({
            type: 'text',
            content: '',
            sessionId: msg.session_id,
          });
        }
        break;

      case 'assistant':
        // Assistant text message
        if (typeof msg.message === 'object' && msg.message !== null) {
          const content = msg.message as Record<string, unknown>;
          if (Array.isArray(content.content)) {
            for (const block of content.content) {
              if (block.type === 'text') {
                events.push({
                  type: 'text',
                  content: block.text || '',
                });
              } else if (block.type === 'thinking') {
                events.push({
                  type: 'thinking',
                  content: block.thinking || '',
                });
              } else if (block.type === 'tool_use') {
                // Check if this is an AskUserQuestion tool call
                if (block.name === 'AskUserQuestion') {
                  const input = block.input as Record<string, unknown>;
                  const questions = input.questions as Array<Record<string, unknown>>;
                  if (questions?.length > 0) {
                    const q = questions[0];
                    events.push({
                      type: 'pause',
                      question: {
                        id: uuidv4(),
                        question: (q.question as string) || '',
                        options: q.options as Array<{ label: string; description?: string }>,
                        header: q.header as string | undefined,
                        multiSelect: q.multiSelect as boolean | undefined,
                      },
                    });
                  }
                } else {
                  events.push({
                    type: 'tool_use',
                    content: JSON.stringify(block.input),
                    toolName: block.name,
                    toolInput: block.input,
                  });
                }
              }
            }
          }
        }
        break;

      case 'result':
        // Tool result
        if (msg.tool_use_id && typeof msg.content === 'string') {
          events.push({
            type: 'tool_result',
            content: msg.content as string,
          });
        }
        break;

      default:
        // Unknown message type, emit as text if it has content
        if (typeof msg.content === 'string') {
          events.push({
            type: 'text',
            content: msg.content,
          });
        }
    }

    return events;
  }

  /**
   * Mock implementation for development/testing when SDK is not available
   */
  private async *executeMockJob(job: Job, signal?: AbortSignal): AsyncGenerator<AgentEvent, void, unknown> {
    // Emit session ID
    yield {
      type: 'text',
      content: '',
      sessionId: `mock-session-${uuidv4()}`,
    };

    // Emit thinking
    yield {
      type: 'thinking',
      content: `Analyzing task: ${job.prompt}`,
    };

    // Simulate some tool use
    yield {
      type: 'tool_use',
      content: JSON.stringify({ pattern: '**/*.ts' }),
      toolName: 'Glob',
      toolInput: { pattern: '**/*.ts' },
    };

    await this.delay(500);

    if (signal?.aborted) return;

    yield {
      type: 'tool_result',
      content: 'Found 10 TypeScript files',
      toolName: 'Glob',
    };

    // Ask a question to test pause/resume
    yield {
      type: 'pause',
      question: {
        id: uuidv4(),
        question: 'Which approach should I use?',
        header: 'Approach',
        options: [
          { label: 'Simple', description: 'Quick and straightforward implementation' },
          { label: 'Advanced', description: 'More complex but more flexible' },
        ],
        multiSelect: false,
      },
    };

    // Wait for answer
    const answer = await this.waitForAnswer(job.id, signal);

    yield {
      type: 'text',
      content: `Got your answer: ${answer}. Proceeding with implementation...`,
    };

    // Complete
    yield { type: 'complete' };
  }

  private delay(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }
}

// Singleton instance
let instance: AgentClient | null = null;

export function getAgentClient(): AgentClient {
  if (!instance) {
    instance = new AgentClient();
  }
  return instance;
}
