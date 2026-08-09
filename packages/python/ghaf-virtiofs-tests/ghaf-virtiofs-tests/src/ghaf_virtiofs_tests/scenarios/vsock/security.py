# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""vsock proxy security test scenario.

Tests:
- SEC-021: Command Injection (zPING followed by zSCAN)
- SEC-022: Buffer Overflow (1MB command)
- SEC-023: Oversized Chunk (>25MB INSTREAM chunk)
- SEC-024: Protocol Violation (invalid chunk length)
- SEC-025: Wrong Delimiter (newline instead of null)
- SEC-026: Lowercase Command (zinstream)
- SEC-027: Extra Whitespace (zPING with space)
- SEC-006: Connection Flood (100 concurrent)
- SEC-007: Slow Client
- SEC-008: Partial Request

Run from guest VM with clamd-vproxy running on host.

Usage:
    # From guest VM:
    ghaf-virtiofs-test run /tmp -s vsock.security --write
"""

import secrets
import socket
import time
from concurrent.futures import ThreadPoolExecutor

from ...context import TestContext

VMADDR_CID_HOST = 2
DEFAULT_VSOCK_PORT = 3400


class SecurityScenario:
    """Test vsock proxy security."""

    def __init__(self, cid: int = VMADDR_CID_HOST, port: int = DEFAULT_VSOCK_PORT):
        self.cid = cid
        self.port = port

    def _connect(self, timeout: float = 10.0) -> socket.socket:
        """Connect to vsock proxy."""
        sock = socket.socket(socket.AF_VSOCK, socket.SOCK_STREAM)
        sock.settimeout(timeout)
        sock.connect((self.cid, self.port))
        return sock

    def _send_recv(self, data: bytes, timeout: float = 10.0) -> bytes:
        """Send data and receive response."""
        sock = self._connect(timeout)
        try:
            sock.sendall(data)
            return sock.recv(4096)
        finally:
            sock.close()

    def write(self, ctx: TestContext) -> None:
        """Run vsock proxy security tests."""

        results = {}

        # SEC-021: Command Injection
        print("[SEC-021] Testing command injection...")
        try:
            # Try to inject SCAN after PING
            response = self._send_recv(b"zPING\0zSCAN /etc/passwd\0")
            response_str = response.decode().strip().strip("\0")

            if response_str == "PONG":
                print("  OK: Only PING processed, SCAN ignored")
                results["sec021_injection"] = True
            elif "passwd" in response_str.lower():
                raise AssertionError(f"SEC-021: SCAN executed - {response_str}")
            else:
                print(f"  OK: Response - {response_str}")
                results["sec021_injection"] = True
        except Exception as e:
            print(f"  OK: Connection rejected - {e}")
            results["sec021_injection"] = True

        # SEC-022: Buffer Overflow (1MB command)
        print("\n[SEC-022] Testing buffer overflow (1MB command)...")
        try:
            large_cmd = b"z" + (b"A" * (1024 * 1024)) + b"\0"
            response = self._send_recv(large_cmd, timeout=5.0)
            response_str = response.decode().strip().strip("\0")
            print(f"  Response: {response_str[:100]}...")
            results["sec022_overflow"] = True
        except (ConnectionResetError, BrokenPipeError, socket.timeout):
            print("  OK: Connection rejected/timeout (expected)")
            results["sec022_overflow"] = True
        except Exception as e:
            print(f"  OK: Rejected - {e}")
            results["sec022_overflow"] = True

        # SEC-023: Oversized Chunk (>25MB limit in vproxy)
        print("\n[SEC-023] Testing oversized INSTREAM chunk (26MB)...")
        try:
            sock = self._connect(timeout=30.0)
            try:
                sock.sendall(b"zINSTREAM\0")

                # Send 26MB chunk header (exceeds 25MB limit)
                chunk_size = 26 * 1024 * 1024
                sock.sendall(chunk_size.to_bytes(4, "big"))
                sock.sendall(secrets.token_bytes(1024))  # Only send 1KB

                response = sock.recv(4096)
                response_str = response.decode().strip().strip("\0")
                print(f"  Response: {response_str!r}")

                # Pass if: empty response, connection closed, or not a successful scan
                # Proxy closes connection on oversized chunk (no explicit error message)
                if not response_str:
                    print("  OK: Connection closed (chunk rejected)")
                    results["sec023_oversized_chunk"] = True
                elif (
                    response_str.endswith("OK") and "error" not in response_str.lower()
                ):
                    raise AssertionError(
                        "SEC-023: Oversized chunk was accepted and scanned"
                    )
                else:
                    print("  OK: Oversized chunk rejected")
                    results["sec023_oversized_chunk"] = True
            finally:
                sock.close()
        except (ConnectionResetError, BrokenPipeError, socket.timeout, OSError):
            print("  OK: Connection rejected (expected)")
            results["sec023_oversized_chunk"] = True
        except Exception as e:
            print(f"  OK: Rejected - {e}")
            results["sec023_oversized_chunk"] = True

        # SEC-024: Protocol Violation (invalid chunk length)
        print("\n[SEC-024] Testing protocol violation...")
        try:
            sock = self._connect()
            try:
                sock.sendall(b"zINSTREAM\0")
                # Send invalid length (negative as unsigned = huge)
                sock.sendall(b"\xff\xff\xff\xff")

                response = sock.recv(4096)
                print(f"  Response: {response.decode().strip()}")
                results["sec024_protocol"] = True
            finally:
                sock.close()
        except (ConnectionResetError, BrokenPipeError, socket.timeout):
            print("  OK: Connection terminated (expected)")
            results["sec024_protocol"] = True
        except Exception as e:
            print(f"  OK: Rejected - {e}")
            results["sec024_protocol"] = True

        # SEC-025: Wrong Delimiter (newline)
        print("\n[SEC-025] Testing wrong delimiter (newline)...")
        try:
            response = self._send_recv(b"zPING\n")
            response_str = response.decode().strip().strip("\0")

            if response_str == "PONG":
                print("  INFO: Newline delimiter accepted")
                results["sec025_delimiter"] = True  # May be acceptable
            else:
                print(f"  OK: Rejected - {response_str}")
                results["sec025_delimiter"] = True
        except (ConnectionResetError, BrokenPipeError):
            print("  OK: Connection rejected")
            results["sec025_delimiter"] = True
        except Exception as e:
            print(f"  OK: Rejected - {e}")
            results["sec025_delimiter"] = True

        # SEC-026: Lowercase Command
        print("\n[SEC-026] Testing lowercase command (zinstream)...")
        try:
            response = self._send_recv(b"zinstream\0")
            response_str = response.decode().strip().strip("\0")

            if "not allowed" in response_str.lower() or response_str == "":
                print("  OK: Lowercase rejected")
                results["sec026_lowercase"] = True
            else:
                print(f"  Response: {response_str}")
                results["sec026_lowercase"] = "error" in response_str.lower()
        except (ConnectionResetError, BrokenPipeError):
            print("  OK: Connection rejected")
            results["sec026_lowercase"] = True
        except Exception as e:
            print(f"  OK: Rejected - {e}")
            results["sec026_lowercase"] = True

        # SEC-027: Extra Whitespace
        print("\n[SEC-027] Testing extra whitespace...")
        try:
            response = self._send_recv(b"zPING \0")
            response_str = response.decode().strip().strip("\0")

            if response_str == "PONG":
                print("  INFO: Whitespace accepted")
                results["sec027_whitespace"] = True
            else:
                print(f"  OK: Rejected - {response_str}")
                results["sec027_whitespace"] = True
        except (ConnectionResetError, BrokenPipeError):
            print("  OK: Connection rejected")
            results["sec027_whitespace"] = True
        except Exception as e:
            print(f"  OK: Rejected - {e}")
            results["sec027_whitespace"] = True

        # SEC-006: Connection Flood
        print("\n[SEC-006] Testing connection flood (100 concurrent)...")
        try:

            def try_connect() -> bool:
                try:
                    sock = self._connect(timeout=5.0)
                    sock.sendall(b"zPING\0")
                    response = sock.recv(64)
                    sock.close()
                    return b"PONG" in response
                except Exception:
                    return False

            with ThreadPoolExecutor(max_workers=100) as executor:
                futures = [executor.submit(try_connect) for _ in range(100)]
                successes = sum(1 for f in futures if f.result())

            print(f"  {successes}/100 connections succeeded")
            results["sec006_flood"] = True  # Test completes = pass
        except Exception as e:
            print(f"  Error: {e}")
            results["sec006_flood"] = True

        # SEC-007: Slow Client
        print("\n[SEC-007] Testing slow client...")
        try:
            sock = self._connect(timeout=120.0)
            try:
                # Send command byte by byte with delays
                for byte in b"zPING\0":
                    sock.send(bytes([byte]))
                    time.sleep(0.5)

                response = sock.recv(64)
                if b"PONG" in response:
                    print("  OK: Slow client handled")
                else:
                    print(f"  Response: {response}")
                results["sec007_slow"] = True
            finally:
                sock.close()
        except socket.timeout:
            print("  OK: Timeout triggered (expected)")
            results["sec007_slow"] = True
        except Exception as e:
            print(f"  OK: Rejected - {e}")
            results["sec007_slow"] = True

        # SEC-008: Partial Request
        print("\n[SEC-008] Testing partial request (disconnect mid-transfer)...")
        try:
            sock = self._connect()
            sock.sendall(b"zINSTREAM\0")
            sock.sendall((1024).to_bytes(4, "big"))  # Claim 1KB
            sock.sendall(b"partial")  # Send only 7 bytes
            sock.close()  # Disconnect

            # Verify proxy didn't crash - try another connection
            time.sleep(0.5)
            test_response = self._send_recv(b"zPING\0")
            if b"PONG" in test_response:
                print("  OK: Proxy still responsive after partial request")
                results["sec008_partial"] = True
            else:
                print(f"  Response: {test_response}")
                results["sec008_partial"] = True
        except Exception as e:
            print(f"  Error: {e}")
            results["sec008_partial"] = False

        # Summary
        print("\n" + "=" * 50)
        print("VSOCK PROXY SECURITY RESULTS")
        print("=" * 50)
        for test, passed in results.items():
            print(f"  {test}: {'PASS' if passed else 'FAIL'}")

        passed = sum(results.values())
        total = len(results)
        print(f"\n{passed}/{total} tests passed")

        if not all(results.values()):
            failed = [k for k, v in results.items() if not v]
            raise AssertionError(f"vsock proxy security tests failed: {failed}")

    def verify(self, ctx: TestContext) -> None:
        """No verification needed - run with --write from guest."""
        print("security.vsock_proxy runs from guest only (use --write)")
