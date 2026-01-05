/**
 * Dev Server Routes
 *
 * API endpoints for dev server management.
 */

import type { FastifyInstance, FastifyRequest, FastifyReply } from 'fastify';
import { getDevServerManager, type DevServerConfig } from '../services/dev-server.js';

interface StartDevServerBody {
  workspaceId: string;
  workDir: string;
  command: string;
  args: string[];
  env?: Record<string, string>;
  port?: number;
}

export async function devServerRoutes(fastify: FastifyInstance): Promise<void> {
  const devServerManager = getDevServerManager();

  /**
   * Start a dev server
   */
  fastify.post<{ Body: StartDevServerBody }>(
    '/dev-servers',
    async (request: FastifyRequest<{ Body: StartDevServerBody }>, reply: FastifyReply) => {
      const { workspaceId, workDir, command, args, env, port } = request.body;

      if (!workspaceId || !workDir || !command) {
        return reply.status(400).send({
          error: 'Missing required fields: workspaceId, workDir, command',
        });
      }

      const config: DevServerConfig = {
        workDir,
        command,
        args: args || [],
        env,
        port,
      };

      try {
        const instance = await devServerManager.start(workspaceId, config);
        return reply.status(201).send(instance);
      } catch (error) {
        const message = error instanceof Error ? error.message : 'Unknown error';
        return reply.status(500).send({ error: message });
      }
    }
  );

  /**
   * List all dev servers
   */
  fastify.get('/dev-servers', async (_request: FastifyRequest, reply: FastifyReply) => {
    const instances = devServerManager.list();
    return reply.send({ instances });
  });

  /**
   * List running dev servers
   */
  fastify.get('/dev-servers/running', async (_request: FastifyRequest, reply: FastifyReply) => {
    const instances = devServerManager.listRunning();
    return reply.send({ instances });
  });

  /**
   * Get a dev server by ID
   */
  fastify.get<{ Params: { id: string } }>(
    '/dev-servers/:id',
    async (request: FastifyRequest<{ Params: { id: string } }>, reply: FastifyReply) => {
      const instance = devServerManager.get(request.params.id);

      if (!instance) {
        return reply.status(404).send({ error: 'Dev server not found' });
      }

      return reply.send(instance);
    }
  );

  /**
   * Get dev server for a workspace
   */
  fastify.get<{ Params: { workspaceId: string } }>(
    '/dev-servers/workspace/:workspaceId',
    async (
      request: FastifyRequest<{ Params: { workspaceId: string } }>,
      reply: FastifyReply
    ) => {
      const instance = devServerManager.getForWorkspace(request.params.workspaceId);

      if (!instance) {
        return reply.status(404).send({ error: 'No dev server running for this workspace' });
      }

      return reply.send(instance);
    }
  );

  /**
   * Get output for a dev server
   */
  fastify.get<{ Params: { id: string } }>(
    '/dev-servers/:id/output',
    async (request: FastifyRequest<{ Params: { id: string } }>, reply: FastifyReply) => {
      const instance = devServerManager.get(request.params.id);

      if (!instance) {
        return reply.status(404).send({ error: 'Dev server not found' });
      }

      return reply.send({ output: instance.output });
    }
  );

  /**
   * Stop a dev server
   */
  fastify.post<{ Params: { id: string } }>(
    '/dev-servers/:id/stop',
    async (request: FastifyRequest<{ Params: { id: string } }>, reply: FastifyReply) => {
      const success = await devServerManager.stop(request.params.id);

      if (!success) {
        return reply.status(404).send({ error: 'Dev server not found' });
      }

      return reply.send({ success: true });
    }
  );

  /**
   * Stop dev server for a workspace
   */
  fastify.post<{ Params: { workspaceId: string } }>(
    '/dev-servers/workspace/:workspaceId/stop',
    async (
      request: FastifyRequest<{ Params: { workspaceId: string } }>,
      reply: FastifyReply
    ) => {
      const success = await devServerManager.stopForWorkspace(request.params.workspaceId);

      if (!success) {
        return reply.status(404).send({ error: 'No dev server running for this workspace' });
      }

      return reply.send({ success: true });
    }
  );
}
