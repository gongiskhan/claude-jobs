/**
 * Orchestration Types
 *
 * Defines types for multi-phase workflow execution.
 * Orchestration allows chaining multiple agent phases (planning, coding, testing)
 * with automatic iteration on failures.
 */

import type { AgentTypeConfig } from './agents.js';

/**
 * Execution phase in an orchestrated workflow
 */
export interface OrchestrationPhase {
  /** Phase identifier */
  id: string;
  /** Human-readable name */
  name: string;
  /** Agent type to use for this phase */
  agentType: string;
  /** Prompt template for this phase (can include {{variables}}) */
  promptTemplate?: string;
  /** Whether this phase is optional */
  optional?: boolean;
  /** Condition to skip this phase */
  skipCondition?: (context: OrchestrationContext) => boolean;
  /** Maximum retries for this phase */
  maxRetries?: number;
}

/**
 * Workflow configuration for orchestrated execution
 */
export interface OrchestrationWorkflow {
  /** Workflow identifier */
  id: string;
  /** Human-readable name */
  name: string;
  /** Description of what this workflow does */
  description: string;
  /** Ordered list of phases */
  phases: OrchestrationPhase[];
  /** Configuration for autofix loop */
  autofixConfig?: AutofixConfig;
}

/**
 * Configuration for the autofix loop
 */
export interface AutofixConfig {
  /** Whether autofix is enabled */
  enabled: boolean;
  /** Maximum autofix iterations */
  maxIterations: number;
  /** Agent type to use for fixing */
  agentType: string;
  /** Test command to verify fixes */
  testCommand?: string;
}

/**
 * Runtime context during orchestration
 */
export interface OrchestrationContext {
  /** ID of the workflow being executed */
  workflowId: string;
  /** Current phase index */
  currentPhaseIndex: number;
  /** Results from each completed phase */
  phaseResults: PhaseResult[];
  /** Number of autofix iterations completed */
  autofixIterations: number;
  /** Whether any tests are failing */
  testsFailing: boolean;
  /** Accumulated file changes */
  changedFiles: string[];
  /** Session ID for agent continuity */
  sessionId?: string;
  /** Workspace path */
  workspacePath: string;
  /** Original user prompt */
  originalPrompt: string;
}

/**
 * Result of a single phase execution
 */
export interface PhaseResult {
  /** Phase ID */
  phaseId: string;
  /** Phase name */
  phaseName: string;
  /** Whether the phase succeeded */
  success: boolean;
  /** Summary from the agent */
  summary?: string;
  /** Any error that occurred */
  error?: string;
  /** Duration in milliseconds */
  durationMs: number;
  /** Files modified in this phase */
  filesModified?: string[];
  /** Test results if applicable */
  testResults?: TestResults;
}

/**
 * Test execution results
 */
export interface TestResults {
  /** Total number of tests */
  total: number;
  /** Number of passed tests */
  passed: number;
  /** Number of failed tests */
  failed: number;
  /** Number of skipped tests */
  skipped: number;
  /** List of failing test names */
  failingTests?: string[];
  /** Raw test output */
  output?: string;
}

/**
 * Built-in workflow definitions
 */
export const WORKFLOWS: Record<string, OrchestrationWorkflow> = {
  /**
   * Standard coding workflow with optional planning and testing
   */
  standard: {
    id: 'standard',
    name: 'Standard Coding',
    description: 'Default workflow: optional planning, coding, optional testing',
    phases: [
      {
        id: 'coding',
        name: 'Implementation',
        agentType: 'coding',
      },
    ],
    autofixConfig: {
      enabled: true,
      maxIterations: 3,
      agentType: 'coding',
    },
  },

  /**
   * Full workflow with planning, coding, testing, and review
   */
  full: {
    id: 'full',
    name: 'Full Workflow',
    description: 'Complete workflow: planning, coding, testing, review',
    phases: [
      {
        id: 'planning',
        name: 'Planning',
        agentType: 'planning',
        promptTemplate: 'Create a detailed implementation plan for: {{originalPrompt}}',
        optional: true,
      },
      {
        id: 'coding',
        name: 'Implementation',
        agentType: 'coding',
        promptTemplate: '{{originalPrompt}}',
      },
      {
        id: 'testing',
        name: 'Testing',
        agentType: 'testing',
        promptTemplate: 'Run tests and verify the implementation is correct.',
        optional: true,
      },
    ],
    autofixConfig: {
      enabled: true,
      maxIterations: 5,
      agentType: 'coding',
    },
  },

  /**
   * Test-driven workflow: write tests first, then implement
   */
  tdd: {
    id: 'tdd',
    name: 'Test-Driven',
    description: 'TDD workflow: write tests first, then implement to pass',
    phases: [
      {
        id: 'planning',
        name: 'Planning',
        agentType: 'planning',
        promptTemplate:
          'Analyze the requirements and plan what tests need to be written: {{originalPrompt}}',
      },
      {
        id: 'testing',
        name: 'Write Tests',
        agentType: 'coding',
        promptTemplate:
          'Write failing tests that will verify the implementation: {{originalPrompt}}',
      },
      {
        id: 'coding',
        name: 'Implement',
        agentType: 'coding',
        promptTemplate: 'Implement the feature to make the tests pass: {{originalPrompt}}',
      },
    ],
    autofixConfig: {
      enabled: true,
      maxIterations: 5,
      agentType: 'coding',
    },
  },

  /**
   * Exploration workflow: understand codebase, then implement
   */
  explore: {
    id: 'explore',
    name: 'Explore First',
    description: 'Explore the codebase thoroughly before implementing',
    phases: [
      {
        id: 'exploration',
        name: 'Exploration',
        agentType: 'explore',
        promptTemplate:
          'Explore the codebase to understand how to implement: {{originalPrompt}}',
      },
      {
        id: 'coding',
        name: 'Implementation',
        agentType: 'coding',
      },
    ],
    autofixConfig: {
      enabled: true,
      maxIterations: 3,
      agentType: 'coding',
    },
  },
};

/**
 * Get workflow by ID
 */
export function getWorkflow(id: string): OrchestrationWorkflow | undefined {
  return WORKFLOWS[id];
}

/**
 * Get default workflow
 */
export function getDefaultWorkflow(): OrchestrationWorkflow {
  return WORKFLOWS.standard;
}

/**
 * Create an initial orchestration context
 */
export function createOrchestrationContext(
  workflowId: string,
  workspacePath: string,
  originalPrompt: string,
  sessionId?: string
): OrchestrationContext {
  return {
    workflowId,
    currentPhaseIndex: 0,
    phaseResults: [],
    autofixIterations: 0,
    testsFailing: false,
    changedFiles: [],
    sessionId,
    workspacePath,
    originalPrompt,
  };
}

/**
 * Interpolate variables in a prompt template
 */
export function interpolatePrompt(template: string, context: OrchestrationContext): string {
  return template
    .replace(/\{\{originalPrompt\}\}/g, context.originalPrompt)
    .replace(/\{\{workspacePath\}\}/g, context.workspacePath)
    .replace(/\{\{sessionId\}\}/g, context.sessionId || '');
}
