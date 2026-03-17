# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""vsock proxy performance measurement scenario.

Tests:
- P-010: INSTREAM round-trip latency via vsock
- P-011: Large file scan throughput via vsock
- P-012: Concurrent scan requests

Run from guest VM with clamd-vproxy running on host.

Usage:
    # From guest VM:
    ghaf-virtiofs-test run /tmp -s vsock.performance --write
"""

import secrets
import socket
import time
from concurrent.futures import ThreadPoolExecutor

from ...context import TestContext

VMADDR_CID_HOST = 2
DEFAULT_VSOCK_PORT = 3400
CHUNK_SIZE = 10 * 1024 * 1024  # 10MB chunks for large data

FILE_SIZES = [
    ("1KB", 1024),
    ("1MB", 1024 * 1024),
    ("10MB", 10 * 1024 * 1024),
    ("100MB", 100 * 1024 * 1024),
]

CONCURRENT_REQUESTS = 10


class PerformanceScenario:
    """Measure vsock proxy performance."""

    def __init__(self, cid: int = VMADDR_CID_HOST, port: int = DEFAULT_VSOCK_PORT):
        self.cid = cid
        self.port = port

    def _send_instream(self, data: bytes) -> tuple[str, float]:
        """Send INSTREAM and return (response, time_ms)."""
        sock = socket.socket(socket.AF_VSOCK, socket.SOCK_STREAM)
        sock.settimeout(120.0)
        sock.connect((self.cid, self.port))

        try:
            start = time.monotonic()

            # Send INSTREAM command
            sock.sendall(b"zINSTREAM\0")

            # Send data chunk
            length = len(data).to_bytes(4, "big")
            sock.sendall(length + data)

            # End marker
            sock.sendall(b"\0\0\0\0")

            # Read response
            response = sock.recv(4096).decode().strip().strip("\0")
            elapsed_ms = (time.monotonic() - start) * 1000

            return response, elapsed_ms
        finally:
            sock.close()

    def _send_instream_chunked(self, size_bytes: int) -> tuple[str, float]:
        """Send INSTREAM with random data in chunks (avoids OOM)."""
        sock = socket.socket(socket.AF_VSOCK, socket.SOCK_STREAM)
        sock.settimeout(300.0)  # Longer timeout for large files
        sock.connect((self.cid, self.port))

        try:
            start = time.monotonic()

            # Send INSTREAM command
            sock.sendall(b"zINSTREAM\0")

            # Send data in chunks
            remaining = size_bytes
            while remaining > 0:
                chunk_size = min(CHUNK_SIZE, remaining)
                chunk_data = secrets.token_bytes(chunk_size)
                sock.sendall(chunk_size.to_bytes(4, "big"))
                sock.sendall(chunk_data)
                remaining -= chunk_size

            # End marker
            sock.sendall(b"\0\0\0\0")

            # Read response
            response = sock.recv(4096).decode().strip().strip("\0")
            elapsed_ms = (time.monotonic() - start) * 1000

            return response, elapsed_ms
        finally:
            sock.close()

    def write(self, ctx: TestContext) -> None:
        """Run vsock proxy performance tests."""
        ctx.metrics.set_metadata("test_type", "vsock_performance")

        # P-010: Latency test (small data, multiple iterations)
        print("[P-010] Measuring INSTREAM latency (1KB, 10 iterations)...")
        latencies = []
        test_data = secrets.token_bytes(1024)

        for i in range(10):
            response, ms = self._send_instream(test_data)
            latencies.append(ms)
            if not response.endswith("OK"):
                print(f"  Warning: Unexpected response: {response}")

        avg_latency = sum(latencies) / len(latencies)
        min_latency = min(latencies)
        max_latency = max(latencies)

        ctx.metrics.record("p010_latency_avg_ms", avg_latency)
        ctx.metrics.record("p010_latency_min_ms", min_latency)
        ctx.metrics.record("p010_latency_max_ms", max_latency)
        print(
            f"  Avg: {avg_latency:.2f}ms, Min: {min_latency:.2f}ms, Max: {max_latency:.2f}ms"
        )

        # P-011: Throughput test (various sizes)
        print("\n[P-011] Measuring throughput for various sizes...")
        for size_name, size_bytes in FILE_SIZES:
            try:
                # Use chunked sending for large files to avoid OOM
                if size_bytes > CHUNK_SIZE:
                    response, ms = self._send_instream_chunked(size_bytes)
                else:
                    test_data = secrets.token_bytes(size_bytes)
                    response, ms = self._send_instream(test_data)

                throughput_mbps = (size_bytes / (1024 * 1024)) / (ms / 1000)

                ctx.metrics.record(f"p011_throughput_{size_name}_mbps", throughput_mbps)
                ctx.metrics.record(f"p011_time_{size_name}_ms", ms)

                status = "OK" if response.endswith("OK") else response
                print(
                    f"  {size_name}: {ms:.2f}ms ({throughput_mbps:.2f} MB/s) - {status}"
                )
            except (BrokenPipeError, ConnectionResetError) as e:
                print(f"  {size_name}: FAILED - {e} (stream limit exceeded?)")

        # P-012: Concurrent requests
        print(
            f"\n[P-012] Testing {CONCURRENT_REQUESTS} concurrent INSTREAM requests..."
        )
        test_data = secrets.token_bytes(1024 * 100)  # 100KB each

        def do_scan() -> float:
            _, ms = self._send_instream(test_data)
            return ms

        start = time.monotonic()
        with ThreadPoolExecutor(max_workers=CONCURRENT_REQUESTS) as executor:
            futures = [executor.submit(do_scan) for _ in range(CONCURRENT_REQUESTS)]
            times = [f.result() for f in futures]
        total_ms = (time.monotonic() - start) * 1000

        avg_concurrent = sum(times) / len(times)
        ctx.metrics.record("p012_concurrent_total_ms", total_ms)
        ctx.metrics.record("p012_concurrent_avg_ms", avg_concurrent)
        print(f"  Total: {total_ms:.2f}ms, Avg per request: {avg_concurrent:.2f}ms")

        # Summary
        print("\n" + "=" * 50)
        print("VSOCK PROXY PERFORMANCE RESULTS")
        print("=" * 50)
        print(f"  [P-010] Latency (1KB): {avg_latency:.2f}ms avg")
        throughput_10mb = ctx.metrics.metrics.get("p011_throughput_10MB_mbps", [])
        if throughput_10mb:
            print(f"  [P-011] Throughput (10MB): {throughput_10mb[0]:.2f} MB/s")
        else:
            print("  [P-011] Throughput (10MB): N/A")
        print(f"  [P-012] Concurrent ({CONCURRENT_REQUESTS}x): {total_ms:.2f}ms total")

    def verify(self, ctx: TestContext) -> None:
        """No verification needed - run with --write from guest."""
        print("vsock.performance runs from guest only (use --write)")
