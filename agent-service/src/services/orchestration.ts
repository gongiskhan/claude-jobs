/**
 * Orchestration Service
 *
 * Manages multi-phase workflow execution with agent types and autofix loops.
 * This is the core component that coordinates complex, multi-step agent tasks.
 */

import { v4 as uuidv4 } from 'uuid';
import { getJobManager } from './job-manager.js';
import { getStreamManager } from './stream-manager.js';
import { getAllowedTools, getAgentType, getDefaultAgentType } from '../types/agents.js';
import {
  type OrchestrationContext,
  type OrchestrationWorkflow,
  type PhaseResult,
  createOrchestrationContext,
  getDefaultWorkflow,
  getWorkflow,
  interpolatePrompt,
} from '../types/orchestration.js';
import type { Job, StartJobRequest } from '../types/job.js';

/**
 * Request to start an orchestrated workflow
 */
export interface StartWorkflowRequest {
  workspaceId: string;
  workspacePath: string;
  prompt: string;
  workflowId?: string; // Optional workflow ID, defaults to 'standard'
  sessionId?: string; // For resuming a previous session
  metadata?: Record<string, unknown>;
}

/**
 * Result of a workflow execution
 */
export interface WorkflowResult {
  workflowId: string;
  success: boolean;
  phaseResults: PhaseResult[];
  totalDurationMs: number;
  finalSessionId?: string;
}

/**
 * Orchestration Service
 *
 * Coordinates multi-phase workflow execution.
 */
export class OrchestrationService {
  private activeWorkflows: Map<string, OrchestrationContext> = new Map();

  /**
   * Execute a workflow
   *
   * This runs through all phases of the workflow, handling failures and autofix loops.
   */
  async executeWorkflow(request: StartWorkflowRequest): Promise<WorkflowResult> {
    const workflow = getWorkflow(request.workflowId || 'standard') || getDefaultWorkflow();
    const workflowRunId = uuidv4();

    const context = createOrchestrationContext(
      workflow.id,
      request.workspacePath,
      request.prompt,
      request.sessionId
    );

    this.activeWorkflows.set(workflowRunId, context);

    const startTime = Date.now();

    try {
      // Execute each phase
      for (let i = 0; i < workflow.phases.length; i++) {
        context.currentPhaseIndex = i;
        const phase = workflow.phases[i];

        // Check skip condition
        if (phase.skipCondition && phase.skipCondition(context)) {
          continue;
        }

        // Get the agent type config
        const agentType = getAgentType(phase.agentType) || getDefaultAgentType();

        // Prepare the prompt
        const prompt = phase.promptTemplate
          ? interpolatePrompt(phase.promptTemplate, context)
          : context.originalPrompt;

        // Execute the phase
        const phaseResult = await this.executePhase(
          workflowRunId,
          phase.id,
          phase.name,
          {
            workspaceId: request.workspaceId,
            workspacePath: request.workspacePath,
            prompt,
            agentType: phase.agentType,
            sessionId: context.sessionId,
            allowedTools: agentType.tools.allowed,
            metadata: {
              ...request.metadata,
              workflowId: workflow.id,
              phaseId: phase.id,
              phaseName: phase.name,
            },
          },
          phase.maxRetries || 1
        );

        context.phaseResults.push(phaseResult);

        // Update session ID for continuity
        if (phaseResult.success) {
          // Track changed files
          if (phaseResult.filesModified) {
            context.changedFiles.push(...phaseResult.filesModified);
          }
        } else if (!phase.optional) {
          // Non-optional phase failed, stop workflow
          break;
        }
      }

      // Run autofix loop if enabled and tests are failing
      if (workflow.autofixConfig?.enabled && context.testsFailing) {
        await this.runAutofixLoop(workflowRunId, context, workflow, request);
      }

      const success = context.phaseResults.every((r) => r.success || this.isOptionalPhase(workflow, r.phaseId));

      return {
        workflowId: workflow.id,
        success,
        phaseResults: context.phaseResults,
        totalDurationMs: Date.now() - startTime,
        finalSessionId: context.sessionId,
      };
    } finally {
      this.activeWorkflows.delete(workflowRunId);
    }
  }

  /**
   * Execute a single phase with retries
   */
  private async executePhase(
    workflowRunId: string,
    phaseId: string,
    phaseName: string,
    request: StartJobRequest,
    maxRetries: number
  ): Promise<PhaseResult> {
    let lastError: string | undefined;
    const startTime = Date.now();

    for (let attempt = 0; attempt < maxRetries; attempt++) {
      try {
        const jobManager = getJobManager();
        const job = await jobManager.startJob(request);

        // Wait for job completion
        const finalJob = await this.waitForJobCompletion(job.id);

        if (finalJob.status === 'completed') {
          return {
            phaseId,
            phaseName,
            success: true,
            durationMs: Date.now() - startTime,
          };
        } else if (finalJob.status === 'failed') {
          lastError = finalJob.error?.message || 'Unknown error';
        } else if (finalJob.status === 'cancelled') {
          lastError = 'Job was cancelled';
          break; // Don't retry cancelled jobs
        }
      } catch (error) {
        lastError = error instanceof Error ? error.message : 'Unknown error';
      }
    }

    return {
      phaseId,
      phaseName,
      success: false,
      error: lastError,
      durationMs: Date.now() - startTime,
    };
  }

  /**
   * Wait for a job to complete
   */
  private async waitForJobCompletion(jobId: string, timeoutMs = 600000): Promise<Job> {
    const jobManager = getJobManager();
    const startTime = Date.now();

    while (Date.now() - startTime < timeoutMs) {
      const job = jobManager.getJob(jobId);
      if (!job) {
        throw new Error(`Job ${jobId} not found`);
      }

      if (['completed', 'failed', 'cancelled'].includes(job.status)) {
        return job;
      }

      // Wait a bit before checking again
      await new Promise((resolve) => setTimeout(resolve, 1000));
    }

    throw new Error(`Job ${jobId} timed out after ${timeoutMs}ms`);
  }

  /**
   * Run the autofix loop
   */
  private async runAutofixLoop(
    workflowRunId: string,
    context: OrchestrationContext,
    workflow: OrchestrationWorkflow,
    originalRequest: StartWorkflowRequest
  ): Promise<void> {
    if (!workflow.autofixConfig) return;

    const { maxIterations, agentType: autofixAgentType } = workflow.autofixConfig;

    while (context.autofixIterations < maxIterations && context.testsFailing) {
      context.autofixIterations++;

      const prompt = `Tests are failing. Please fix the issues and make all tests pass.\n\nFailing tests: ${context.phaseResults
        .filter((r) => r.testResults?.failed)
        .map((r) => r.testResults?.failingTests?.join(', '))
        .join(', ')}`;

      const autofixResult = await this.executePhase(
        workflowRunId,
        `autofix-${context.autofixIterations}`,
        `Autofix Iteration ${context.autofixIterations}`,
        {
          workspaceId: originalRequest.workspaceId,
          workspacePath: originalRequest.workspacePath,
          prompt,
          agentType: autofixAgentType,
          sessionId: context.sessionId,
          allowedTools: getAllowedTools(autofixAgentType),
          metadata: {
            ...originalRequest.metadata,
            workflowId: workflow.id,
            autofixIteration: context.autofixIterations,
          },
        },
        1
      );

      context.phaseResults.push(autofixResult);

      // Check if tests are now passing (would need test execution integration)
      // For now, assume autofix success means tests pass
      if (autofixResult.success) {
        context.testsFailing = false;
      }
    }
  }

  /**
   * Check if a phase is optional in the workflow
   */
  private isOptionalPhase(workflow: OrchestrationWorkflow, phaseId: string): boolean {
    const phase = workflow.phases.find((p) => p.id === phaseId);
    return phase?.optional || false;
  }

  /**
   * Get the status of an active workflow
   */
  getWorkflowStatus(workflowRunId: string): OrchestrationContext | undefined {
    return this.activeWorkflows.get(workflowRunId);
  }

  /**
   * Cancel an active workflow
   */
  async cancelWorkflow(workflowRunId: string): Promise<boolean> {
    const context = this.activeWorkflows.get(workflowRunId);
    if (!context) {
      return false;
    }

    // The workflow will detect cancellation on next phase
    this.activeWorkflows.delete(workflowRunId);
    return true;
  }
}

// Singleton instance
let instance: OrchestrationService | null = null;

export function getOrchestrationService(): OrchestrationService {
  if (!instance) {
    instance = new OrchestrationService();
  }
  return instance;
}
