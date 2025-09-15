# Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

import json
import socket
import logging
from collections.abc import Generator
from typing import Any

logger = logging.getLogger("upm")


def send(sock: socket.socket, obj: dict[str, Any]) -> None:
    data = (json.dumps(obj, separators=(",", ":")) + "\n").encode("utf-8")
    sock.sendall(data)


def receive(sock: socket.socket) -> Generator[dict[str, Any], None, None]:
    buf = b""
    while True:
        chunk = sock.recv(4096)
        logger.debug(f"JSON received a chunk {chunk}...parsing...")
        if not chunk:
            break
        buf += chunk
        while b"\n" in buf:
            line, buf = buf.split(b"\n", 1)
            line = line.strip()
            logger.debug(f"JSON  parsing line {line}")
            if line:
                yield json.loads(line.decode("utf-8"))
        logger.debug("JSON parsing done.")
