/**
 * Agent Service Fastify Application
 *
 * This service uses the Anthropic Agent SDK to run Claude agents
 * as interactive sessions, rather than spawning CLI processes.
 */

import Fastify, { type FastifyInstance } from 'fastify';
import cors from '@fastify/cors';
import { config } from './config/index.js';
import { registerRoutes } from './routes/index.js';
import { getJobManager } from './services/job-manager.js';

export async function buildApp(): Promise<FastifyInstance> {
  const fastify = Fastify({
    logger: {
      level: config.logLevel,
      transport:
        process.env.NODE_ENV !== 'production'
          ? {
              target: 'pino-pretty',
              options: {
                translateTime: 'HH:MM:ss Z',
                ignore: 'pid,hostname',
              },
            }
          : undefined,
    },
  });

  // Allow empty JSON bodies for certain requests
  fastify.addContentTypeParser(
    'application/json',
    { parseAs: 'string' },
    (req, body, done) => {
      if (!body || (typeof body === 'string' && body.trim() === '')) {
        done(null, null);
        return;
      }
      try {
        const json = JSON.parse(body as string);
        done(null, json);
      } catch (err) {
        done(err as Error, undefined);
      }
    }
  );

  // Initialize job manager
  getJobManager();

  // Register CORS plugin
  // Since Rust backend is the gateway, we only accept requests from localhost
  await fastify.register(cors, {
    origin: true, // Allow all origins (Rust backend handles external auth)
    methods: ['GET', 'POST', 'PUT', 'DELETE', 'OPTIONS'],
  });

  // Register routes
  await registerRoutes(fastify);

  // Register shutdown hook
  fastify.addHook('onClose', async () => {
    fastify.log.info('Shutting down agent service...');
    const jobManager = getJobManager();
    await jobManager.shutdown();
  });

  return fastify;
}
