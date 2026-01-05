/**
 * Agent Service Configuration
 */

import 'dotenv/config';

export interface Config {
  port: number;
  host: string;
  logLevel: string;
  maxConcurrentJobs: number;
  jobTimeoutMs: number;
}

export function loadConfig(): Config {
  return {
    port: parseInt(process.env.AGENT_SERVICE_PORT || '3001', 10),
    host: process.env.AGENT_SERVICE_HOST || '127.0.0.1',
    logLevel: process.env.LOG_LEVEL || 'info',
    maxConcurrentJobs: parseInt(process.env.MAX_CONCURRENT_JOBS || '10', 10),
    jobTimeoutMs: parseInt(process.env.JOB_TIMEOUT_MS || '600000', 10), // 10 minutes default
  };
}

export const config = loadConfig();
