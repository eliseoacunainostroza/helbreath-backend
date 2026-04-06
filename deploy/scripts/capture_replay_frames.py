#!/usr/bin/env python3
"""
TCP proxy that captures client->gateway framed packets into replay_frames.bin.

Frame format expected by Helbreath gateway:
- u16 length (LE), includes opcode + payload
- u16 opcode (LE)
- payload

Usage example:
  python3 deploy/scripts/capture_replay_frames.py \
    --listen 127.0.0.1:3848 \
    --upstream 127.0.0.1:2848 \
    --output crates/net/tests/fixtures/replay_frames.bin

Then point the game client to 127.0.0.1:3848.
"""

from __future__ import annotations

import argparse
import asyncio
from pathlib import Path
from typing import Tuple


def parse_bind(raw: str) -> Tuple[str, int]:
    text = raw.strip()
    if ":" not in text:
        raise ValueError(f"invalid bind: {raw}")
    host, port = text.rsplit(":", 1)
    return host.strip(), int(port.strip())


class FrameCollector:
    def __init__(self, output: Path):
        self.output = output
        self.buffer = bytearray()
        self.frames = 0
        self.bytes = 0
        self._fp = output.open("wb")

    def feed(self, chunk: bytes) -> None:
        self.buffer.extend(chunk)
        while True:
            if len(self.buffer) < 2:
                return
            length = int.from_bytes(self.buffer[0:2], "little")
            total = 2 + length
            if total > 65537:
                # malformed stream, drop one byte and continue scanning
                del self.buffer[0]
                continue
            if len(self.buffer) < total:
                return
            frame = bytes(self.buffer[:total])
            del self.buffer[:total]
            self._fp.write(frame)
            self.frames += 1
            self.bytes += len(frame)

    def close(self) -> None:
        self._fp.flush()
        self._fp.close()


async def pipe_stream(
    reader: asyncio.StreamReader,
    writer: asyncio.StreamWriter,
    collector: FrameCollector | None = None,
) -> None:
    try:
        while True:
            data = await reader.read(4096)
            if not data:
                break
            if collector is not None:
                collector.feed(data)
            writer.write(data)
            await writer.drain()
    finally:
        try:
            writer.close()
            await writer.wait_closed()
        except Exception:
            pass


async def handle_client(
    client_reader: asyncio.StreamReader,
    client_writer: asyncio.StreamWriter,
    upstream_host: str,
    upstream_port: int,
    collector: FrameCollector,
) -> None:
    upstream_reader, upstream_writer = await asyncio.open_connection(upstream_host, upstream_port)
    client_addr = client_writer.get_extra_info("peername")
    print(f"[capture] client connected: {client_addr}")
    try:
        await asyncio.gather(
            pipe_stream(client_reader, upstream_writer, collector=collector),  # client -> gateway
            pipe_stream(upstream_reader, client_writer, collector=None),  # gateway -> client
        )
    finally:
        print("[capture] client disconnected")


async def main_async() -> int:
    parser = argparse.ArgumentParser(description="Capture replay_frames.bin using a TCP proxy")
    parser.add_argument("--listen", default="127.0.0.1:3848", help="local bind for client")
    parser.add_argument("--upstream", default="127.0.0.1:2848", help="gateway tcp bind")
    parser.add_argument(
        "--output",
        default="crates/net/tests/fixtures/replay_frames.bin",
        help="output replay binary file",
    )
    args = parser.parse_args()

    listen_host, listen_port = parse_bind(args.listen)
    upstream_host, upstream_port = parse_bind(args.upstream)
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    collector = FrameCollector(output)

    async def _handler(reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
        await handle_client(reader, writer, upstream_host, upstream_port, collector)

    server = await asyncio.start_server(_handler, listen_host, listen_port)
    print(f"[capture] listening on {listen_host}:{listen_port}")
    print(f"[capture] forwarding to {upstream_host}:{upstream_port}")
    print(f"[capture] writing frames to {output}")
    print("[capture] press Ctrl+C when done")

    try:
        async with server:
            await server.serve_forever()
    except asyncio.CancelledError:
        pass
    finally:
        collector.close()
        print(f"[capture] done: frames={collector.frames} bytes={collector.bytes} file={output}")
        if collector.buffer:
            print(
                f"[capture] warning: {len(collector.buffer)} trailing bytes could not be framed"
            )
    return 0


def main() -> int:
    try:
        return asyncio.run(main_async())
    except KeyboardInterrupt:
        return 0


if __name__ == "__main__":
    raise SystemExit(main())
