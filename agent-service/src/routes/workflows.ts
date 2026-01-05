/**
 * Workflow Routes
 *
 * API endpoints for orchestrated workflow execution.
 */

import type { FastifyInstance, FastifyRequest, FastifyReply } from 'fastify';
import { getOrchestrationService, type StartWorkflowRequest } from '../services/orchestration.js';
import { getAgentRegistry } from '../services/agent-registry.js';
import { WORKFLOWS, getWorkflow } from '../types/orchestration.js';

export async function workflowRoutes(fastify: FastifyInstance): Promise<void> {
  const orchestrationService = getOrchestrationService();
  const agentRegistry = getAgentRegistry();

  /**
   * List available workflows
   */
  fastify.get('/workflows', async (_request: FastifyRequest, reply: FastifyReply) => {
    const workflows = Object.values(WORKFLOWS).map((w) => ({
      id: w.id,
      name: w.name,
      description: w.description,
      phases: w.phases.map((p) => ({
        id: p.id,
        name: p.name,
        agentType: p.agentType,
        optional: p.optional,
      })),
    }));

    return reply.send({ workflows });
  });

  /**
   * Get a specific workflow
   */
  fastify.get<{ Params: { id: string } }>(
    '/workflows/:id',
    async (request: FastifyRequest<{ Params: { id: string } }>, reply: FastifyReply) => {
      const workflow = getWorkflow(request.params.id);

      if (!workflow) {
        return reply.status(404).send({ error: 'Workflow not found' });
      }

      return reply.send({
        id: workflow.id,
        name: workflow.name,
        description: workflow.description,
        phases: workflow.phases.map((p) => ({
          id: p.id,
          name: p.name,
          agentType: p.agentType,
          optional: p.optional,
        })),
        autofixConfig: workflow.autofixConfig,
      });
    }
  );

  /**
   * Start a workflow execution
   */
  fastify.post<{ Body: StartWorkflowRequest }>(
    '/workflows/execute',
    async (request: FastifyRequest<{ Body: StartWorkflowRequest }>, reply: FastifyReply) => {
      const { workspaceId, workspacePath, prompt, workflowId, sessionId, metadata } = request.body;

      if (!workspaceId || !workspacePath || !prompt) {
        return reply.status(400).send({
          error: 'Missing required fields: workspaceId, workspacePath, prompt',
        });
      }

      // Verify workflow exists if specified
      if (workflowId && !getWorkflow(workflowId)) {
        return reply.status(400).send({ error: `Unknown workflow: ${workflowId}` });
      }

      try {
        const result = await orchestrationService.executeWorkflow({
          workspaceId,
          workspacePath,
          prompt,
          workflowId,
          sessionId,
          metadata,
        });

        return reply.send(result);
      } catch (error) {
        const message = error instanceof Error ? error.message : 'Unknown error';
        return reply.status(500).send({ error: message });
      }
    }
  );

  /**
   * List available agent types
   */
  fastify.get('/agent-types', async (_request: FastifyRequest, reply: FastifyReply) => {
    const agentTypes = agentRegistry.list().map((a) => ({
      id: a.id,
      name: a.name,
      description: a.description,
      tools: a.tools,
      maxTurns: a.maxTurns,
      constraints: a.constraints,
    }));

    return reply.send({ agentTypes });
  });

  /**
   * Get a specific agent type
   */
  fastify.get<{ Params: { id: string } }>(
    '/agent-types/:id',
    async (request: FastifyRequest<{ Params: { id: string } }>, reply: FastifyReply) => {
      const agentType = agentRegistry.get(request.params.id);

      if (!agentType) {
        return reply.status(404).send({ error: 'Agent type not found' });
      }

      return reply.send({
        id: agentType.id,
        name: agentType.name,
        description: agentType.description,
        tools: agentType.tools,
        mcpServers: agentType.mcpServers,
        maxTurns: agentType.maxTurns,
        constraints: agentType.constraints,
      });
    }
  );
}
