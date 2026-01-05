/**
 * Job Management Routes
 *
 * POST   /jobs              - Start new job
 * GET    /jobs/:id          - Get job status
 * POST   /jobs/:id/resume   - Resume paused job with user input
 * POST   /jobs/:id/cancel   - Cancel running job
 */

import { type FastifyInstance, type FastifyRequest, type FastifyReply } from 'fastify';
import { getJobManager } from '../services/job-manager.js';
import { type StartJobRequest, type ResumeJobRequest } from '../types/job.js';

export async function registerJobRoutes(fastify: FastifyInstance): Promise<void> {
  // Start a new job
  fastify.post<{ Body: StartJobRequest }>(
    '/',
    async (request: FastifyRequest<{ Body: StartJobRequest }>, reply: FastifyReply) => {
      const jobManager = getJobManager();
      const job = await jobManager.startJob(request.body);
      return reply.status(201).send(job);
    }
  );

  // Get job status
  fastify.get<{ Params: { id: string } }>(
    '/:id',
    async (request: FastifyRequest<{ Params: { id: string } }>, reply: FastifyReply) => {
      const jobManager = getJobManager();
      const job = jobManager.getJob(request.params.id);
      if (!job) {
        return reply.status(404).send({ error: 'Job not found' });
      }
      return job;
    }
  );

  // Resume a paused job with user input
  fastify.post<{ Params: { id: string }; Body: ResumeJobRequest }>(
    '/:id/resume',
    async (
      request: FastifyRequest<{ Params: { id: string }; Body: ResumeJobRequest }>,
      reply: FastifyReply
    ) => {
      const jobManager = getJobManager();
      const job = await jobManager.resumeJob(request.params.id, request.body);
      if (!job) {
        return reply.status(404).send({ error: 'Job not found' });
      }
      return job;
    }
  );

  // Cancel a running job
  fastify.post<{ Params: { id: string } }>(
    '/:id/cancel',
    async (request: FastifyRequest<{ Params: { id: string } }>, reply: FastifyReply) => {
      const jobManager = getJobManager();
      const success = await jobManager.cancelJob(request.params.id);
      if (!success) {
        return reply.status(404).send({ error: 'Job not found or already completed' });
      }
      return { status: 'cancelled' };
    }
  );
}
