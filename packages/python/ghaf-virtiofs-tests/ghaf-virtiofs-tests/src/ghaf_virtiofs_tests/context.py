# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""Test context providing paths and metrics."""

import secrets
import time
from dataclasses import dataclass
from pathlib import Path

from .metrics import MetricsCollector

# Prefix for all test files - used for cleanup
TEST_FILE_PREFIX = "_GVTT-TESTFILE_"

# EICAR standard anti-virus test string
# This is a harmless test signature recognized by all AV engines
EICAR_STRING = b"X5O!P%@AP[4\\PZX54(P^)7CC)7}$EICAR-STANDARD-ANTIVIRUS-TEST-FILE!$H+H*"


@dataclass
class TestContext:
    """Context for running virtiofs integration tests.

    Provides:
    - Path to share mount (VM) or basePath (host)
    - Metrics collector for performance data
    - Helper methods for common operations
    """

    role: str  # hostname
    scenario: str
    path: Path
    source_vm: str | None = None
    _metrics: MetricsCollector | None = None

    def __post_init__(self) -> None:
        self.path = Path(self.path)

    @property
    def metrics(self) -> MetricsCollector:
        """Get metrics collector (lazy initialization)."""
        if self._metrics is None:
            self._metrics = MetricsCollector(
                role=self.role,
                scenario=self.scenario,
            )
        return self._metrics

    def create_test_file(
        self,
        name: str,
        size_bytes: int = 1024,
        content: bytes | None = None,
    ) -> Path:
        """Create a test file with random or specified content.

        Args:
            name: Filename (relative to path)
            size_bytes: Size in bytes (ignored if content provided)
            content: Specific content to write

        Returns:
            Path to created file
        """
        file_path = self.path / name
        file_path.parent.mkdir(parents=True, exist_ok=True)

        if content is not None:
            file_path.write_bytes(content)
        else:
            file_path.write_bytes(secrets.token_bytes(size_bytes))

        return file_path

    def wait_for_file(
        self,
        name: str,
        timeout: float = 10.0,
        poll_interval: float = 0.1,
    ) -> Path:
        """Wait for a file to appear.

        Args:
            name: Filename (relative to path)
            timeout: Maximum wait time in seconds
            poll_interval: Time between checks

        Returns:
            Path to the file

        Raises:
            TimeoutError: If file doesn't appear within timeout
        """
        file_path = self.path / name
        start = time.monotonic()

        while not file_path.exists():
            if time.monotonic() - start > timeout:
                raise TimeoutError(
                    f"Timeout waiting for file '{name}' "
                    f"(waited {timeout}s, path={file_path})"
                )
            time.sleep(poll_interval)

        return file_path

    def file_exists(self, name: str) -> bool:
        """Check if a file exists."""
        return (self.path / name).exists()

    def read_file(self, name: str) -> bytes:
        """Read file content."""
        return (self.path / name).read_bytes()
