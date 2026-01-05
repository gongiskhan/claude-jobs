/**
 * Dev Server Service
 *
 * Manages development server processes for workspaces.
 * Tracks running processes, ports, and provides lifecycle management.
 */

import { spawn, type ChildProcess } from 'child_process';
import { EventEmitter } from 'events';
import { v4 as uuidv4 } from 'uuid';

/**
 * Dev server configuration
 */
export interface DevServerConfig {
  /** Working directory for the dev server */
  workDir: string;
  /** Command to run (e.g., 'npm', 'pnpm', 'yarn') */
  command: string;
  /** Arguments (e.g., ['run', 'dev']) */
  args: string[];
  /** Environment variables */
  env?: Record<string, string>;
  /** Port to use (if known in advance) */
  port?: number;
}

/**
 * Dev server instance
 */
export interface DevServerInstance {
  /** Unique identifier */
  id: string;
  /** Workspace ID */
  workspaceId: string;
  /** Configuration used to start the server */
  config: DevServerConfig;
  /** Process ID */
  pid?: number;
  /** Detected port */
  port?: number;
  /** Server URL */
  url?: string;
  /** Status */
  status: 'starting' | 'running' | 'stopped' | 'failed';
  /** Error message if failed */
  error?: string;
  /** Start time */
  startedAt: Date;
  /** Stop time */
  stoppedAt?: Date;
  /** Output log */
  output: string[];
}

/**
 * Dev Server Manager
 *
 * Manages dev server processes with automatic port detection.
 */
export class DevServerManager extends EventEmitter {
  private instances: Map<string, DevServerInstance> = new Map();
  private processes: Map<string, ChildProcess> = new Map();
  private workspaceToInstance: Map<string, string> = new Map();

  // Patterns to detect server URLs in output
  private readonly urlPatterns = [
    /https?:\/\/localhost:\d+/g,
    /https?:\/\/127\.0\.0\.1:\d+/g,
    /https?:\/\/0\.0\.0\.0:\d+/g,
    /Local:\s+(https?:\/\/[^\s]+)/,
    /Network:\s+(https?:\/\/[^\s]+)/,
    /listening on port (\d+)/i,
    /server running at (https?:\/\/[^\s]+)/i,
    /started server on (https?:\/\/[^\s]+)/i,
  ];

  /**
   * Start a dev server for a workspace
   */
  async start(workspaceId: string, config: DevServerConfig): Promise<DevServerInstance> {
    // Check if already running for this workspace
    const existingId = this.workspaceToInstance.get(workspaceId);
    if (existingId) {
      const existing = this.instances.get(existingId);
      if (existing && (existing.status === 'starting' || existing.status === 'running')) {
        return existing;
      }
    }

    const instance: DevServerInstance = {
      id: uuidv4(),
      workspaceId,
      config,
      status: 'starting',
      startedAt: new Date(),
      output: [],
    };

    this.instances.set(instance.id, instance);
    this.workspaceToInstance.set(workspaceId, instance.id);

    try {
      const childProcess = spawn(config.command, config.args, {
        cwd: config.workDir,
        env: { ...process.env, ...config.env },
        shell: true,
        stdio: ['ignore', 'pipe', 'pipe'],
      });

      this.processes.set(instance.id, childProcess);
      instance.pid = childProcess.pid;

      // Capture stdout
      childProcess.stdout?.on('data', (data: Buffer) => {
        const output = data.toString();
        instance.output.push(output);

        // Detect URL/port
        this.detectServerUrl(instance, output);

        // Emit output event
        this.emit('output', { instanceId: instance.id, output });
      });

      // Capture stderr
      childProcess.stderr?.on('data', (data: Buffer) => {
        const output = data.toString();
        instance.output.push(output);

        // Also check stderr for URLs (some servers log there)
        this.detectServerUrl(instance, output);

        this.emit('output', { instanceId: instance.id, output, isError: true });
      });

      // Handle process exit
      childProcess.on('exit', (code: number | null, signal: NodeJS.Signals | null) => {
        instance.status = code === 0 ? 'stopped' : 'failed';
        instance.stoppedAt = new Date();
        if (code !== 0) {
          instance.error = `Process exited with code ${code}${signal ? ` (${signal})` : ''}`;
        }

        this.processes.delete(instance.id);
        this.emit('stopped', { instanceId: instance.id, code, signal });
      });

      // Handle process error
      childProcess.on('error', (error: Error) => {
        instance.status = 'failed';
        instance.error = error.message;
        this.processes.delete(instance.id);
        this.emit('error', { instanceId: instance.id, error });
      });

      // Wait briefly for startup
      await new Promise((resolve) => setTimeout(resolve, 2000));

      if (instance.status === 'starting') {
        instance.status = 'running';
      }

      return instance;
    } catch (error) {
      instance.status = 'failed';
      instance.error = error instanceof Error ? error.message : 'Unknown error';
      throw error;
    }
  }

  /**
   * Stop a dev server
   */
  async stop(instanceId: string): Promise<boolean> {
    const instance = this.instances.get(instanceId);
    if (!instance) {
      return false;
    }

    const childProcess = this.processes.get(instanceId);
    if (childProcess) {
      // Try graceful shutdown first
      childProcess.kill('SIGTERM');

      // Wait for graceful shutdown
      await new Promise((resolve) => setTimeout(resolve, 3000));

      // Force kill if still running
      if (!childProcess.killed) {
        childProcess.kill('SIGKILL');
      }
    }

    instance.status = 'stopped';
    instance.stoppedAt = new Date();
    this.processes.delete(instanceId);
    this.workspaceToInstance.delete(instance.workspaceId);

    return true;
  }

  /**
   * Stop dev server for a workspace
   */
  async stopForWorkspace(workspaceId: string): Promise<boolean> {
    const instanceId = this.workspaceToInstance.get(workspaceId);
    if (!instanceId) {
      return false;
    }
    return this.stop(instanceId);
  }

  /**
   * Get dev server instance by ID
   */
  get(instanceId: string): DevServerInstance | undefined {
    return this.instances.get(instanceId);
  }

  /**
   * Get dev server for a workspace
   */
  getForWorkspace(workspaceId: string): DevServerInstance | undefined {
    const instanceId = this.workspaceToInstance.get(workspaceId);
    if (!instanceId) {
      return undefined;
    }
    return this.instances.get(instanceId);
  }

  /**
   * List all dev server instances
   */
  list(): DevServerInstance[] {
    return Array.from(this.instances.values());
  }

  /**
   * List running dev servers
   */
  listRunning(): DevServerInstance[] {
    return this.list().filter((i) => i.status === 'running' || i.status === 'starting');
  }

  /**
   * Get output for a dev server
   */
  getOutput(instanceId: string): string[] {
    return this.instances.get(instanceId)?.output || [];
  }

  /**
   * Detect server URL from output
   */
  private detectServerUrl(instance: DevServerInstance, output: string): void {
    // Already detected
    if (instance.url) {
      return;
    }

    for (const pattern of this.urlPatterns) {
      const match = output.match(pattern);
      if (match) {
        const url = match[1] || match[0];
        if (url.startsWith('http')) {
          instance.url = url;

          // Extract port from URL
          const portMatch = url.match(/:(\d+)/);
          if (portMatch) {
            instance.port = parseInt(portMatch[1], 10);
          }

          this.emit('url-detected', { instanceId: instance.id, url });
          break;
        }
      }
    }
  }

  /**
   * Shutdown all dev servers
   */
  async shutdown(): Promise<void> {
    const promises = Array.from(this.instances.keys()).map((id) => this.stop(id));
    await Promise.all(promises);
  }
}

// Singleton instance
let instance: DevServerManager | null = null;

export function getDevServerManager(): DevServerManager {
  if (!instance) {
    instance = new DevServerManager();
  }
  return instance;
}
