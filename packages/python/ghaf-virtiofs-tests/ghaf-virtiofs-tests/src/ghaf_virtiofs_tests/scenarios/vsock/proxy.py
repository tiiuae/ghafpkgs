# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""vsock proxy command filtering test scenario.

Tests:
- V-001: PING via Proxy (allowed)
- V-002: VERSION via Proxy (allowed)
- V-003: INSTREAM Clean (allowed, returns OK)
- V-004: INSTREAM Infected (allowed, returns FOUND)
- V-005: SCAN Blocked (rejected)
- V-006: SHUTDOWN Blocked (rejected)
- V-007: RELOAD Blocked (rejected)
- V-008: Case Sensitivity (lowercase rejected)
- V-009: CONTSCAN Blocked (rejected)
- V-010: MULTISCAN Blocked (rejected)
- V-011: ALLMATCHSCAN Blocked (rejected)

Requires clamd-vproxy running on host with vsock listener.

Usage:
    # From guest VM:
    ghaf-virtiofs-test run /tmp -s vsock.proxy --write

    # No verify action needed (tests run entirely from guest)
"""

import socket

from ...context import EICAR_STRING, TestContext

# Default vsock settings
VMADDR_CID_HOST = 2
DEFAULT_VSOCK_PORT = 3400


class ProxyScenario:
    """Test vsock proxy command filtering."""

    def __init__(self, cid: int = VMADDR_CID_HOST, port: int = DEFAULT_VSOCK_PORT):
        self.cid = cid
        self.port = port

    def _connect(self) -> socket.socket:
        """Connect to vsock proxy."""
        sock = socket.socket(socket.AF_VSOCK, socket.SOCK_STREAM)
        sock.settimeout(10.0)
        sock.connect((self.cid, self.port))
        return sock

    def _send_command(self, command: bytes) -> bytes:
        """Send command and receive response."""
        sock = self._connect()
        try:
            sock.sendall(command)
            return sock.recv(4096)
        finally:
            sock.close()

    def _send_instream(self, data: bytes) -> bytes:
        """Send INSTREAM command with data."""
        sock = self._connect()
        try:
            # Send INSTREAM command
            sock.sendall(b"zINSTREAM\0")

            # Send data chunk: length (big-endian u32) + data
            length = len(data).to_bytes(4, "big")
            sock.sendall(length + data)

            # Send end marker (4 zero bytes)
            sock.sendall(b"\0\0\0\0")

            return sock.recv(4096)
        finally:
            sock.close()

    def write(self, ctx: TestContext) -> None:
        """Run vsock proxy tests from guest VM."""

        results = {
            "v001_ping": False,
            "v002_version": False,
            "v003_instream_clean": False,
            "v004_instream_infected": False,
            "v005_scan_blocked": False,
            "v006_shutdown_blocked": False,
            "v007_reload_blocked": False,
            "v008_case_sensitive": False,
            "v009_contscan_blocked": False,
            "v010_multiscan_blocked": False,
            "v011_allmatchscan_blocked": False,
        }

        # V-001: PING via Proxy
        print("[V-001] Testing PING...")
        response = self._send_command(b"zPING\0")
        response_str = response.decode().strip().strip("\0")
        if response_str == "PONG":
            results["v001_ping"] = True
            print("  OK: Received PONG")
        else:
            raise AssertionError(f"V-001: Unexpected response: {response_str}")

        # V-002: VERSION via Proxy
        print("\n[V-002] Testing VERSION...")
        response = self._send_command(b"zVERSION\0")
        response_str = response.decode().strip().strip("\0")
        if "ClamAV" in response_str:
            results["v002_version"] = True
            print(f"  OK: {response_str}")
        else:
            raise AssertionError(f"V-002: Unexpected response: {response_str}")

        # V-003: INSTREAM Clean
        print("\n[V-003] Testing INSTREAM with clean data...")
        response = self._send_instream(b"This is clean test content")
        response_str = response.decode().strip().strip("\0")
        if response_str.endswith("OK"):
            results["v003_instream_clean"] = True
            print(f"  OK: {response_str}")
        else:
            raise AssertionError(f"V-003: Unexpected response: {response_str}")

        # V-004: INSTREAM Infected
        print("\n[V-004] Testing INSTREAM with EICAR...")
        response = self._send_instream(EICAR_STRING)
        response_str = response.decode().strip().strip("\0")
        if "FOUND" in response_str:
            results["v004_instream_infected"] = True
            print(f"  OK: {response_str}")
        else:
            raise AssertionError(f"V-004: Unexpected response: {response_str}")

        # V-005: SCAN Blocked
        print("\n[V-005] Testing SCAN (should be blocked)...")
        try:
            response = self._send_command(b"zSCAN /etc/passwd\0")
            response_str = response.decode().strip().strip("\0")
            if (
                "not allowed" in response_str.lower()
                or "rejected" in response_str.lower()
            ):
                results["v005_scan_blocked"] = True
                print(f"  OK: Blocked - {response_str}")
            elif response_str == "":
                # Connection closed = blocked
                results["v005_scan_blocked"] = True
                print("  OK: Connection closed (command blocked)")
            else:
                raise AssertionError(f"V-005: Command not blocked: {response_str}")
        except (ConnectionResetError, BrokenPipeError):
            results["v005_scan_blocked"] = True
            print("  OK: Connection reset (command blocked)")

        # V-006: SHUTDOWN Blocked
        print("\n[V-006] Testing SHUTDOWN (should be blocked)...")
        try:
            response = self._send_command(b"zSHUTDOWN\0")
            response_str = response.decode().strip().strip("\0")
            if (
                "not allowed" in response_str.lower()
                or "rejected" in response_str.lower()
            ):
                results["v006_shutdown_blocked"] = True
                print(f"  OK: Blocked - {response_str}")
            elif response_str == "":
                results["v006_shutdown_blocked"] = True
                print("  OK: Connection closed (command blocked)")
            else:
                raise AssertionError(f"V-006: Command not blocked: {response_str}")
        except (ConnectionResetError, BrokenPipeError):
            results["v006_shutdown_blocked"] = True
            print("  OK: Connection reset (command blocked)")

        # V-007: RELOAD Blocked
        print("\n[V-007] Testing RELOAD (should be blocked)...")
        try:
            response = self._send_command(b"zRELOAD\0")
            response_str = response.decode().strip().strip("\0")
            if (
                "not allowed" in response_str.lower()
                or "rejected" in response_str.lower()
            ):
                results["v007_reload_blocked"] = True
                print(f"  OK: Blocked - {response_str}")
            elif response_str == "":
                results["v007_reload_blocked"] = True
                print("  OK: Connection closed (command blocked)")
            else:
                raise AssertionError(f"V-007: Command not blocked: {response_str}")
        except (ConnectionResetError, BrokenPipeError):
            results["v007_reload_blocked"] = True
            print("  OK: Connection reset (command blocked)")

        # V-008: Case Sensitivity
        print("\n[V-008] Testing lowercase 'zping' (should be rejected)...")
        try:
            response = self._send_command(b"zping\0")
            response_str = response.decode().strip().strip("\0")
            if response_str == "PONG":
                raise AssertionError("V-008: Lowercase accepted (should be rejected)")
            elif (
                "not allowed" in response_str.lower()
                or "rejected" in response_str.lower()
            ):
                results["v008_case_sensitive"] = True
                print(f"  OK: Rejected - {response_str}")
            elif response_str == "":
                results["v008_case_sensitive"] = True
                print("  OK: Connection closed (command rejected)")
            else:
                results["v008_case_sensitive"] = True
                print(f"  OK: Not PONG - {response_str}")
        except (ConnectionResetError, BrokenPipeError):
            results["v008_case_sensitive"] = True
            print("  OK: Connection reset (command rejected)")

        # V-009: CONTSCAN Blocked
        print("\n[V-009] Testing CONTSCAN (should be blocked)...")
        try:
            response = self._send_command(b"zCONTSCAN /etc/passwd\0")
            response_str = response.decode().strip().strip("\0")
            if (
                "not allowed" in response_str.lower()
                or "rejected" in response_str.lower()
            ):
                results["v009_contscan_blocked"] = True
                print(f"  OK: Blocked - {response_str}")
            elif response_str == "":
                results["v009_contscan_blocked"] = True
                print("  OK: Connection closed (command blocked)")
            else:
                raise AssertionError(f"V-009: Command not blocked: {response_str}")
        except (ConnectionResetError, BrokenPipeError):
            results["v009_contscan_blocked"] = True
            print("  OK: Connection reset (command blocked)")

        # V-010: MULTISCAN Blocked
        print("\n[V-010] Testing MULTISCAN (should be blocked)...")
        try:
            response = self._send_command(b"zMULTISCAN /etc/passwd\0")
            response_str = response.decode().strip().strip("\0")
            if (
                "not allowed" in response_str.lower()
                or "rejected" in response_str.lower()
            ):
                results["v010_multiscan_blocked"] = True
                print(f"  OK: Blocked - {response_str}")
            elif response_str == "":
                results["v010_multiscan_blocked"] = True
                print("  OK: Connection closed (command blocked)")
            else:
                raise AssertionError(f"V-010: Command not blocked: {response_str}")
        except (ConnectionResetError, BrokenPipeError):
            results["v010_multiscan_blocked"] = True
            print("  OK: Connection reset (command blocked)")

        # V-011: ALLMATCHSCAN Blocked
        print("\n[V-011] Testing ALLMATCHSCAN (should be blocked)...")
        try:
            response = self._send_command(b"zALLMATCHSCAN /etc/passwd\0")
            response_str = response.decode().strip().strip("\0")
            if (
                "not allowed" in response_str.lower()
                or "rejected" in response_str.lower()
            ):
                results["v011_allmatchscan_blocked"] = True
                print(f"  OK: Blocked - {response_str}")
            elif response_str == "":
                results["v011_allmatchscan_blocked"] = True
                print("  OK: Connection closed (command blocked)")
            else:
                raise AssertionError(f"V-011: Command not blocked: {response_str}")
        except (ConnectionResetError, BrokenPipeError):
            results["v011_allmatchscan_blocked"] = True
            print("  OK: Connection reset (command blocked)")

        # Summary
        print("\n" + "=" * 50)
        print("VSOCK PROXY TEST RESULTS")
        print("=" * 50)
        for test, passed in results.items():
            print(f"  {test}: {'PASS' if passed else 'FAIL'}")

        passed = sum(results.values())
        total = len(results)
        print(f"\n{passed}/{total} tests passed")

        if not all(results.values()):
            failed = [k for k, v in results.items() if not v]
            raise AssertionError(f"vsock proxy tests failed: {failed}")

    def verify(self, ctx: TestContext) -> None:
        """No verification needed - tests run entirely from guest."""
        print("vsock.proxy tests run from guest only (use --write)")
        print("No host verification needed.")
