# Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

from abc import ABC, abstractmethod
from typing import Any, Callable, Tuple
import threading
import socket
import time

import vsock_bridge.json_transport as json_transport
from vsock_bridge.logger import logger, log_entry_exit

AF_VSOCK = getattr(socket, "AF_VSOCK", None)
SOCK_STREAM = socket.SOCK_STREAM

__all__ = ["VsockServer", "VsockConnectionHandler", "ConnCtx", "VsockClient"]


class ConnCtx:
    def __init__(
        self,
        peer: Tuple[int, int],
        send_func: Callable[[dict[str, Any]], bool],
        close_func: Callable[[], None],
    ):
        self.peer_cid, self.peer_port = peer
        self._send = send_func
        self._close = close_func

    @log_entry_exit
    def send(self, data: dict[str, Any]) -> bool:
        """Send a JSON message to this client; True on success."""
        return self._send(data)

    def close(self) -> None:
        """Close this connection."""
        self._close()


class VsockConnectionHandler(ABC):
    @abstractmethod
    def on_connect(self, ctx: ConnCtx) -> None: ...

    @abstractmethod
    def on_message(self, msg: dict[str, Any]) -> None: ...

    @abstractmethod
    def on_disconnect(self) -> None: ...


class VsockConnectionWorker(threading.Thread):
    def __init__(
        self,
        conn: socket.socket,
        addr: Tuple[int, int],
        handler: VsockConnectionHandler,
        name: str | None = None,
    ):
        super().__init__(daemon=True, name=name)
        self._conn = conn
        self._peer = addr
        self._handler = handler
        self._stop = threading.Event()
        self._send_lock = threading.Lock()

        # build the per-connection context
        def _send(data: dict[str, Any]) -> bool:
            try:
                with self._send_lock:
                    json_transport.send(self._conn, data)
                return True
            except Exception:
                return False

        def _close() -> None:
            self.stop()

        self._ctx = ConnCtx(self._peer, _send, _close)

    @log_entry_exit
    def run(self) -> None:
        try:
            self._handler.on_connect(self._ctx)

            for msg in json_transport.receive(self._conn):
                if self._stop.is_set():
                    break
                self._handler.on_message(msg)

        except OSError:
            pass
        finally:
            try:
                self._conn.close()
            except Exception:
                pass
            # always notify disconnect once
            try:
                self._handler.on_disconnect()
            except Exception:
                pass

    @log_entry_exit
    def stop(self) -> None:
        if self._stop.is_set():
            return
        self._stop.set()
        try:
            self._conn.shutdown(socket.SHUT_RDWR)
        except Exception:
            pass
        try:
            self._conn.close()
        except Exception:
            pass


class VsockServer(threading.Thread):
    def __init__(
        self, cid: int, port: int, max_connections: int, name: str | None = None
    ):
        super().__init__(daemon=True, name=name or f"VsockServer:{cid}:{port}")
        self._cid = cid
        self._port = port
        self._handlers_by_cid = {}
        self._sock = socket.socket(socket.AF_VSOCK, socket.SOCK_STREAM)
        self._sock.bind((cid, port))
        self._sock.listen(max_connections)
        self._stop = threading.Event()
        self._workers_lock = threading.Lock()
        self._workers: dict[tuple[int, int], VsockConnectionWorker] = {}

    @log_entry_exit
    def register_handler(self, cid: int, handler: VsockConnectionHandler) -> bool:
        if cid in self._handlers_by_cid:
            return False
        self._handlers_by_cid[cid] = handler
        return True

    @log_entry_exit
    def run(self) -> None:
        while not self._stop.is_set():
            try:
                conn, addr = self._sock.accept()  # addr = (peer_cid, peer_port)
            except OSError:
                break
            peer_cid, peer_port = addr
            handler = self._handlers_by_cid.get(peer_cid)
            if not handler:
                conn.close()
                continue
            w = VsockConnectionWorker(
                conn, addr, handler, name=f"VsockWorker:{peer_cid}:{peer_port}"
            )
            with self._workers_lock:
                self._workers[addr] = w
            w.start()

    @log_entry_exit
    def stop(self) -> None:
        if self._stop.is_set():
            return
        self._stop.set()
        try:
            self._sock.shutdown(socket.SHUT_RDWR)
        except Exception:
            pass
        try:
            self._sock.close()
        except Exception:
            pass
        with self._workers_lock:
            workers = list(self._workers.values())
        for w in workers:
            w.stop()
            w.join(timeout=2)


class VsockClient(threading.Thread):
    def __init__(
        self,
        on_message: Callable[[dict[str, Any]], None],
        on_connect: Callable[[], None],
        on_disconnect: Callable[[], None],
        cid: int,
        port: int,
    ):
        super().__init__(daemon=True)
        self.on_message = on_message
        self.on_connect = on_connect
        self.on_disconnect = on_disconnect
        self.conn = None
        self.stop_flag = threading.Event()
        self.lock = threading.Lock()
        self.port = port
        self.cid = cid

    @log_entry_exit
    def server(self) -> socket.socket:
        with self.lock:
            if self.conn is None:
                self.conn = socket.socket(socket.AF_VSOCK, socket.SOCK_STREAM)
                while True:
                    try:
                        # Wait for server,
                        # TODO: better to start as socket unit
                        logger.info(
                            f"Waiting for server. cid:{self.cid}, port:{self.port}"
                        )
                        self.conn.connect((self.cid, self.port))
                        break
                    except OSError:
                        time.sleep(1)
            return self.conn

    @log_entry_exit
    def close_connection(self):
        with self.lock:
            if self.conn is not None:
                self.conn.close()
                self.conn = None
                self.on_disconnect()

    @log_entry_exit
    def run(self):
        while not self.stop_flag.is_set():
            try:
                for msg in json_transport.receive(self.server()):
                    self.on_message(msg)
            except OSError as err:
                logger.error(f"VSOCK server error: {err}")
            self.close_connection()

    @log_entry_exit
    def send(self, data: dict[str, Any]) -> bool:
        for _ in range(5):
            try:
                json_transport.send(self.server(), data)
                return True
            except Exception:
                logger.error("Vsock server error, send failed! Retrying...")
                self.close_connection()
                continue
        return False

    @log_entry_exit
    def stop(self):
        self.stop_flag.set()
        time.sleep(1)
        try:
            self.close_connection()
        except Exception:
            pass
