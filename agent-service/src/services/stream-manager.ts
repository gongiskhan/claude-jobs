/**
 * Stream Manager Service
 *
 * Manages SSE connections and event broadcasting for job updates.
 */

import { EventEmitter } from 'events';
import type { FastifyReply } from 'fastify';
import {
  SSEEventTypes,
  type SSEEventType,
  type SSEEventData,
  type SSEStatusEvent,
  type SSEOutputEvent,
  type SSEProgressEvent,
  type SSEResultEvent,
  type SSEErrorEvent,
  type SSEQuestionEvent,
  type SSEUsageEvent,
  type SSESessionIdEvent,
  type SSECompleteEvent,
} from '../types/events.js';

interface StreamConnection {
  jobId: string;
  reply: FastifyReply;
  createdAt: Date;
}

export class StreamManager {
  private connections: Map<string, Set<StreamConnection>> = new Map();
  private jobEmitters: Map<string, EventEmitter> = new Map();

  /**
   * Add a new SSE connection for a job
   */
  addConnection(jobId: string, reply: FastifyReply): void {
    if (!this.connections.has(jobId)) {
      this.connections.set(jobId, new Set());
    }

    const connection: StreamConnection = {
      jobId,
      reply,
      createdAt: new Date(),
    };

    this.connections.get(jobId)!.add(connection);

    // Clean up on client disconnect
    reply.raw.on('close', () => {
      this.removeConnectionInternal(jobId, connection);
    });
  }

  /**
   * Remove a connection (public API for route handler)
   */
  removeConnection(jobId: string, reply: FastifyReply): void {
    const connections = this.connections.get(jobId);
    if (connections) {
      for (const conn of connections) {
        if (conn.reply === reply) {
          connections.delete(conn);
          break;
        }
      }
      if (connections.size === 0) {
        this.connections.delete(jobId);
      }
    }
  }

  /**
   * Remove a connection by reference
   */
  private removeConnectionInternal(jobId: string, connection: StreamConnection): void {
    const connections = this.connections.get(jobId);
    if (connections) {
      connections.delete(connection);
      if (connections.size === 0) {
        this.connections.delete(jobId);
      }
    }
  }

  /**
   * Get connection count for a job
   */
  getConnectionCount(jobId: string): number {
    return this.connections.get(jobId)?.size || 0;
  }

  /**
   * Broadcast an event to all connections for a job
   */
  broadcast(jobId: string, event: SSEEventType, data: SSEEventData): void {
    const connections = this.connections.get(jobId);
    if (!connections || connections.size === 0) return;

    const payload = `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`;

    for (const conn of connections) {
      try {
        conn.reply.raw.write(payload);
      } catch {
        // Connection might be closed, remove it
        this.removeConnectionInternal(jobId, conn);
      }
    }
  }

  /**
   * Send status update
   */
  sendStatus(jobId: string, status: SSEStatusEvent['status']): void {
    const event: SSEStatusEvent = {
      status,
      timestamp: new Date().toISOString(),
    };
    this.broadcast(jobId, SSEEventTypes.STATUS, event);
  }

  /**
   * Send output (text or tool use)
   */
  sendOutput(jobId: string, output: Omit<SSEOutputEvent, 'timestamp'>): void {
    const event: SSEOutputEvent = {
      ...output,
      timestamp: new Date().toISOString(),
    };
    this.broadcast(jobId, SSEEventTypes.OUTPUT, event);
  }

  /**
   * Send progress update
   */
  sendProgress(jobId: string, progress: Omit<SSEProgressEvent, 'timestamp'>): void {
    const event: SSEProgressEvent = {
      ...progress,
      timestamp: new Date().toISOString(),
    };
    this.broadcast(jobId, SSEEventTypes.PROGRESS, event);
  }

  /**
   * Send result
   */
  sendResult(jobId: string, result: Omit<SSEResultEvent, 'timestamp'>): void {
    const event: SSEResultEvent = {
      ...result,
      timestamp: new Date().toISOString(),
    };
    this.broadcast(jobId, SSEEventTypes.RESULT, event);
  }

  /**
   * Send error
   */
  sendError(jobId: string, error: Omit<SSEErrorEvent, 'timestamp'>): void {
    const event: SSEErrorEvent = {
      ...error,
      timestamp: new Date().toISOString(),
    };
    this.broadcast(jobId, SSEEventTypes.ERROR, event);
  }

  /**
   * Send question (agent is asking user for input)
   */
  sendQuestion(jobId: string, question: Omit<SSEQuestionEvent, 'timestamp'>): void {
    const event: SSEQuestionEvent = {
      ...question,
      timestamp: new Date().toISOString(),
    };
    this.broadcast(jobId, SSEEventTypes.QUESTION, event);
  }

  /**
   * Send usage/context information
   */
  sendUsage(jobId: string, usage: Omit<SSEUsageEvent, 'timestamp'>): void {
    const event: SSEUsageEvent = {
      ...usage,
      timestamp: new Date().toISOString(),
    };
    this.broadcast(jobId, SSEEventTypes.USAGE, event);
  }

  /**
   * Send session ID for session persistence/resumption
   */
  sendSessionId(jobId: string, sessionId: string): void {
    const event: SSESessionIdEvent = {
      sessionId,
      timestamp: new Date().toISOString(),
    };
    this.broadcast(jobId, SSEEventTypes.SESSION_ID, event);
  }

  /**
   * Send completion and close connections
   */
  sendComplete(jobId: string, duration: number): void {
    const event: SSECompleteEvent = {
      jobId,
      duration,
      timestamp: new Date().toISOString(),
    };
    this.broadcast(jobId, SSEEventTypes.COMPLETE, event);

    // Close all connections for this job
    const connections = this.connections.get(jobId);
    if (connections) {
      for (const conn of connections) {
        try {
          conn.reply.raw.end();
        } catch {
          // Ignore errors on close
        }
      }
      this.connections.delete(jobId);
    }

    // Clean up emitter
    this.jobEmitters.delete(jobId);
  }

  /**
   * Get or create an event emitter for a job (for internal coordination)
   */
  getJobEmitter(jobId: string): EventEmitter {
    if (!this.jobEmitters.has(jobId)) {
      this.jobEmitters.set(jobId, new EventEmitter());
    }
    return this.jobEmitters.get(jobId)!;
  }

  /**
   * Check if job has active connections
   */
  hasConnections(jobId: string): boolean {
    return (this.connections.get(jobId)?.size || 0) > 0;
  }

  /**
   * Get all active job IDs
   */
  getActiveJobIds(): string[] {
    return Array.from(this.connections.keys());
  }

  /**
   * Clean up stale connections (older than timeout)
   */
  cleanup(maxAgeMs = 3600000): number {
    const cutoff = Date.now() - maxAgeMs;
    let cleaned = 0;

    for (const [jobId, connections] of this.connections) {
      for (const conn of connections) {
        if (conn.createdAt.getTime() < cutoff) {
          try {
            conn.reply.raw.end();
          } catch {
            // Ignore
          }
          connections.delete(conn);
          cleaned++;
        }
      }
      if (connections.size === 0) {
        this.connections.delete(jobId);
      }
    }

    return cleaned;
  }
}

// Singleton instance
let instance: StreamManager | null = null;

export function getStreamManager(): StreamManager {
  if (!instance) {
    instance = new StreamManager();
  }
  return instance;
}
