/**
 * Agent Type Definitions
 *
 * Defines different agent types with their configurations, tools, and constraints.
 * Each agent type is optimized for specific tasks.
 */

/**
 * Tool permissions for agents
 */
export interface AgentTools {
  /** List of allowed tool names */
  allowed: string[];
  /** List of explicitly denied tool names */
  denied?: string[];
}

/**
 * MCP server configuration for an agent
 */
export interface MCPServerConfig {
  name: string;
  command: string;
  args?: string[];
  env?: Record<string, string>;
}

/**
 * Agent type configuration
 */
export interface AgentTypeConfig {
  /** Unique identifier for the agent type */
  id: string;
  /** Display name */
  name: string;
  /** Description of what this agent does */
  description: string;
  /** Tool configuration */
  tools: AgentTools;
  /** MCP servers to enable */
  mcpServers?: MCPServerConfig[];
  /** Maximum turns before forcing completion */
  maxTurns?: number;
  /** Custom system prompt additions */
  systemPromptAdditions?: string;
  /** Constraints/guardrails for the agent */
  constraints?: string[];
}

/**
 * Built-in agent type definitions
 *
 * These define the default behavior for different coding tasks.
 */
export const AGENT_TYPES: Record<string, AgentTypeConfig> = {
  /**
   * General coding agent - full tool access for implementation work
   */
  coding: {
    id: 'coding',
    name: 'Coding Agent',
    description: 'Full-featured coding agent for implementation tasks',
    tools: {
      allowed: [
        'Read',
        'Write',
        'Edit',
        'Glob',
        'Grep',
        'Bash',
        'LSP',
        'TodoWrite',
        'AskUserQuestion',
      ],
    },
  },

  /**
   * Planning agent - read-only access for designing implementation plans
   */
  planning: {
    id: 'planning',
    name: 'Planning Agent',
    description: 'Read-only agent for creating implementation plans',
    tools: {
      allowed: ['Read', 'Glob', 'Grep', 'LSP'],
      denied: ['Write', 'Edit', 'Bash'],
    },
    maxTurns: 5,
    constraints: [
      'Do not make any code changes',
      'Focus on understanding the codebase and creating a plan',
      'Output a structured plan with clear steps',
    ],
  },

  /**
   * Testing agent - focused on running and fixing tests
   */
  testing: {
    id: 'testing',
    name: 'Testing Agent',
    description: 'Agent specialized for running and fixing tests',
    tools: {
      allowed: ['Read', 'Write', 'Edit', 'Glob', 'Grep', 'Bash', 'TodoWrite'],
    },
    constraints: [
      'Focus on test execution and fixing failing tests',
      'Run tests to verify changes',
      'Report test results clearly',
    ],
  },

  /**
   * Review agent - read-only code review
   */
  review: {
    id: 'review',
    name: 'Review Agent',
    description: 'Agent for code review and feedback',
    tools: {
      allowed: ['Read', 'Glob', 'Grep', 'LSP'],
      denied: ['Write', 'Edit', 'Bash'],
    },
    maxTurns: 3,
    constraints: [
      'Do not make any code changes',
      'Focus on reviewing code quality and potential issues',
      'Provide actionable feedback',
    ],
  },

  /**
   * Exploration agent - fast codebase exploration
   */
  explore: {
    id: 'explore',
    name: 'Exploration Agent',
    description: 'Fast agent for codebase exploration and research',
    tools: {
      allowed: ['Read', 'Glob', 'Grep', 'LSP'],
      denied: ['Write', 'Edit', 'Bash'],
    },
    maxTurns: 10,
    constraints: [
      'Do not make any code changes',
      'Focus on finding and understanding code',
      'Be thorough in exploration',
    ],
  },
};

/**
 * Get agent type configuration by ID
 */
export function getAgentType(id: string): AgentTypeConfig | undefined {
  return AGENT_TYPES[id];
}

/**
 * Get default agent type
 */
export function getDefaultAgentType(): AgentTypeConfig {
  return AGENT_TYPES.coding;
}

/**
 * Get allowed tools for an agent type
 */
export function getAllowedTools(agentTypeId: string): string[] {
  const agentType = getAgentType(agentTypeId) || getDefaultAgentType();
  return agentType.tools.allowed;
}

/**
 * Check if a tool is allowed for an agent type
 */
export function isToolAllowed(agentTypeId: string, toolName: string): boolean {
  const agentType = getAgentType(agentTypeId) || getDefaultAgentType();

  // Check if explicitly denied
  if (agentType.tools.denied?.includes(toolName)) {
    return false;
  }

  // Check if explicitly allowed
  return agentType.tools.allowed.includes(toolName);
}
