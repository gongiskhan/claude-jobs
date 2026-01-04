"""Main entry point for the agent sidecar."""

import argparse
import asyncio
import logging
import os
import sys

from .server import run_server

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    handlers=[logging.StreamHandler(sys.stderr)],
)

logger = logging.getLogger(__name__)


def main() -> None:
    """Main entry point."""
    parser = argparse.ArgumentParser(
        description="Claude Agent SDK sidecar for claude-jobs"
    )
    parser.add_argument(
        "--socket",
        default=os.environ.get("AGENT_SIDECAR_SOCKET", "/tmp/agent-sidecar.sock"),
        help="Path to Unix socket for IPC",
    )
    parser.add_argument(
        "--workspace-id",
        default=os.environ.get("WORKSPACE_ID"),
        help="Workspace ID (for per-workspace sidecars)",
    )
    parser.add_argument(
        "--verbose",
        "-v",
        action="store_true",
        help="Enable verbose logging",
    )

    args = parser.parse_args()

    if args.verbose:
        logging.getLogger().setLevel(logging.DEBUG)

    # If workspace ID is provided, use a workspace-specific socket
    socket_path = args.socket
    if args.workspace_id:
        socket_dir = os.path.dirname(socket_path)
        socket_path = os.path.join(socket_dir, f"agent-sidecar-{args.workspace_id}.sock")

    logger.info(f"Starting agent sidecar with socket: {socket_path}")

    # Use uvloop if available for better performance
    try:
        import uvloop

        asyncio.set_event_loop_policy(uvloop.EventLoopPolicy())
        logger.info("Using uvloop for event loop")
    except ImportError:
        logger.info("uvloop not available, using default event loop")

    try:
        asyncio.run(run_server(socket_path))
    except KeyboardInterrupt:
        logger.info("Shutting down...")
    except Exception:
        logger.exception("Fatal error in agent sidecar")
        sys.exit(1)


if __name__ == "__main__":
    main()
