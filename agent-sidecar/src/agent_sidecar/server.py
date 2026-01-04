"""JSON-RPC server for communication with Rust backend."""

import asyncio
import json
import logging
import os
import signal
from dataclasses import asdict
from pathlib import Path
from typing import Any, Optional
from uuid import UUID

from .session_manager import AgentSessionManager
from .types import (
    AgentConfig,
    AgentEvent,
    AgentType,
)

logger = logging.getLogger(__name__)


def serialize_event(event: AgentEvent) -> dict[str, Any]:
    """Serialize an event to JSON-compatible dict."""
    result = asdict(event)
    # Convert UUIDs to strings
    for key, value in result.items():
        if isinstance(value, UUID):
            result[key] = str(value)
        elif hasattr(value, "value"):  # Enum
            result[key] = value.value
    result["event_type"] = type(event).__name__
    return result


class JsonRpcServer:
    """JSON-RPC server over Unix socket for Rust backend communication."""

    def __init__(self, socket_path: str):
        self.socket_path = socket_path
        self.session_manager = AgentSessionManager()
        self._server: Optional[asyncio.AbstractServer] = None
        self._clients: dict[int, asyncio.StreamWriter] = {}
        self._client_id = 0
        self._running = False

    async def start(self) -> None:
        """Start the JSON-RPC server."""
        # Remove existing socket file
        socket_path = Path(self.socket_path)
        if socket_path.exists():
            socket_path.unlink()

        # Ensure parent directory exists
        socket_path.parent.mkdir(parents=True, exist_ok=True)

        self._server = await asyncio.start_unix_server(
            self._handle_client,
            path=self.socket_path,
        )

        # Set socket permissions
        os.chmod(self.socket_path, 0o660)

        self._running = True
        logger.info(f"JSON-RPC server listening on {self.socket_path}")

        async with self._server:
            await self._server.serve_forever()

    async def stop(self) -> None:
        """Stop the server."""
        self._running = False

        # Close all client connections
        for writer in self._clients.values():
            writer.close()
            await writer.wait_closed()
        self._clients.clear()

        # Terminate all sessions
        await self.session_manager.terminate_all()

        # Stop server
        if self._server:
            self._server.close()
            await self._server.wait_closed()

        # Remove socket file
        socket_path = Path(self.socket_path)
        if socket_path.exists():
            socket_path.unlink()

        logger.info("JSON-RPC server stopped")

    async def _handle_client(
        self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter
    ) -> None:
        """Handle a client connection."""
        self._client_id += 1
        client_id = self._client_id
        self._clients[client_id] = writer

        logger.info(f"Client {client_id} connected")

        try:
            while self._running:
                # Read a line (JSON-RPC message)
                line = await reader.readline()
                if not line:
                    break

                try:
                    message = json.loads(line.decode("utf-8"))
                    response = await self._handle_request(client_id, message)
                    if response:
                        await self._send_response(writer, response)
                except json.JSONDecodeError as e:
                    logger.error(f"Invalid JSON from client {client_id}: {e}")
                    await self._send_error(writer, None, -32700, "Parse error")
                except Exception as e:
                    logger.exception(f"Error handling request from client {client_id}")
                    msg_id = message.get("id") if isinstance(message, dict) else None
                    await self._send_error(writer, msg_id, -32603, str(e))

        except asyncio.CancelledError:
            pass
        except Exception:
            logger.exception(f"Error in client {client_id} handler")
        finally:
            self._clients.pop(client_id, None)
            writer.close()
            await writer.wait_closed()
            logger.info(f"Client {client_id} disconnected")

    async def _handle_request(
        self, client_id: int, message: dict[str, Any]
    ) -> Optional[dict[str, Any]]:
        """Handle a JSON-RPC request."""
        method = message.get("method")
        params = message.get("params", {})
        msg_id = message.get("id")

        logger.debug(f"Client {client_id} request: {method}")

        # Dispatch to handler
        handler = getattr(self, f"_rpc_{method}", None)
        if handler is None:
            return {
                "jsonrpc": "2.0",
                "id": msg_id,
                "error": {"code": -32601, "message": f"Method not found: {method}"},
            }

        try:
            result = await handler(client_id, params)
            return {"jsonrpc": "2.0", "id": msg_id, "result": result}
        except Exception as e:
            logger.exception(f"Error in RPC handler {method}")
            return {
                "jsonrpc": "2.0",
                "id": msg_id,
                "error": {"code": -32603, "message": str(e)},
            }

    async def _send_response(
        self, writer: asyncio.StreamWriter, response: dict[str, Any]
    ) -> None:
        """Send a JSON-RPC response."""
        data = json.dumps(response) + "\n"
        writer.write(data.encode("utf-8"))
        await writer.drain()

    async def _send_error(
        self, writer: asyncio.StreamWriter, msg_id: Any, code: int, message: str
    ) -> None:
        """Send a JSON-RPC error response."""
        response = {
            "jsonrpc": "2.0",
            "id": msg_id,
            "error": {"code": code, "message": message},
        }
        await self._send_response(writer, response)

    async def _broadcast_event(self, event: AgentEvent) -> None:
        """Broadcast an event to all connected clients."""
        message = {
            "jsonrpc": "2.0",
            "method": "event",
            "params": serialize_event(event),
        }
        data = json.dumps(message) + "\n"
        encoded = data.encode("utf-8")

        for client_id, writer in list(self._clients.items()):
            try:
                writer.write(encoded)
                await writer.drain()
            except Exception:
                logger.warning(f"Failed to send event to client {client_id}")

    # RPC Handlers

    async def _rpc_create_session(
        self, client_id: int, params: dict[str, Any]
    ) -> dict[str, Any]:
        """Create a new agent session."""
        session_id = UUID(params["session_id"])
        agent_type = AgentType(params["agent_type"])
        config = AgentConfig(
            working_dir=params["config"]["working_dir"],
            system_prompt=params["config"].get("system_prompt"),
            max_context_tokens=params["config"].get("max_context_tokens", 200000),
            auto_approve_read_tools=params["config"].get("auto_approve_read_tools", True),
            allowed_tools=params["config"].get("allowed_tools"),
            env_vars=params["config"].get("env_vars", {}),
        )

        session = await self.session_manager.create_session(
            session_id=session_id,
            agent_type=agent_type,
            config=config,
            event_callback=self._broadcast_event,
        )

        return {
            "session_id": str(session_id),
            "state": session.state.value,
        }

    async def _rpc_query(self, client_id: int, params: dict[str, Any]) -> dict[str, Any]:
        """Send a query to an agent session."""
        session_id = UUID(params["session_id"])
        prompt = params["prompt"]

        session = self.session_manager.get_session(session_id)
        if not session:
            raise ValueError(f"Session not found: {session_id}")

        # Start query in background task
        asyncio.create_task(session.query(prompt))

        return {"status": "started"}

    async def _rpc_resume_with_response(
        self, client_id: int, params: dict[str, Any]
    ) -> dict[str, Any]:
        """Resume session with user response."""
        session_id = UUID(params["session_id"])
        input_id = UUID(params["input_id"])
        response = params["response"]

        session = self.session_manager.get_session(session_id)
        if not session:
            raise ValueError(f"Session not found: {session_id}")

        await session.resume_with_response(input_id, response)
        return {"status": "resumed"}

    async def _rpc_approve_tool(
        self, client_id: int, params: dict[str, Any]
    ) -> dict[str, Any]:
        """Approve or deny tool use."""
        session_id = UUID(params["session_id"])
        input_id = UUID(params["input_id"])
        tool_call_id = params["tool_call_id"]
        approved = params["approved"]
        reason = params.get("reason")

        session = self.session_manager.get_session(session_id)
        if not session:
            raise ValueError(f"Session not found: {session_id}")

        await session.approve_tool(input_id, tool_call_id, approved, reason)
        return {"status": "processed"}

    async def _rpc_interrupt(
        self, client_id: int, params: dict[str, Any]
    ) -> dict[str, Any]:
        """Interrupt current execution."""
        session_id = UUID(params["session_id"])

        session = self.session_manager.get_session(session_id)
        if not session:
            raise ValueError(f"Session not found: {session_id}")

        await session.interrupt()
        return {"status": "interrupted"}

    async def _rpc_pause(self, client_id: int, params: dict[str, Any]) -> dict[str, Any]:
        """Pause execution."""
        session_id = UUID(params["session_id"])

        session = self.session_manager.get_session(session_id)
        if not session:
            raise ValueError(f"Session not found: {session_id}")

        await session.pause()
        return {"status": "paused"}

    async def _rpc_resume(
        self, client_id: int, params: dict[str, Any]
    ) -> dict[str, Any]:
        """Resume paused execution."""
        session_id = UUID(params["session_id"])

        session = self.session_manager.get_session(session_id)
        if not session:
            raise ValueError(f"Session not found: {session_id}")

        await session.resume()
        return {"status": "resumed"}

    async def _rpc_terminate(
        self, client_id: int, params: dict[str, Any]
    ) -> dict[str, Any]:
        """Terminate a session."""
        session_id = UUID(params["session_id"])

        await self.session_manager.remove_session(session_id)
        return {"status": "terminated"}

    async def _rpc_get_context(
        self, client_id: int, params: dict[str, Any]
    ) -> dict[str, Any]:
        """Get serialized context for persistence."""
        session_id = UUID(params["session_id"])

        session = self.session_manager.get_session(session_id)
        if not session:
            raise ValueError(f"Session not found: {session_id}")

        context = session.get_context()
        return {
            "messages": context.messages,
            "sdk_session_id": context.sdk_session_id,
            "metadata": context.metadata,
        }

    async def _rpc_list_sessions(
        self, client_id: int, params: dict[str, Any]
    ) -> dict[str, Any]:
        """List all sessions."""
        sessions = self.session_manager.list_sessions()
        return {
            "sessions": [
                {"session_id": str(sid), "state": state.value}
                for sid, state in sessions
            ]
        }

    async def _rpc_ping(self, client_id: int, params: dict[str, Any]) -> dict[str, Any]:
        """Health check."""
        return {"status": "ok"}


async def run_server(socket_path: str) -> None:
    """Run the JSON-RPC server."""
    server = JsonRpcServer(socket_path)

    # Handle shutdown signals
    loop = asyncio.get_running_loop()

    def handle_shutdown() -> None:
        asyncio.create_task(server.stop())

    for sig in (signal.SIGTERM, signal.SIGINT):
        loop.add_signal_handler(sig, handle_shutdown)

    try:
        await server.start()
    except asyncio.CancelledError:
        pass
    finally:
        await server.stop()
