/**
 * SSE Streaming Routes
 *
 * GET /jobs/:id/stream - Server-Sent Events stream for job updates
 */

import { type FastifyInstance, type FastifyRequest, type FastifyReply } from 'fastify';
import { getStreamManager } from '../services/stream-manager.js';

export async function registerStreamRoutes(fastify: FastifyInstance): Promise<void> {
  // SSE stream for job events
  fastify.get<{ Params: { id: string } }>(
    '/:id/stream',
    async (request: FastifyRequest<{ Params: { id: string } }>, reply: FastifyReply) => {
      const streamManager = getStreamManager();
      const jobId = request.params.id;

      // Set SSE headers
      reply.raw.setHeader('Content-Type', 'text/event-stream');
      reply.raw.setHeader('Cache-Control', 'no-cache');
      reply.raw.setHeader('Connection', 'keep-alive');
      reply.raw.setHeader('Access-Control-Allow-Origin', '*');

      // Register this connection with the stream manager
      streamManager.addConnection(jobId, reply);

      // Handle client disconnect
      request.raw.on('close', () => {
        streamManager.removeConnection(jobId, reply);
      });

      // Keep the connection open - events will be sent by the stream manager
      // Don't call reply.send() - the SSE connection stays open
    }
  );
}
