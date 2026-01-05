/**
 * Agent Registry Service
 *
 * Manages the registry of available agent types and their configurations.
 * Allows dynamic registration of new agent types.
 */

import {
  type AgentTypeConfig,
  type MCPServerConfig,
  AGENT_TYPES,
  getAgentType,
  getDefaultAgentType,
} from '../types/agents.js';

/**
 * Agent Registry
 *
 * Provides CRUD operations for agent types and manages the registry.
 */
export class AgentRegistry {
  private customAgentTypes: Map<string, AgentTypeConfig> = new Map();

  /**
   * Get an agent type by ID
   *
   * Checks custom types first, then falls back to built-in types.
   */
  get(id: string): AgentTypeConfig | undefined {
    return this.customAgentTypes.get(id) || getAgentType(id);
  }

  /**
   * Get the default agent type
   */
  getDefault(): AgentTypeConfig {
    return getDefaultAgentType();
  }

  /**
   * List all available agent types
   */
  list(): AgentTypeConfig[] {
    // Combine built-in and custom types
    const builtIn = Object.values(AGENT_TYPES);
    const custom = Array.from(this.customAgentTypes.values());

    // Custom types override built-in types with the same ID
    const combined = new Map<string, AgentTypeConfig>();
    for (const config of builtIn) {
      combined.set(config.id, config);
    }
    for (const config of custom) {
      combined.set(config.id, config);
    }

    return Array.from(combined.values());
  }

  /**
   * Register a custom agent type
   */
  register(config: AgentTypeConfig): void {
    this.validateConfig(config);
    this.customAgentTypes.set(config.id, config);
  }

  /**
   * Unregister a custom agent type
   */
  unregister(id: string): boolean {
    return this.customAgentTypes.delete(id);
  }

  /**
   * Check if an agent type exists
   */
  exists(id: string): boolean {
    return this.customAgentTypes.has(id) || id in AGENT_TYPES;
  }

  /**
   * Get the allowed tools for an agent type
   */
  getAllowedTools(id: string): string[] {
    const config = this.get(id);
    if (!config) {
      return this.getDefault().tools.allowed;
    }
    return config.tools.allowed;
  }

  /**
   * Get the denied tools for an agent type
   */
  getDeniedTools(id: string): string[] {
    const config = this.get(id);
    return config?.tools.denied || [];
  }

  /**
   * Check if a tool is allowed for an agent type
   */
  isToolAllowed(agentTypeId: string, toolName: string): boolean {
    const config = this.get(agentTypeId) || this.getDefault();

    // Check if explicitly denied
    if (config.tools.denied?.includes(toolName)) {
      return false;
    }

    // Check if explicitly allowed
    return config.tools.allowed.includes(toolName);
  }

  /**
   * Get MCP servers for an agent type
   */
  getMCPServers(id: string): MCPServerConfig[] {
    const config = this.get(id);
    return config?.mcpServers || [];
  }

  /**
   * Get max turns for an agent type
   */
  getMaxTurns(id: string): number | undefined {
    const config = this.get(id);
    return config?.maxTurns;
  }

  /**
   * Get constraints for an agent type
   */
  getConstraints(id: string): string[] {
    const config = this.get(id);
    return config?.constraints || [];
  }

  /**
   * Validate an agent type configuration
   */
  private validateConfig(config: AgentTypeConfig): void {
    if (!config.id || typeof config.id !== 'string') {
      throw new Error('Agent type must have a valid ID');
    }

    if (!config.name || typeof config.name !== 'string') {
      throw new Error('Agent type must have a valid name');
    }

    if (!config.tools || !Array.isArray(config.tools.allowed)) {
      throw new Error('Agent type must have a valid tools configuration');
    }

    if (config.tools.allowed.length === 0) {
      throw new Error('Agent type must have at least one allowed tool');
    }
  }

  /**
   * Create a modified agent type with additional constraints
   */
  createConstrained(
    baseTypeId: string,
    constraints: string[],
    newId?: string
  ): AgentTypeConfig {
    const baseConfig = this.get(baseTypeId);
    if (!baseConfig) {
      throw new Error(`Agent type ${baseTypeId} not found`);
    }

    return {
      ...baseConfig,
      id: newId || `${baseConfig.id}-constrained`,
      constraints: [...(baseConfig.constraints || []), ...constraints],
    };
  }

  /**
   * Create a modified agent type with restricted tools
   */
  createRestricted(
    baseTypeId: string,
    deniedTools: string[],
    newId?: string
  ): AgentTypeConfig {
    const baseConfig = this.get(baseTypeId);
    if (!baseConfig) {
      throw new Error(`Agent type ${baseTypeId} not found`);
    }

    return {
      ...baseConfig,
      id: newId || `${baseConfig.id}-restricted`,
      tools: {
        allowed: baseConfig.tools.allowed.filter((t) => !deniedTools.includes(t)),
        denied: [...(baseConfig.tools.denied || []), ...deniedTools],
      },
    };
  }
}

// Singleton instance
let instance: AgentRegistry | null = null;

export function getAgentRegistry(): AgentRegistry {
  if (!instance) {
    instance = new AgentRegistry();
  }
  return instance;
}
