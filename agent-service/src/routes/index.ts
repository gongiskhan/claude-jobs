/**
 * Route Registration
 *
 * Registers all API routes for the agent service.
 */

import { type FastifyInstance } from 'fastify';
import { registerJobRoutes } from './jobs.js';
import { registerStreamRoutes } from './stream.js';
import { workflowRoutes } from './workflows.js';
import { devServerRoutes } from './dev-server.js';

export async function registerRoutes(fastify: FastifyInstance): Promise<void> {
  // Health check endpoint
  fastify.get('/health', async () => {
    return { status: 'ok', service: 'agent-service' };
  });

  // Register job management routes
  await fastify.register(registerJobRoutes, { prefix: '/jobs' });

  // Register SSE streaming routes
  await fastify.register(registerStreamRoutes, { prefix: '/jobs' });

  // Register workflow and agent type routes
  await fastify.register(workflowRoutes);

  // Register dev server routes
  await fastify.register(devServerRoutes);
}
