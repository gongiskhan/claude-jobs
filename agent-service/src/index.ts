/**
 * Agent Service Entry Point
 *
 * This is the main entry point for the Agent Service, which provides
 * interactive Claude agent sessions using the Anthropic Agent SDK.
 */

import { buildApp } from './app.js';
import { config } from './config/index.js';

async function main(): Promise<void> {
  const app = await buildApp();

  try {
    await app.listen({ port: config.port, host: config.host });
    console.log(`Agent Service running at http://${config.host}:${config.port}`);

    // Handle graceful shutdown
    const shutdown = async () => {
      console.log('Shutting down...');
      await app.close();
      process.exit(0);
    };

    process.on('SIGTERM', shutdown);
    process.on('SIGINT', shutdown);
  } catch (err) {
    app.log.error(err);
    process.exit(1);
  }
}

main();
