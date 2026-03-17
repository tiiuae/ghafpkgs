# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

"""Performance metrics collection and reporting."""

import json
import statistics
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass
class Timer:
    """Context manager for timing operations."""

    name: str
    start_time: float = 0.0
    end_time: float = 0.0

    def __enter__(self) -> "Timer":
        self.start_time = time.monotonic()
        return self

    def __exit__(self, *args: Any) -> None:
        self.end_time = time.monotonic()

    @property
    def elapsed_ms(self) -> float:
        """Elapsed time in milliseconds."""
        return (self.end_time - self.start_time) * 1000


@dataclass
class MetricsCollector:
    """Collect and report performance metrics."""

    role: str
    scenario: str
    metrics: dict[str, list[float]] = field(default_factory=dict)
    metadata: dict[str, Any] = field(default_factory=dict)

    def record(self, name: str, value: float) -> None:
        """Record a metric value.

        Args:
            name: Metric name (e.g., "sync_latency_ms", "throughput_mbps")
            value: Metric value
        """
        if name not in self.metrics:
            self.metrics[name] = []
        self.metrics[name].append(value)

    def timer(self, name: str) -> Timer:
        """Create a timer context manager that records elapsed time.

        Usage:
            with metrics.timer("operation_time_ms") as t:
                do_operation()
            # Automatically records t.elapsed_ms
        """
        return _RecordingTimer(name, self)

    def set_metadata(self, key: str, value: Any) -> None:
        """Set metadata about the test run.

        Args:
            key: Metadata key (e.g., "file_size", "file_count")
            value: Metadata value
        """
        self.metadata[key] = value

    def summary(self) -> dict[str, Any]:
        """Generate summary statistics for all metrics.

        Returns:
            Dictionary with min/max/mean/median/stdev for each metric
        """
        result: dict[str, Any] = {
            "role": self.role,
            "scenario": self.scenario,
            "metadata": self.metadata,
            "metrics": {},
        }

        for name, values in self.metrics.items():
            if not values:
                continue

            stats: dict[str, float] = {
                "count": len(values),
                "min": min(values),
                "max": max(values),
                "mean": statistics.mean(values),
            }

            if len(values) >= 2:
                stats["median"] = statistics.median(values)
                stats["stdev"] = statistics.stdev(values)

            result["metrics"][name] = stats

        return result

    def report(self) -> str:
        """Generate human-readable report.

        Returns:
            Formatted report string
        """
        summary = self.summary()
        lines = [
            f"Performance Report: {self.scenario} ({self.role})",
            "=" * 60,
        ]

        if self.metadata:
            lines.append("Metadata:")
            for key, value in self.metadata.items():
                lines.append(f"  {key}: {value}")
            lines.append("")

        if summary["metrics"]:
            lines.append("Metrics:")
            for name, stats in summary["metrics"].items():
                if stats["count"] == 1:
                    lines.append(f"  {name}: {stats['mean']:.3f}")
                else:
                    line = f"  {name}: {stats['mean']:.3f} (n={stats['count']}"
                    if "stdev" in stats:
                        line += f", stdev={stats['stdev']:.3f}"
                    line += ")"
                    lines.append(line)

        return "\n".join(lines)

    def save(self, path: Path) -> None:
        """Save metrics to JSON file.

        Args:
            path: Output file path
        """
        path.write_text(json.dumps(self.summary(), indent=2))


class _RecordingTimer(Timer):
    """Timer that automatically records elapsed time to a MetricsCollector."""

    def __init__(self, name: str, collector: MetricsCollector) -> None:
        super().__init__(name)
        self.collector = collector

    def __exit__(self, *args: Any) -> None:
        super().__exit__(*args)
        self.collector.record(self.name, self.elapsed_ms)
