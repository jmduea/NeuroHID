"""
Bridge Module - IPC Communication with Rust Core

This module provides the Python side of the IPC connection to the Rust core
service. It handles connecting to the TCP localhost socket, sending and
receiving messages, and converting between Python objects and the JSON protocol.

The bridge runs in its own thread/async task to avoid blocking the ML
inference loop. Messages are queued for sending and received messages
are dispatched to the appropriate handlers.
"""

import asyncio
import json
import struct
from dataclasses import dataclass
from typing import Optional, Callable, Any

# Default IPC port — must match neurohid-ipc DEFAULT_IPC_PORT
DEFAULT_IPC_PORT = 47384


@dataclass
class IpcConfig:
    """Configuration for the IPC connection."""

    # TCP host and port for IPC communication
    host: str = "127.0.0.1"
    port: int = DEFAULT_IPC_PORT

    # Connection timeouts
    connect_timeout_sec: float = 5.0
    recv_timeout_sec: float = 0.1

    # Reconnection settings
    auto_reconnect: bool = True
    reconnect_delay_sec: float = 1.0
    max_reconnect_attempts: int = 10


class IpcClient:
    """Client for communicating with the Rust core service.

    This class manages the connection to the Rust service and provides
    methods for sending features and receiving actions. It handles
    connection management, reconnection, and message framing.

    Example usage:
        client = IpcClient(IpcConfig())
        await client.connect()

        # Send features and get action
        action = await client.send_features(features)

        # Send ErrP result
        await client.send_errp_result(errp_result)
    """

    def __init__(self, config: IpcConfig):
        self.config = config
        self.reader: Optional[asyncio.StreamReader] = None
        self.writer: Optional[asyncio.StreamWriter] = None
        self.connected = False
        self.sequence = 0

        # Callbacks for received messages
        self._on_feature_batch: Optional[Callable] = None
        self._on_errp_window: Optional[Callable] = None
        self._on_shutdown: Optional[Callable] = None

    async def connect(self) -> bool:
        """Connect to the Rust core service.

        Returns True if connection succeeded, False otherwise.
        """
        try:
            self.reader, self.writer = await asyncio.wait_for(
                asyncio.open_connection(self.config.host, self.config.port),
                timeout=self.config.connect_timeout_sec
            )

            # Disable Nagle's algorithm for lower latency
            sock = self.writer.get_extra_info("socket")
            if sock is not None:
                import socket
                sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)

            self.connected = True

            # Send ready message
            await self._send_message({"type": "Ready"})

            return True

        except ConnectionRefusedError:
            print(
                f"Connection refused at {self.config.host}:{self.config.port}"
            )
            print("Is the Rust service running?")
            return False
        except asyncio.TimeoutError:
            print("Connection timed out")
            return False
        except Exception as e:
            print(f"Connection failed: {e}")
            return False

    async def disconnect(self):
        """Disconnect from the service."""
        if self.writer:
            self.writer.close()
            await self.writer.wait_closed()

        self.reader = None
        self.writer = None
        self.connected = False

    async def send_action(
        self,
        mouse_dx: float,
        mouse_dy: float,
        discrete_action: int,
        confidence: float,
        inference_latency_us: int
    ):
        """Send a decoded action to the Rust service.

        Args:
            mouse_dx: Horizontal mouse movement
            mouse_dy: Vertical mouse movement
            discrete_action: Index of discrete action (0=no-op, etc.)
            confidence: Confidence in the action (0.0-1.0)
            inference_latency_us: How long inference took in microseconds
        """
        self.sequence += 1

        message = {
            "type": "Action",
            "action": {
                "mouse": {
                    "movement": {"dx": mouse_dx, "dy": mouse_dy},
                    "buttons": [],
                    "scroll": None,
                },
                "keyboard": None,
                "confidence": confidence,
                "timestamp": 0,  # Will be set by Rust
            },
            "sequence": self.sequence,
            "inference_latency_us": inference_latency_us,
        }

        await self._send_message(message)

    async def send_errp_result(
        self,
        error_probability: float,
        confidence: float,
        sequence: int
    ):
        """Send an ErrP detection result to the Rust service.

        Args:
            error_probability: Probability that an error was detected (0.0-1.0)
            confidence: Confidence in the detection
            sequence: Sequence number of the ErrP window this corresponds to
        """
        message = {
            "type": "ErrPResult",
            "result": {
                "error_probability": error_probability,
                "classification_confidence": confidence,
                "signal_quality": "Good",  # Simplified
                "magnitude": None,
            },
            "sequence": sequence,
        }

        await self._send_message(message)

    async def receive_message(self) -> Optional[dict]:
        """Receive a message from the Rust service.

        Returns the message as a dictionary, or None if no message available.
        """
        if not self.connected or not self.reader:
            return None

        try:
            # Read length prefix (4 bytes, little-endian)
            length_bytes = await asyncio.wait_for(
                self.reader.readexactly(4),
                timeout=self.config.recv_timeout_sec
            )
            length = struct.unpack('<I', length_bytes)[0]

            # Read message body
            body_bytes = await self.reader.readexactly(length)
            message = json.loads(body_bytes.decode('utf-8'))

            return message

        except asyncio.TimeoutError:
            # No message available, that's fine
            return None
        except asyncio.IncompleteReadError:
            # Connection closed
            self.connected = False
            return None
        except Exception as e:
            print(f"Error receiving message: {e}")
            return None

    async def _send_message(self, message: dict):
        """Send a message to the Rust service."""
        if not self.connected or not self.writer:
            raise ConnectionError("Not connected to service")

        # Encode message as JSON
        body = json.dumps(message).encode('utf-8')

        # Create length-prefixed frame
        frame = struct.pack('<I', len(body)) + body

        # Send
        self.writer.write(frame)
        await self.writer.drain()

    def on_feature_batch(self, callback: Callable[[dict], None]):
        """Register a callback for when feature batches are received."""
        self._on_feature_batch = callback

    def on_errp_window(self, callback: Callable[[dict], None]):
        """Register a callback for when ErrP windows are received."""
        self._on_errp_window = callback

    def on_shutdown(self, callback: Callable[[], None]):
        """Register a callback for when shutdown is requested."""
        self._on_shutdown = callback

    async def run_receive_loop(self):
        """Run the message receive loop.

        This continuously receives messages and dispatches them to the
        appropriate callbacks. Run this in a separate task.
        """
        while self.connected:
            message = await self.receive_message()

            if message is None:
                await asyncio.sleep(0.001)  # Brief sleep to avoid busy-waiting
                continue

            msg_type = message.get("type")

            if msg_type == "FeatureBatch" and self._on_feature_batch:
                self._on_feature_batch(message)
            elif msg_type == "ErrPWindow" and self._on_errp_window:
                self._on_errp_window(message)
            elif msg_type == "Shutdown":
                if self._on_shutdown:
                    self._on_shutdown()
                break
            elif msg_type == "Ping":
                # Respond to ping with pong
                await self._send_message({
                    "type": "Pong",
                    "timestamp": message.get("timestamp", 0),
                    "python_timestamp": int(
                        asyncio.get_event_loop().time() * 1_000_000
                    ),
                })


async def main():
    """Simple test of the IPC client."""
    config = IpcConfig()
    client = IpcClient(config)

    print(f"Connecting to {config.host}:{config.port}...")

    if await client.connect():
        print("Connected!")

        # Simple echo test
        for i in range(5):
            await client.send_action(
                mouse_dx=1.0,
                mouse_dy=0.5,
                discrete_action=0,
                confidence=0.8,
                inference_latency_us=5000
            )
            print(f"Sent action {i+1}")
            await asyncio.sleep(0.1)

        await client.disconnect()
        print("Disconnected")
    else:
        print("Failed to connect")


if __name__ == "__main__":
    asyncio.run(main())
